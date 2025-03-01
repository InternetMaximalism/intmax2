use intmax2_zkp::ethereum_types::u256::U256;

#[derive(Debug, Clone)]
pub struct StateConfig {
    pub use_fee: bool,
    pub use_collateral: bool,
    pub fee_beneficiary: U256,
    pub tx_timeout: u64,
    pub accepting_tx_interval: u64,
    pub proposing_block_interval: u64,
}
