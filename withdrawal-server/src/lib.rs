use intmax2_zkp::ethereum_types::bytes32::Bytes32;
use serde::Deserialize;

pub mod api;
pub mod app;

#[derive(Debug, Deserialize)]
pub struct Env {
    pub port: u16,
    pub database_url: String,
    pub database_max_connections: u32,
    pub database_timeout: u64,

    pub withdrawal_beneficiary: Option<Bytes32>,
    pub claim_beneficiary: Option<Bytes32>,
    pub direct_withdrawal_fee: Option<String>,
    pub claimable_withdrawal_fee: Option<String>,
    pub claim_fee: Option<String>,
}
