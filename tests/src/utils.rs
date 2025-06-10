use alloy::{
    primitives::{Address as AlloyAddress, B256, U256 as AlloyU256},
    providers::Provider,
};
use intmax2_client_sdk::{
    client::client::Client,
    external_api::{
        contract::{
            convert::convert_b256_to_bytes32,
            utils::{get_address_from_private_key, NormalProvider},
        },
        indexer::IndexerClient,
    },
};
use intmax2_interfaces::{
    api::indexer::interface::IndexerClientInterface,
    utils::{
        address::IntmaxAddress,
        key::{KeyPair, ViewPair},
        key_derivation::{derive_keypair_from_spend_key, derive_spend_key_from_bytes32},
    },
};
use intmax2_zkp::ethereum_types::u256::U256;

pub fn get_keypair_from_eth_key(eth_private_key: B256) -> KeyPair {
    let spend_key = derive_spend_key_from_bytes32(convert_b256_to_bytes32(eth_private_key));
    derive_keypair_from_spend_key(spend_key, false)
}

pub async fn calculate_balance_with_gas_deduction(
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

pub async fn get_balance_on_intmax(client: &Client, view_pair: ViewPair) -> anyhow::Result<U256> {
    let balance = client.get_user_data(view_pair).await?.balances();
    let eth_balance = balance.0.get(&0).map_or(U256::default(), |b| b.amount);
    Ok(eth_balance)
}

pub async fn get_block_builder_url(indexer_url: &str) -> anyhow::Result<String> {
    let indexer = IndexerClient::new(indexer_url);
    let block_builder_info = indexer.get_block_builder_info().await?;
    Ok(block_builder_info.url.clone())
}

pub async fn print_info(client: &Client, eth_private_key: B256) -> anyhow::Result<()> {
    let key_pair = get_keypair_from_eth_key(eth_private_key);
    client.sync(key_pair.into()).await?;

    let eth_address = get_address_from_private_key(eth_private_key);
    let eth_balance = client
        .liquidity_contract
        .provider
        .get_balance(eth_address)
        .await?;
    println!("ETH Address: {eth_address}");
    println!("ETH Balance: {eth_balance}");
    let balance = get_balance_on_intmax(client, key_pair.into()).await?;
    let intmax_address = IntmaxAddress::from_keypair(client.config.network, &key_pair);
    println!("Intmax Address: {intmax_address}");
    println!("Intmax Balance: {balance}");
    Ok(())
}
