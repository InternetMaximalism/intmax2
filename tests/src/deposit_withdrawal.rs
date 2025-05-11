use std::time::Duration;

use alloy::{
    primitives::{Address as AlloyAddress, B256, U256 as AlloyU256},
    providers::Provider,
};
use intmax2_cli::cli::deposit::fetch_predicate_permission;
use intmax2_client_sdk::{
    client::{client::Client, key_from_eth::generate_intmax_account_from_eth_key},
    external_api::contract::{
        convert::{convert_address_to_intmax, convert_u256_to_intmax},
        utils::{get_address_from_private_key, NormalProvider},
    },
};
use intmax2_interfaces::data::deposit_data::TokenType;
use intmax2_zkp::{
    common::signature_content::key_set::KeySet,
    ethereum_types::{address::Address, u256::U256},
};

use crate::config::TestConfig;

pub async fn single_deposit_withdrawal(
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
        if retries >= config.deposit_check_retries {
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
        tokio::time::sleep(Duration::from_secs(config.deposit_check_interval)).await;
        retries += 1;
    }
    log::info!("Deposit is synced to validity prover");

    // Wait for the deposit to be relayed to the L2
    let mut retries = 0;
    loop {
        if retries >= config.deposit_check_retries {
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
        tokio::time::sleep(Duration::from_secs(config.deposit_check_interval)).await;
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

    // withdraw

    let transfer_fee = client
        .quote_transfer_fee(block_builder_url, pubkey, fee_token_index)
        .await?;

    Ok(())
}

async fn calculate_balance_with_gas_deduction(
    provider: &NormalProvider,
    address: AlloyAddress,
    multiplier: u64,
    gas_limit: u64,
) -> anyhow::Result<AlloyU256> {
    let balance = provider.get_balance(address).await?;
    let gas_estimation = provider.estimate_eip1559_fees().await?;
    let gas_price = gas_estimation.max_fee_per_gas + gas_estimation.max_priority_fee_per_gas;
    let gas_fee = AlloyU256::from(gas_price) * AlloyU256::from(gas_limit);
    if balance < gas_fee * AlloyU256::from(multiplier) {
        return Err(anyhow::anyhow!(
            "Insufficient balance for gas fee: balance: {}",
            balance
        ));
    }
    let new_balance = balance - gas_fee * AlloyU256::from(multiplier);
    Ok(new_balance)
}

async fn get_balance_on_intmax(client: &Client, key: KeySet) -> anyhow::Result<U256> {
    let balance = client.get_user_data(key).await?.balances();
    let eth_balance = balance.0.get(&0).map_or(U256::default(), |b| b.amount);
    Ok(eth_balance)
}
