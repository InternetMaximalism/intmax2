use alloy::primitives::B256;
use anyhow::Context;
use chrono::{Local, TimeZone as _, Utc};
use intmax2_client_sdk::{
    client::{
        client::Client,
        key_from_eth::generate_intmax_account_from_eth_key,
        strategy::mining::{Mining, MiningStatus},
    },
    external_api::{
        contract::{
            convert::{convert_address_to_intmax, convert_u256_to_intmax},
            utils::get_address_from_private_key,
        },
        utils::time::sleep_for,
    },
};
use intmax2_interfaces::api::withdrawal_server::interface::ClaimStatus;
use intmax2_zkp::{
    circuits::claim::utils::get_mining_deposit_nullifier,
    common::signature_content::key_set::KeySet,
    ethereum_types::{bytes32::Bytes32, u256::U256},
};
use num_bigint::BigUint;

use crate::{
    config::TestConfig,
    deposit::single_deposit,
    utils::{calculate_balance_with_gas_deduction, print_info},
    withdrawal::single_withdrawal,
};

pub async fn mining_loop(
    config: &TestConfig,
    client: &Client,
    eth_private_key: B256,
) -> anyhow::Result<()> {
    print_info(client, eth_private_key).await?;
    // Set deposit amount to 0.1 ETH
    let deposit_amount: U256 = BigUint::from(10u32).pow(17).try_into().unwrap();

    let recipient = convert_address_to_intmax(get_address_from_private_key(eth_private_key));
    let key = generate_intmax_account_from_eth_key(eth_private_key);

    loop {
        let depositor = get_address_from_private_key(eth_private_key);
        let gas_limit = 200000;
        let balance_deducted = calculate_balance_with_gas_deduction(
            &client.liquidity_contract.provider,
            depositor,
            2,
            gas_limit,
        )
        .await?;
        let balance_deducted = convert_u256_to_intmax(balance_deducted);
        if balance_deducted < deposit_amount {
            return Err(anyhow::anyhow!(
                "Insufficient balance for deposit: balance deducted: {}",
                balance_deducted,
            ));
        }
        let deposit_data = single_deposit(config, client, eth_private_key, deposit_amount)
            .await
            .context("Failed to perform deposit")?;
        log::info!("Deposit completed",);

        let mining_info = get_mining_info(client, key, deposit_data.pubkey_salt_hash).await?;
        let maturity = mining_info.maturity.ok_or(anyhow::anyhow!(
            "No maturity found for the corresponding mining info"
        ))?;
        let local_time = Local.timestamp_opt(maturity as i64, 0).single().unwrap();
        log::info!(
            "Maturity time: {}, Current time: {}",
            local_time,
            Local::now()
        );
        let current_timestamp = Utc::now().timestamp() as u64;
        let sleep_time = maturity.saturating_sub(current_timestamp);
        sleep_for(sleep_time).await;

        let mut retry = 0;
        loop {
            if retry >= config.mining_info_check_retries {
                return Err(anyhow::anyhow!(
                    "Failed to check mining info after {} retries",
                    retry
                ));
            }
            let mining_info = get_mining_info(client, key, deposit_data.pubkey_salt_hash).await?;
            if matches!(mining_info.status, MiningStatus::Claimable(_)) {
                break;
            }
            if !matches!(mining_info.status, MiningStatus::Locking) {
                return Err(anyhow::anyhow!("Mining info status is not locking"));
            }
            log::warn!("Mining info status is not claimable yet, retrying...");
            sleep_for(config.mining_info_check_interval).await;
            retry += 1;
        }
        log::info!("Mining is claimable");
        single_withdrawal(config, client, eth_private_key, true, false)
            .await
            .context("Failed to perform withdrawal")?;
        log::info!("Withdrawal completed");

        let fee_info = client.withdrawal_server.get_claim_fee().await?;
        client.sync_claims(key, recipient, &fee_info, 0).await?;
        log::info!(
            "Claim synced, sleep for {} seconds",
            config.claim_check_wait_time
        );
        sleep_for(config.claim_check_wait_time).await;

        let nullifier = get_mining_deposit_nullifier(
            &mining_info.deposit_data.deposit().unwrap(),
            mining_info.deposit_data.deposit_salt,
        );
        let mut retries = 0;
        loop {
            if retries >= config.claim_check_retries {
                return Err(anyhow::anyhow!(
                    "Failed to check claim after {} retries",
                    retries
                ));
            }
            let claim_info = client.get_claim_info(key).await?;
            let corresponding_claim_info = claim_info
                .iter()
                .find(|w| w.claim.nullifier == nullifier)
                .ok_or(anyhow::anyhow!("Claim not found"))?;
            log::info!("Claim status: {}", corresponding_claim_info.status);
            match corresponding_claim_info.status {
                ClaimStatus::Success => {
                    log::info!("Claim is successful");
                    break;
                }
                ClaimStatus::Failed => {
                    return Err(anyhow::anyhow!("Claim failed"));
                }
                _ => {}
            }
            log::warn!("Claim is not successful yet, retrying...");
            sleep_for(config.claim_check_interval).await;
            retries += 1;
        }
    }
}

async fn get_mining_info(
    client: &Client,
    key: KeySet,
    pubkey_salt_hash: Bytes32,
) -> anyhow::Result<Mining> {
    let mining_info = client.get_mining_list(key).await?;
    let corresponding_mining_info = mining_info
        .into_iter()
        .find(|info| info.deposit_data.pubkey_salt_hash == pubkey_salt_hash)
        .ok_or(anyhow::anyhow!(
            "No corresponding mining info found for the deposit data"
        ))?;
    Ok(corresponding_mining_info)
}
