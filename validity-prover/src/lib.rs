use alloy::primitives::Address;
use serde::Deserialize;

pub mod api;
pub mod app;
pub mod trees;

#[derive(Deserialize)]
pub struct EnvVar {
    pub port: u16,

    // sync settings
    pub is_sync_mode: bool,
    pub leader_lock_ttl: u64,
    pub witness_sync_interval: u64,
    pub validity_proof_interval: u64,
    pub add_tasks_interval: u64,
    pub cleanup_inactive_tasks_interval: u64,
    pub validity_prover_restart_interval: u64,

    // observer settings
    pub observer_event_block_interval: u64,
    pub observer_max_query_times: usize,
    pub observer_sync_interval: u64,
    pub observer_restart_interval: u64,

    // onchain settings
    pub l1_rpc_url: String,
    pub l2_rpc_url: String,
    pub rollup_contract_address: Address,
    pub rollup_contract_deployed_block_number: u64,
    pub liquidity_contract_address: Address,
    pub liquidity_contract_deployed_block_number: u64,

    // the graph settings
    pub the_graph_l1_url: Option<String>,
    pub the_graph_l2_url: Option<String>,
    pub the_graph_l1_bearer: Option<String>,
    pub the_graph_l2_bearer: Option<String>,

    // db settings
    pub database_url: String,
    pub database_max_connections: u32,
    pub database_timeout: u64,

    // prover coordinator
    pub redis_url: String,
    pub task_ttl: u64,
    pub heartbeat_interval: u64,

    // cache
    pub dynamic_cache_ttl: u64,
    pub static_cache_ttl: u64,

    // rate manager
    pub observer_error_threshold: u64,
    pub rate_manager_window: u64,
    pub rate_manager_timeout: u64,
}
