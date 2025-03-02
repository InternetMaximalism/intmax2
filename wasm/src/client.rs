use intmax2_client_sdk::{
    client::{client::Client, config::ClientConfig},
    external_api::{
        block_builder::BlockBuilderClient,
        contract::{
            liquidity_contract::LiquidityContract, rollup_contract::RollupContract,
            withdrawal_contract::WithdrawalContract,
        },
        private_zkp_server::PrivateZKPServerClient,
        store_vault_server::StoreVaultServerClient,
        validity_prover::ValidityProverClient,
        withdrawal_server::WithdrawalServerClient,
    },
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct Config {
    /// URL of the store vault server
    pub store_vault_server_url: String,

    /// URL of the balance prover
    pub balance_prover_url: String,

    /// URL of the block validity prover
    pub validity_prover_url: String,

    /// URL of the withdrawal aggregator
    pub withdrawal_server_url: String,

    /// Time to reach the rollup contract after taking a backup of the deposit
    /// If this time is exceeded, the deposit backup will be ignored
    pub deposit_timeout: u64,

    /// Time to reach the rollup contract after sending a tx request
    /// If this time is exceeded, the tx request will be ignored
    pub tx_timeout: u64,

    /// Interval between retries for tx requests
    pub block_builder_request_interval: u64,

    /// Maximum number of retries for tx requests,
    pub block_builder_request_limit: u64,

    /// Initial wait time for tx query
    pub block_builder_query_wait_time: u64,

    /// Interval between retries for tx queries
    pub block_builder_query_interval: u64,

    /// Maximum number of retries for tx queries
    pub block_builder_query_limit: u64,

    /// URL of the Ethereum RPC
    pub l1_rpc_url: String,

    /// Chain ID of the Ethereum network
    pub l1_chain_id: u64,

    /// Address of the liquidity contract
    pub liquidity_contract_address: String,

    /// URL of the Scroll RPC
    pub l2_rpc_url: String,

    /// Chain ID of the Scroll network
    pub l2_chain_id: u64,

    /// Address of the rollup contract
    pub rollup_contract_address: String,

    /// Scroll block number when the rollup contract was deployed
    pub rollup_contract_deployed_block_number: u64,

    /// Address of the withdrawal contract
    pub withdrawal_contract_address: String,
}

#[wasm_bindgen]
impl Config {
    #[allow(clippy::too_many_arguments)]
    #[wasm_bindgen(constructor)]
    pub fn new(
        store_vault_server_url: String,
        balance_prover_url: String,
        validity_prover_url: String,
        withdrawal_server_url: String,
        deposit_timeout: u64,
        tx_timeout: u64,

        block_builder_request_interval: u64,
        block_builder_request_limit: u64,
        block_builder_query_wait_time: u64,
        block_builder_query_interval: u64,
        block_builder_query_limit: u64,

        l1_rpc_url: String,
        l1_chain_id: u64,
        liquidity_contract_address: String,
        l2_rpc_url: String,
        l2_chain_id: u64,
        rollup_contract_address: String,
        rollup_contract_deployed_block_number: u64,
        withdrawal_contract_address: String,
    ) -> Config {
        Config {
            store_vault_server_url,
            balance_prover_url,
            validity_prover_url,
            withdrawal_server_url,
            deposit_timeout,
            tx_timeout,
            block_builder_request_interval,
            block_builder_request_limit,
            block_builder_query_wait_time,
            block_builder_query_interval,
            block_builder_query_limit,
            l1_rpc_url,
            l1_chain_id,
            liquidity_contract_address,
            l2_rpc_url,
            l2_chain_id,
            rollup_contract_address,
            rollup_contract_deployed_block_number,
            withdrawal_contract_address,
        }
    }
}

pub fn get_client(config: &Config) -> Client {
    let block_builder = Box::new(BlockBuilderClient::new());
    let store_vault_server = Box::new(StoreVaultServerClient::new(&config.store_vault_server_url));

    let validity_prover = Box::new(ValidityProverClient::new(&config.validity_prover_url));
    let balance_prover = Box::new(PrivateZKPServerClient::new(&config.balance_prover_url));
    let withdrawal_server = Box::new(WithdrawalServerClient::new(&config.withdrawal_server_url));

    let client_config = ClientConfig {
        deposit_timeout: config.deposit_timeout,
        tx_timeout: config.tx_timeout,
        block_builder_request_interval: config.block_builder_request_interval,
        block_builder_request_limit: config.block_builder_request_limit,
        block_builder_query_wait_time: config.block_builder_query_wait_time,
        block_builder_query_interval: config.block_builder_query_interval,
        block_builder_query_limit: config.block_builder_query_limit,
    };

    let liquidity_contract = LiquidityContract::new(
        &config.l1_rpc_url,
        config.l1_chain_id,
        config.liquidity_contract_address.parse().unwrap(),
    );

    let rollup_contract = RollupContract::new(
        &config.l2_rpc_url,
        config.l2_chain_id,
        config.rollup_contract_address.parse().unwrap(),
        config.rollup_contract_deployed_block_number,
    );
    let withdrawal_contract = WithdrawalContract::new(
        &config.l2_rpc_url,
        config.l2_chain_id,
        config.withdrawal_contract_address.parse().unwrap(),
    );

    Client {
        block_builder,
        store_vault_server,
        validity_prover,
        balance_prover,
        withdrawal_server,
        liquidity_contract,
        rollup_contract,
        withdrawal_contract,
        config: client_config,
    }
}
