use ethers::types::{Address, H256};
use serde::Deserialize;

pub mod api;
pub mod app;

#[derive(Deserialize)]
pub struct EnvVar {
    pub port: u16,
    pub block_builder_url: String,
    pub redis_url: Option<String>,
    pub l2_rpc_url: String,
    pub l2_chain_id: u64,
    pub rollup_contract_address: Address,
    pub rollup_contract_deployed_block_number: u64,
    pub block_builder_registry_contract_address: Address,

    pub store_vault_server_base_url: String,
    pub use_s3: Option<bool>,
    pub validity_prover_base_url: String,

    pub block_builder_private_key: H256,
    pub eth_allowance_for_block: String,

    pub tx_timeout: u64,
    pub accepting_tx_interval: u64,
    pub proposing_block_interval: u64,
    pub deposit_check_interval: Option<u64>,
    pub initial_heart_beat_delay: u64,
    pub heart_beat_interval: u64,

    pub beneficiary_pubkey: Option<H256>,
    pub registration_fee: Option<String>,
    pub non_registration_fee: Option<String>,
    pub registration_collateral_fee: Option<String>,
    pub non_registration_collateral_fee: Option<String>,
}
