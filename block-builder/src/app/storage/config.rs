use intmax2_zkp::ethereum_types::{address::Address, u256::U256};

#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub use_fee: bool,
    pub use_collateral: bool,
    pub block_builder_address: Address,
    pub fee_beneficiary: U256,
    pub tx_timeout: u64,
    pub accepting_tx_interval: u64,
    pub proposing_block_interval: u64,
    pub deposit_check_interval: Option<u64>,
    pub block_builder_id: String,

    // Redis configuration
    pub redis_url: Option<String>,
    pub cluster_id: Option<String>,
}
