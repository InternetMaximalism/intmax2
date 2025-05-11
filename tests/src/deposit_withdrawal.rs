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
use intmax2_zkp::ethereum_types::address::Address;

pub async fn single_deposit_withdrawal(
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
            deposit_result.deposit_data.pubkey_salt_hash,
            deposit_result.deposit_data.amount,
            &aml_permission,
            &eligibility_permission,
        )
        .await?;

    // Wait for the deposit to be confirmed

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
