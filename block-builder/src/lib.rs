use alloy::primitives::{Address, B256};
use intmax2_zkp::ethereum_types::bytes32::Bytes32;
use serde::Deserialize;

pub mod api;
pub mod app;

#[derive(Deserialize)]
pub struct EnvVar {
    pub port: u16,
    pub block_builder_url: String,
    pub redis_url: Option<String>,
    pub cluster_id: Option<String>,
    pub l2_rpc_url: String,
    pub rollup_contract_address: Address,
    pub block_builder_registry_contract_address: Address,

    pub store_vault_server_base_url: String,
    pub use_s3: Option<bool>,
    pub validity_prover_base_url: String,

    pub block_builder_private_key: B256,
    pub eth_allowance_for_block: String,

    pub tx_timeout: u64,
    pub accepting_tx_interval: u64,
    pub proposing_block_interval: u64,
    pub deposit_check_interval: Option<u64>,
    pub initial_heart_beat_delay: u64,
    pub gas_limit_for_block_post: Option<u64>,
    pub heart_beat_interval: u64,
    pub nonce_waiting_time: Option<u64>,

    pub beneficiary_pubkey: Option<Bytes32>,
    pub registration_fee: Option<String>,
    pub non_registration_fee: Option<String>,
    pub registration_collateral_fee: Option<String>,
    pub non_registration_collateral_fee: Option<String>,
}
