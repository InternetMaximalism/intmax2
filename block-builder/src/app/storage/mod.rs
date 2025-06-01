use intmax2_client_sdk::external_api::contract::{
    convert::convert_address_to_alloy, rollup_contract::RollupContract,
};
use intmax2_interfaces::api::store_vault_server::interface::StoreVaultClientInterface;
use intmax2_zkp::common::block_builder::{BlockProposal, UserSignature};
use nonce_manager::{
    config::NonceManagerConfig, memory_nonce_manager::InMemoryNonceManager,
    redis_nonce_manager::RedisNonceManager,
};

use super::{block_post::BlockPostTask, types::TxRequest};

pub mod config;
use config::StorageConfig;
pub mod error;
pub mod memory_storage;
pub mod nonce_manager;
pub mod redis_storage;

#[async_trait::async_trait(?Send)]
pub trait Storage: Sync + Send {
    /// Add a transaction request to the queue
    async fn add_tx(
        &self,
        is_registration: bool,
        tx_request: TxRequest,
    ) -> Result<(), error::StorageError>;

    async fn query_proposal(
        &self,
        request_id: &str,
    ) -> Result<Option<BlockProposal>, error::StorageError>;

    /// Add a signature for a transaction request
    async fn add_signature(
        &self,
        request_id: &str,
        signature: UserSignature,
    ) -> Result<(), error::StorageError>;

    /// Dequeue a block post task
    async fn dequeue_block_post_task(&self) -> Result<Option<BlockPostTask>, error::StorageError>;

    /// Process transaction requests in the queue
    async fn process_requests(&self, is_registration: bool) -> Result<(), error::StorageError>;

    /// Process signatures and create block post tasks
    async fn process_signatures(&self) -> Result<(), error::StorageError>;

    /// Process fee collection tasks
    async fn process_fee_collection(
        &self,
        store_vault_server_client: &dyn StoreVaultClientInterface,
    ) -> Result<(), error::StorageError>;

    async fn enqueue_empty_block(&self) -> Result<(), error::StorageError>;
}

/// Create a storage implementation based on the configuration
///
/// Returns RedisStorage if redis_url is set in the config, otherwise returns InMemoryStorage
pub async fn create_storage(config: &StorageConfig, rollup: RollupContract) -> Box<dyn Storage> {
    if config.redis_url.is_some() {
        log::info!("use redis storage");
        let nonce_config = NonceManagerConfig {
            block_builder_address: convert_address_to_alloy(config.block_builder_address),
            redis_url: config.redis_url.clone(),
            cluster_id: config.cluster_id.clone(),
        };
        let nonce_manager = RedisNonceManager::new(nonce_config, rollup).await;
        Box::new(redis_storage::RedisStorage::new(config, nonce_manager).await)
    } else {
        log::info!("use in-memory storage");
        let nonce_config = NonceManagerConfig {
            block_builder_address: convert_address_to_alloy(config.block_builder_address),
            redis_url: None,
            cluster_id: None,
        };
        let nonce_manager = InMemoryNonceManager::new(nonce_config, rollup);
        Box::new(memory_storage::InMemoryStorage::new(config, nonce_manager))
    }
}
