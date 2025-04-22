use ethers::types::Address;
use serde::Deserialize;

pub mod api;
pub mod app;
pub mod trees;

#[derive(Deserialize)]
pub struct Env {
    pub port: u16,

    // sync settings
    pub sync_interval: Option<u64>,

    // onchain settings
    pub l1_rpc_url: String,
    pub l1_chain_id: u64,
    pub l2_rpc_url: String,
    pub l2_chain_id: u64,
    pub rollup_contract_address: Address,
    pub rollup_contract_deployed_block_number: u64,
    pub liquidity_contract_address: Address,
    pub liquidity_contract_deployed_block_number: u64,

    // db settings
    pub database_url: String,
    pub database_max_connections: u32,
    pub database_timeout: u64,

    // Prover coordinator
    pub redis_url: String,
    pub task_ttl: u64,
    pub heartbeat_interval: u64,

    // Cache
    pub dynamic_cache_ttl: u64,
    pub static_cache_ttl: u64,
}
