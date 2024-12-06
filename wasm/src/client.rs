use intmax2_client_sdk::{
    client::{client::Client, config::ClientConfig},
    external_api::{
        balance_prover::BalanceProverClient,
        block_builder::BlockBuilderClient,
        contract::{liquidity_contract::LiquidityContract, rollup_contract::RollupContract},
        store_vault_server::StoreVaultServerClient,
        validity_prover::ValidityProverClient,
        withdrawal_server::WithdrawalServerClient,
    },
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::wasm_bindgen;

type BB = BlockBuilderClient;
type S = StoreVaultServerClient;
type V = ValidityProverClient;
type B = BalanceProverClient;
type W = WithdrawalServerClient;

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

    /// Auth token for the validity prover
    pub auth_validity_prover: String,

    /// Auth token for the balance prover
    pub auth_balance_prover: String,

    /// Time to reach the rollup contract after sending a tx request
    /// If this time is exceeded, the tx request will be ignored
    pub tx_timeout: u64,

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
}

#[wasm_bindgen]
impl Config {
    #[wasm_bindgen(constructor)]
    pub fn new(
        store_vault_server_url: String,
        balance_prover_url: String,
        validity_prover_url: String,
        withdrawal_server_url: String,
        deposit_timeout: u64,
        tx_timeout: u64,
        auth_validity_prover: String,
        auth_balance_prover: String,

        l1_rpc_url: String,
        l1_chain_id: u64,
        liquidity_contract_address: String,
        l2_rpc_url: String,
        l2_chain_id: u64,
        rollup_contract_address: String,
        rollup_contract_deployed_block_number: u64,
    ) -> Config {
        Config {
            store_vault_server_url,
            balance_prover_url,
            validity_prover_url,
            withdrawal_server_url,
            deposit_timeout,
            auth_validity_prover,
            auth_balance_prover,
            tx_timeout,
            l1_rpc_url,
            l1_chain_id,
            liquidity_contract_address,
            l2_rpc_url,
            l2_chain_id,
            rollup_contract_address,
            rollup_contract_deployed_block_number,
        }
    }
}

pub fn get_client(config: &Config) -> Client<BB, S, V, B, W> {
    let block_builder = BB::new();
    let store_vault_server = S::new(&config.store_vault_server_url);
    let balance_prover: BalanceProverClient = B::new(
        &config.balance_prover_url,
        Some(config.auth_balance_prover.clone()),
    );
    let validity_prover = V::new(
        &config.validity_prover_url,
        Some(config.auth_validity_prover.clone()),
    );
    let withdrawal_server = W::new(&config.withdrawal_server_url);

    let client_config = ClientConfig {
        deposit_timeout: config.deposit_timeout,
        tx_timeout: config.tx_timeout,
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

    Client {
        block_builder,
        store_vault_server,
        validity_prover,
        balance_prover,
        withdrawal_server,
        liquidity_contract,
        rollup_contract,
        config: client_config,
    }
}
