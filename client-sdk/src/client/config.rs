use common::env::{get_env_type, EnvType};
use intmax2_interfaces::utils::network::Network;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientConfig {
    pub network: Network,
    pub deposit_timeout: u64,
    pub tx_timeout: u64,
    pub block_builder_query_wait_time: u64,
    pub block_builder_query_interval: u64,
    pub block_builder_query_limit: u64,
    pub is_faster_mining: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            network: network_from_env(),
            deposit_timeout: 7200,
            tx_timeout: 60,
            block_builder_query_wait_time: 5,
            block_builder_query_interval: 5,
            block_builder_query_limit: 20,
            is_faster_mining: false,
        }
    }
}

pub fn env_type_to_network(env_type: EnvType) -> Network {
    match env_type {
        EnvType::Local => Network::Devnet,
        EnvType::Dev => Network::Devnet,
        EnvType::Staging => Network::Testnet,
        EnvType::Prod => Network::Mainnet,
    }
}

pub fn network_from_env() -> Network {
    env_type_to_network(get_env_type())
}
