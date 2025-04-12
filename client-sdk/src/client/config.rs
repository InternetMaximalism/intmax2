use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientConfig {
    pub deposit_timeout: u64,
    pub tx_timeout: u64,
    pub block_builder_request_interval: u64,
    pub block_builder_request_limit: u64,
    pub block_builder_query_wait_time: u64,
    pub block_builder_query_interval: u64,
    pub block_builder_query_limit: u64,
    pub is_faster_mining: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            deposit_timeout: 7200,
            tx_timeout: 60,
            block_builder_request_interval: 5,
            block_builder_request_limit: 12,
            block_builder_query_wait_time: 5,
            block_builder_query_interval: 5,
            block_builder_query_limit: 20,
            is_faster_mining: false,
        }
    }
}
