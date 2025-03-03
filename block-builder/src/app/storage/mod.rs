use intmax2_client_sdk::external_api::store_vault_server::StoreVaultServerClient;
use intmax2_zkp::common::block_builder::{BlockProposal, UserSignature};

use super::{block_post::BlockPostTask, types::TxRequest};

pub mod config;
use config::StorageConfig;
pub mod error;
pub mod memory_storage;
pub mod redis_storage;

/// Factory trait for creating Storage instances
#[async_trait::async_trait(?Send)]
pub trait StorageFactory {
    /// Create a new storage instance with the given configuration
    async fn new(config: &StorageConfig) -> Self;
}

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
        store_vault_server_client: &StoreVaultServerClient,
    ) -> Result<(), error::StorageError>;

    async fn enqueue_empty_block(&self) -> Result<(), error::StorageError>;
}

/// Create a storage implementation based on the configuration
///
/// Returns RedisStorage if redis_url is set in the config, otherwise returns InMemoryStorage
pub async fn create_storage(config: &StorageConfig) -> Box<dyn Storage> {
    if config.redis_url.is_some() {
        log::info!("use redis storage");
        Box::new(redis_storage::RedisStorage::new(config).await)
    } else {
        log::info!("use in-memory storage");
        Box::new(memory_storage::InMemoryStorage::new(config).await)
    }
}
