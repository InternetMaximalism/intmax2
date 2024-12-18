use ethers::types::Address;
use serde::Deserialize;

pub mod api;
pub mod utils;

#[derive(Deserialize)]
pub struct Env {
    pub port: u16,
    pub sync_interval: u64,
    pub l2_rpc_url: String,
    pub l2_chain_id: u64,
    pub rollup_contract_address: Address,
    pub rollup_contract_deployed_block_number: u64,
    pub database_url: String,
    pub database_max_connections: u32,
    pub database_timeout: u64,
}
