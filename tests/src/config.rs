use serde::Deserialize;

#[derive(Deserialize)]
pub struct TestConfig {
    pub indexer_base_url: String,

    // deposits config
    pub deposit_sync_check_interval: u64,
    pub deposit_sync_check_retries: u64,
    pub deposit_relay_check_interval: u64,
    pub deposit_relay_check_retries: u64,

    // withdrawals config
    pub withdrawal_check_interval: u64,
    pub withdrawal_check_retries: u64,

    // mining config
    pub mining_info_check_interval: u64,
    pub mining_info_check_retries: u64,
    pub claim_check_wait_time: u64,
    pub claim_check_interval: u64,
    pub claim_check_retries: u64,

    // tx send config
    pub block_builder_query_wait_time: u64,
    pub block_sync_margin: u64,
    pub tx_status_check_interval: u64,
    pub tx_resend_interval: u64,
    pub tx_resend_retries: u64,

    pub bridge_loop_eth_wait_time: u64,
    pub bridge_loop_intmax_wait_time: u64,
    pub transfer_loop_wait_time: u64,
    pub mining_loop_eth_wait_time: u64,
}

impl TestConfig {
    pub fn load_from_env() -> anyhow::Result<Self> {
        let config = envy::from_env::<TestConfig>()?;
        Ok(config)
    }
}
