use alloy::{
    primitives::{Address as AlloyAddress, B256, U256 as AlloyU256},
    providers::Provider,
};
use intmax2_client_sdk::{
    client::{client::Client, key_from_eth::generate_intmax_account_from_eth_key},
    external_api::{
        contract::utils::{get_address_from_private_key, NormalProvider},
        indexer::IndexerClient,
    },
};
use intmax2_interfaces::api::indexer::interface::IndexerClientInterface;
use intmax2_zkp::{
    common::signature_content::key_set::KeySet,
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
};
use reqwest::Url;

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

pub async fn get_balance_on_intmax(client: &Client, key: KeySet) -> anyhow::Result<U256> {
    let balance = client.get_user_data(key).await?.balances();
    let eth_balance = balance.0.get(&0).map_or(U256::default(), |b| b.amount);
    Ok(eth_balance)
}

pub async fn get_block_builder_url(indexer_url: &str) -> anyhow::Result<String> {
    let indexer = IndexerClient::new(indexer_url);
    let block_builder_info = indexer.get_block_builder_info().await?;
    if block_builder_info.is_empty() {
        return Err(anyhow::anyhow!("Block builder info is empty"));
    }
    let url = block_builder_info.first().unwrap().url.clone();
    let _ =
        Url::parse(&url).map_err(|_| anyhow::anyhow!("Malformed block builder URL, {}", url))?;
    Ok(block_builder_info.first().unwrap().url.clone())
}

pub async fn print_info(client: &Client, eth_private_key: B256) -> anyhow::Result<()> {
    let key = generate_intmax_account_from_eth_key(eth_private_key);
    client.sync(key).await?;

    let eth_address = get_address_from_private_key(eth_private_key);
    let eth_balance = client
        .liquidity_contract
        .provider
        .get_balance(eth_address)
        .await?;
    println!("ETH Address: {eth_address}");
    println!("ETH Balance: {eth_balance}");
    let balance = get_balance_on_intmax(client, key).await?;
    println!("Intmax Address: {}", key.pubkey.to_hex());
    println!("Intmax Balance: {balance}");
    Ok(())
}
