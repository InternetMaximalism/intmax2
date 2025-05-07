use intmax2_interfaces::api::store_vault_server::types::StoreVaultType;
use intmax2_zkp::ethereum_types::address::Address;
use serde::Deserialize;

use common::env::EnvType;

#[derive(Deserialize)]
pub struct EnvVar {
    pub env: EnvType,

    // client settings
    pub indexer_base_url: String,
    pub store_vault_type: StoreVaultType,
    pub local_backup_path: Option<String>,
    pub store_vault_server_base_url: Option<String>,
    pub validity_prover_base_url: String,
    pub balance_prover_base_url: String,
    pub use_private_zkp_server: Option<bool>,
    pub withdrawal_server_base_url: String,
    pub predicate_base_url: Option<String>,
    pub deposit_timeout: u64,
    pub tx_timeout: u64,

    // block builder settings
    pub block_builder_request_interval: u64,
    pub block_builder_request_limit: u64,
    pub block_builder_query_wait_time: u64,
    pub block_builder_query_interval: u64,
    pub block_builder_query_limit: u64,

    // blockchain settings
    pub l1_rpc_url: String,
    pub liquidity_contract_address: Address,
    pub l2_rpc_url: String,
    pub rollup_contract_address: Address,
    pub withdrawal_contract_address: Address,

    // mining settings
    pub is_faster_mining: bool,

    // optional block builder base url
    pub block_builder_base_url: Option<String>,

    // optional block builder reward contract address
    pub reward_contract_address: Option<Address>,

    // optional private zkp server settings
    pub private_zkp_server_max_retires: Option<usize>,
    pub private_zkp_server_retry_interval: Option<u64>,
}
