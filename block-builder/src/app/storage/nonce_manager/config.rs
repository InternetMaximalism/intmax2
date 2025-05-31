use intmax2_zkp::ethereum_types::{address::Address, u256::U256};

#[derive(Debug, Clone)]
pub struct NonceManagerConfig {
    pub block_builder_address: Address,

    // Redis configuration
    pub redis_url: Option<String>,
    pub cluster_id: Option<String>,
}
