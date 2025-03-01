#[derive(Debug, Clone)]
pub struct StateConfig {
    pub tx_timeout: u64,
    pub accepting_tx_interval: u64,
    pub proposing_block_interval: u64,
}
