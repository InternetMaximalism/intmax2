use crate::{config::TestConfig, utils::get_balance_on_intmax};
use alloy::primitives::B256;
use intmax2_cli::cli::deposit::fetch_predicate_permission;
use intmax2_client_sdk::{
    client::{client::Client, key_from_eth::generate_intmax_account_from_eth_key},
    external_api::contract::{
        convert::convert_address_to_intmax, utils::get_address_from_private_key,
    },
};
use intmax2_interfaces::data::deposit_data::{DepositData, TokenType};
use intmax2_zkp::ethereum_types::{address::Address, u256::U256};
use std::time::Duration;

pub async fn single_deposit(
    config: &TestConfig,
    client: &Client,
    eth_private_key: B256,
    amount: U256,
) -> anyhow::Result<DepositData> {
    let key = generate_intmax_account_from_eth_key(eth_private_key);
    let depositor = get_address_from_private_key(eth_private_key);
    let depositor = convert_address_to_intmax(depositor);
    let deposit_result = client
        .prepare_deposit(
            depositor,
            key.pubkey,
            amount,
            TokenType::NATIVE,
            Address::default(),
            0.into(),
            false,
        )
        .await?;

    let deposit_data = deposit_result.deposit_data.clone();
    let aml_permission = fetch_predicate_permission(
        client,
        depositor,
        deposit_data.pubkey_salt_hash,
        deposit_data.token_type,
        deposit_data.amount,
        deposit_data.token_address,
        deposit_data.token_id,
    )
    .await?;
    let eligibility_permission = vec![];

    client
        .liquidity_contract
        .deposit_native(
            eth_private_key,
            None,
            deposit_data.pubkey_salt_hash,
            deposit_data.amount,
            &aml_permission,
            &eligibility_permission,
        )
        .await?;

    // Wait for the deposit to be synced to the validity prover
    let mut retries = 0;
    loop {
        if retries >= config.deposit_sync_check_retries {
            return Err(anyhow::anyhow!(
                "Deposit is not synced to validity prover after retries"
            ));
        }
        let deposit_info = client
            .validity_prover
            .get_deposit_info(deposit_data.pubkey_salt_hash)
            .await?;
        if deposit_info.is_some() {
            break;
        }
        log::warn!("Deposit is not synced to validity prover, retrying...");
        tokio::time::sleep(Duration::from_secs(config.deposit_sync_check_interval)).await;
        retries += 1;
    }
    log::info!("Deposit is synced to validity prover");

    // Wait for the deposit to be relayed to the L2
    let mut retries = 0;
    loop {
        if retries >= config.deposit_relay_check_retries {
            return Err(anyhow::anyhow!(
                "Deposit is not relayed to L2 after retries"
            ));
        }
        let deposit_info = client
            .validity_prover
            .get_deposit_info(deposit_data.pubkey_salt_hash)
            .await?;
        if deposit_info.is_none() {
            // This should not happen, but if it does, we ignore it and continue
            log::error!(
                "Deposit info disappeared after sync: pubkey_salt_hash {}",
                deposit_data.pubkey_salt_hash
            );
            continue;
        }
        let deposit_info = deposit_info.unwrap();
        if deposit_info.block_number.is_some() {
            break;
        }
        log::warn!("Deposit is not relayed to L2, retrying...");
        tokio::time::sleep(Duration::from_secs(config.deposit_relay_check_interval)).await;
        retries += 1;
    }
    log::info!("Deposit is relayed to L2");

    // sync balance
    client.sync(key).await?;
    log::info!("Synced balance");

    let intmax_balance = get_balance_on_intmax(client, key).await?;
    if intmax_balance < deposit_data.amount {
        return Err(anyhow::anyhow!(
            "Deposit is not reflected in the balance: {}",
            intmax_balance
        ));
    }
    Ok(deposit_data)
}
