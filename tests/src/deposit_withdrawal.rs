use alloy::{primitives::B256, providers::Provider as _};
use anyhow::Context;
use intmax2_cli::cli::deposit::fetch_predicate_permission;
use intmax2_client_sdk::{
    client::{
        client::Client, fee_payment::generate_fee_payment_memo,
        key_from_eth::generate_intmax_account_from_eth_key,
    },
    external_api::{
        contract::{
            convert::{convert_address_to_intmax, convert_u256_to_intmax},
            utils::get_address_from_private_key,
        },
        utils::time::sleep_for,
    },
};
use intmax2_interfaces::{
    api::withdrawal_server::interface::WithdrawalStatus, data::deposit_data::TokenType,
};
use intmax2_zkp::{
    common::{salt::Salt, transfer::Transfer, withdrawal::get_withdrawal_nullifier},
    ethereum_types::{address::Address, u256::U256, u32limb_trait::U32LimbTrait},
};
use std::time::Duration;

use crate::{
    config::TestConfig,
    send::send_transfers,
    utils::{calculate_balance_with_gas_deduction, get_balance_on_intmax, get_block_builder_url},
};

pub async fn single_deposit(
    config: &TestConfig,
    client: &Client,
    eth_private_key: B256,
) -> anyhow::Result<()> {
    let key = generate_intmax_account_from_eth_key(eth_private_key);
    let depositor = get_address_from_private_key(eth_private_key);
    let gas_limit = 200000;
    let deposit_amount = calculate_balance_with_gas_deduction(
        &client.liquidity_contract.provider,
        depositor,
        2,
        gas_limit,
    )
    .await?;

    let depositor = convert_address_to_intmax(depositor);
    let deposit_result = client
        .prepare_deposit(
            depositor,
            key.pubkey,
            convert_u256_to_intmax(deposit_amount),
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
            return Err(anyhow::anyhow!("Deposit is not synced to validity prover"));
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
            return Err(anyhow::anyhow!("Deposit is not synced to validity prover"));
        }
        let deposit_info = client
            .validity_prover
            .get_deposit_info(deposit_data.pubkey_salt_hash)
            .await?
            .ok_or(anyhow::anyhow!(
                "Deposit is disappeared from validity prover"
            ))?;
        if deposit_info.block_number.is_some() {
            break;
        }
        log::warn!("Deposit is not synced to L2, retrying...");
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
    Ok(())
}

pub async fn single_withdrawal(
    config: &TestConfig,
    client: &Client,
    eth_private_key: B256,
    with_claim_fee: bool,
) -> anyhow::Result<()> {
    let key = generate_intmax_account_from_eth_key(eth_private_key);
    let intmax_balance = get_balance_on_intmax(client, key).await?;
    let ethereum_address = get_address_from_private_key(eth_private_key);
    let fee_token_index = 0;

    let block_builder_url = get_block_builder_url(&config.indexer_base_url).await?;

    // fetch transfer fee
    let transfer_fee_quote = client
        .quote_transfer_fee(&block_builder_url, key.pubkey, fee_token_index)
        .await?;
    let transfer_fee = transfer_fee_quote
        .fee
        .clone()
        .map_or(U256::zero(), |f| f.amount);

    // fetch withdraw fee
    let withdraw_fee_quote = client.quote_withdrawal_fee(0, fee_token_index).await?;
    let withdraw_fee = withdraw_fee_quote.fee.map_or(U256::zero(), |f| f.amount);

    // fetch claim fee
    let claim_fee = if with_claim_fee {
        let claim_fee_quote = client.quote_claim_fee(fee_token_index).await?;
        claim_fee_quote.fee.map_or(U256::zero(), |f| f.amount)
    } else {
        U256::zero()
    };
    if intmax_balance < transfer_fee + withdraw_fee + claim_fee {
        return Err(anyhow::anyhow!(
            "Insufficient balance for withdrawal: balance: {}, transfer fee: {}, withdraw fee: {}, claim fee: {}",
            intmax_balance,
            transfer_fee,
            withdraw_fee,
            claim_fee
        ));
    }
    let withdrawal_amount = intmax_balance - transfer_fee - withdraw_fee - claim_fee;
    let withdrawal_transfer = Transfer {
        recipient: convert_address_to_intmax(ethereum_address).into(),
        token_index: 0,
        amount: withdrawal_amount,
        salt: Salt::rand(&mut rand::thread_rng()),
    };
    let withdrawal_transfers = client
        .generate_withdrawal_transfers(&withdrawal_transfer, fee_token_index, with_claim_fee)
        .await?;
    let payment_memos = generate_fee_payment_memo(
        &withdrawal_transfers.transfers,
        withdrawal_transfers.withdrawal_fee_transfer_index,
        withdrawal_transfers.claim_fee_transfer_index,
    )?;

    let mut retries = 0;
    loop {
        if retries >= config.tx_resend_retries {
            return Err(anyhow::anyhow!(
                "Failed to send withdrawal after {} retries",
                retries
            ));
        }
        let result = send_transfers(
            config,
            client,
            key,
            &withdrawal_transfers.transfers,
            payment_memos.clone(),
            fee_token_index,
        )
        .await;
        match result {
            Ok(_) => break,
            Err(e) => {
                log::warn!("Failed to send withdrawal: {}", e);
            }
        }
        log::warn!("Retrying...");
        sleep_for(config.tx_resend_interval).await;
        retries += 1;
    }

    // execute withdrawal
    let withdrawal_fee_info = client.withdrawal_server.get_withdrawal_fee().await?;
    client
        .sync_withdrawals(key, &withdrawal_fee_info, fee_token_index)
        .await
        .context("Failed to sync withdrawals")?;

    let nullifier = get_withdrawal_nullifier(&withdrawal_transfer);

    let mut retries = 0;
    loop {
        if retries >= config.withdrawal_check_retries {
            return Err(anyhow::anyhow!(
                "Failed to check withdrawal after {} retries",
                retries
            ));
        }
        let withdrawal_info = client
            .get_withdrawal_info(key)
            .await
            .context("Failed to get withdrawal info")?;
        let corresponding_withdrawal_info = withdrawal_info
            .iter()
            .find(|w| w.contract_withdrawal.nullifier == nullifier)
            .ok_or(anyhow::anyhow!("Withdrawal not found"))?;

        log::info!(
            "Withdrawal status: {}",
            corresponding_withdrawal_info.status
        );
        match corresponding_withdrawal_info.status {
            WithdrawalStatus::Success => {
                log::info!("Withdrawal is successful");
                break;
            }
            WithdrawalStatus::Failed => {
                return Err(anyhow::anyhow!("Withdrawal failed"));
            }
            _ => {}
        }
        log::warn!("Withdrawal is not successful yet, retrying...");
        tokio::time::sleep(Duration::from_secs(config.withdrawal_check_interval)).await;
        retries += 1;
    }

    // check l1 balance
    let eth_balance = client
        .liquidity_contract
        .provider
        .get_balance(ethereum_address)
        .await?;
    let eth_balance = convert_u256_to_intmax(eth_balance);
    if eth_balance < withdrawal_amount {
        return Err(anyhow::anyhow!(
            "Withdrawal is not reflected in the balance: {}",
            eth_balance
        ));
    }
    Ok(())
}
