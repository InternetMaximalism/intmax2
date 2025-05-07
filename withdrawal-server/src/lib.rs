use alloy::primitives::{Address, B256};
use serde::Deserialize;

pub mod api;
pub mod app;

#[derive(Debug, Deserialize)]
pub struct Env {
    pub port: u16,
    pub database_url: String,
    pub database_max_connections: u32,
    pub database_timeout: u64,

    pub store_vault_server_base_url: String,
    pub use_s3: Option<bool>,
    pub validity_prover_base_url: String,

    pub l2_rpc_url: String,
    pub rollup_contract_address: Address,
    pub withdrawal_contract_address: Address,

    pub is_faster_mining: bool,
    pub withdrawal_beneficiary_private_key: Option<B256>,
    pub claim_beneficiary_private_key: Option<B256>,
    pub direct_withdrawal_fee: Option<String>,
    pub claimable_withdrawal_fee: Option<String>,
    pub claim_fee: Option<String>,
}
