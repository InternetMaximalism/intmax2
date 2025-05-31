use alloy::primitives::Address;

#[derive(Debug, Clone)]
pub struct NonceManagerConfig {
    pub block_builder_address: Address,

    // Redis configuration
    pub redis_url: Option<String>,
    pub cluster_id: Option<String>,
}
