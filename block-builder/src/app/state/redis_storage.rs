use std::sync::Arc;

use intmax2_client_sdk::external_api::store_vault_server::StoreVaultServerClient;
use intmax2_zkp::{common::block_builder::UserSignature, constants::NUM_SENDERS_IN_BLOCK};
use redis::{aio::ConnectionManager, AsyncCommands, Client, RedisResult};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::app::{
    block_post::BlockPostTask,
    fee::{collect_fee, FeeCollection},
    types::{ProposalMemo, TxRequest},
};

use super::{config::StateConfig, error::StateError, Storage};

// Serializable versions of our data structures for Redis storage
#[derive(Serialize, Deserialize, Clone, Debug)]
struct SerializableTxRequest {
    request: TxRequest,
    timestamp: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SerializableProposalMemo {
    memo: ProposalMemo,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SerializableUserSignature {
    signature: UserSignature,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SerializableFeeCollection {
    fee_collection: FeeCollection,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SerializableBlockPostTask {
    task: BlockPostTask,
}

pub struct RedisStorage {
    pub config: StateConfig,
    conn_manager: Arc<Mutex<ConnectionManager>>,
    // Redis key names - shared across all block builder instances
    registration_tx_requests_key: String,
    registration_tx_last_processed_key: String,
    non_registration_tx_requests_key: String,
    non_registration_tx_last_processed_key: String,
    request_id_to_block_id_key: String,
    memos_key: String,
    signatures_key: String,
    fee_collection_tasks_key: String,
    block_post_tasks_hi_key: String,
    block_post_tasks_lo_key: String,
}

impl RedisStorage {
    // Helper method to get a connection from the pool
    async fn get_conn(&self) -> RedisResult<ConnectionManager> {
        let conn = self.conn_manager.lock().await;
        Ok(conn.clone())
    }
}

#[async_trait::async_trait(?Send)]
impl Storage for RedisStorage {
    fn new(config: &StateConfig) -> Self {
        // Create a common prefix for all block builder instances to share the same state
        let prefix = "block_builder:shared:";
        
        // Create Redis client
        let redis_url = config.redis_url.clone().unwrap_or_else(|| "redis://127.0.0.1:6379".to_string());
        let client = Client::open(redis_url).expect("Failed to create Redis client");
        
        // Create connection manager - this is a blocking operation but it's only done once at startup
        let conn_manager = tokio::runtime::Handle::current().block_on(async {
            ConnectionManager::new(client).await.expect("Failed to create Redis connection manager")
        });
        
        Self {
            config: config.clone(),
            conn_manager: Arc::new(Mutex::new(conn_manager)),
            
            // Define Redis keys with shared prefix
            registration_tx_requests_key: format!("{}registration_tx_requests", prefix),
            registration_tx_last_processed_key: format!("{}registration_tx_last_processed", prefix),
            non_registration_tx_requests_key: format!("{}non_registration_tx_requests", prefix),
            non_registration_tx_last_processed_key: format!("{}non_registration_tx_last_processed", prefix),
            request_id_to_block_id_key: format!("{}request_id_to_block_id", prefix),
            memos_key: format!("{}memos", prefix),
            signatures_key: format!("{}signatures", prefix),
            fee_collection_tasks_key: format!("{}fee_collection_tasks", prefix),
            block_post_tasks_hi_key: format!("{}block_post_tasks_hi", prefix),
            block_post_tasks_lo_key: format!("{}block_post_tasks_lo", prefix),
        }
    }

    async fn add_tx(
        &self,
        is_registration: bool,
        tx_request: TxRequest,
    ) -> Result<(), StateError> {
        let key = if is_registration {
            &self.registration_tx_requests_key
        } else {
            &self.non_registration_tx_requests_key
        };
        
        let serializable_request = SerializableTxRequest {
            request: tx_request,
            timestamp: chrono::Utc::now().timestamp() as u64,
        };
        
        let serialized = serde_json::to_string(&serializable_request)?;
        
        let mut conn = self.get_conn().await?;
        
        // Push to the list
        let _: () = conn.rpush(key, serialized).await?;
        
        Ok(())
    }

    async fn process_requests(&self, is_registration: bool) -> Result<(), StateError> {
        let requests_key = if is_registration {
            &self.registration_tx_requests_key
        } else {
            &self.non_registration_tx_requests_key
        };
        
        let last_processed_key = if is_registration {
            &self.registration_tx_last_processed_key
        } else {
            &self.non_registration_tx_last_processed_key
        };
        
        let mut conn = self.get_conn().await?;
        
        // Get the last processed timestamp
        let last_processed: Option<String> = conn.get(last_processed_key).await?;
        
        let last_processed = last_processed
            .map(|s| s.parse::<u64>().unwrap_or(0))
            .unwrap_or(0);
        
        // Get the length of the queue
        let queue_len: usize = conn.llen(requests_key).await?;
        
        // Check if we should process requests
        let current_time = chrono::Utc::now().timestamp() as u64;
        if (queue_len < NUM_SENDERS_IN_BLOCK && 
            current_time < last_processed + self.config.accepting_tx_interval) || 
            queue_len == 0 {
            return Ok(());
        }
        
        // Get up to NUM_SENDERS_IN_BLOCK requests
        let num_to_process = std::cmp::min(queue_len, NUM_SENDERS_IN_BLOCK);
        let serialized_requests: Vec<String> = conn.lrange(requests_key, 0, num_to_process as isize - 1).await?;
        
        // Deserialize requests
        let mut tx_requests = Vec::with_capacity(num_to_process);
        for serialized in &serialized_requests {
            let serializable_request: SerializableTxRequest = serde_json::from_str(serialized)?;
            tx_requests.push(serializable_request.request);
        }
        
        // Create memo
        let memo = ProposalMemo::from_tx_requests(is_registration, &tx_requests, self.config.tx_timeout);
        
        // Store memo
        let serialized_memo = serde_json::to_string(&SerializableProposalMemo { memo: memo.clone() })?;
        
        let _: () = conn.hset(&self.memos_key, &memo.block_id, &serialized_memo).await?;
        
        // Update request_id -> block_id mapping
        for tx_request in &tx_requests {
            let _: () = conn.hset(&self.request_id_to_block_id_key, &tx_request.request_id, &memo.block_id).await?;
        }
        
        // Remove processed requests from the queue
        let _: () = conn.ltrim(requests_key, num_to_process as isize, -1).await?;
        
        // Update last processed timestamp
        let _: () = conn.set(last_processed_key, current_time.to_string()).await?;
        
        Ok(())
    }

    async fn add_signature(
        &self,
        request_id: &str,
        signature: UserSignature,
    ) -> Result<(), StateError> {
        let mut conn = self.get_conn().await?;
        
        // Get block_id for request_id
        let block_id: Option<String> = conn.hget(&self.request_id_to_block_id_key, request_id).await?;
        
        let block_id = block_id.ok_or_else(|| {
            StateError::AddSignatureError(format!("block_id not found for request_id: {}", request_id))
        })?;
        
        // Get memo for block_id
        let serialized_memo: Option<String> = conn.hget(&self.memos_key, &block_id).await?;
        
        let serialized_memo = serialized_memo.ok_or_else(|| {
            StateError::AddSignatureError(format!("memo not found for block_id: {}", block_id))
        })?;
        
        let memo = serde_json::from_str::<SerializableProposalMemo>(&serialized_memo)?
            .memo;
        
        // Verify signature
        signature
            .verify(memo.tx_tree_root, memo.expiry, memo.pubkey_hash)
            .map_err(|e| {
                StateError::AddSignatureError(format!("signature verification failed: {}", e))
            })?;
        
        // Serialize signature
        let serialized_signature = serde_json::to_string(&SerializableUserSignature { signature })?;
        
        // Add signature to the list for this block_id
        let signatures_key = format!("{}:{}", self.signatures_key, block_id);
        let _: () = conn.rpush(&signatures_key, serialized_signature).await?;
        
        Ok(())
    }

    async fn process_signatures(&self) {
        let mut conn = match self.get_conn().await {
            Ok(conn) => conn,
            Err(e) => {
                log::error!("Failed to get Redis connection: {}", e);
                return;
            }
        };
        
        // Get all memo keys
        let memo_keys: Vec<String> = match conn.hkeys(&self.memos_key).await {
            Ok(keys) => keys,
            Err(e) => {
                log::error!("Failed to get memo keys: {}", e);
                return;
            }
        };
        
        let current_time = chrono::Utc::now().timestamp() as u64;
        
        for block_id in memo_keys {
            // Get memo
            let serialized_memo: Option<String> = match conn.hget(&self.memos_key, &block_id).await {
                Ok(memo) => memo,
                Err(e) => {
                    log::error!("Failed to get memo for block_id {}: {}", block_id, e);
                    continue;
                }
            };
            
            let memo = match serialized_memo {
                Some(serialized) => match serde_json::from_str::<SerializableProposalMemo>(&serialized) {
                    Ok(memo) => memo.memo,
                    Err(e) => {
                        log::error!("Failed to deserialize memo for block_id {}: {}", block_id, e);
                        continue;
                    }
                },
                None => continue,
            };
            
            // Check if it's time to process this memo
            if current_time <= memo.created_at + self.config.proposing_block_interval {
                continue;
            }
            
            // Get signatures for this block
            let signatures_key = format!("{}:{}", self.signatures_key, block_id);
            let serialized_signatures: Vec<String> = match conn.lrange(&signatures_key, 0, -1).await {
                Ok(sigs) => sigs,
                Err(e) => {
                    log::error!("Failed to get signatures for block_id {}: {}", block_id, e);
                    continue;
                }
            };
            
            // Skip if no signatures
            if serialized_signatures.is_empty() {
                continue;
            }
            
            // Deserialize signatures
            let mut signatures = Vec::with_capacity(serialized_signatures.len());
            for serialized in serialized_signatures {
                match serde_json::from_str::<SerializableUserSignature>(&serialized) {
                    Ok(sig) => signatures.push(sig.signature),
                    Err(e) => {
                        log::error!("Failed to deserialize signature: {}", e);
                        continue;
                    }
                }
            }
            
            // Create block post task
            let block_post_task = BlockPostTask::from_memo(&memo, &signatures);
            let serialized_task = match serde_json::to_string(&SerializableBlockPostTask { task: block_post_task }) {
                Ok(task) => task,
                Err(e) => {
                    log::error!("Failed to serialize block post task: {}", e);
                    continue;
                }
            };
            
            // Add to high priority queue
            if let Err(e) = conn.rpush::<_, _, ()>(&self.block_post_tasks_hi_key, &serialized_task).await {
                log::error!("Failed to add block post task to high priority queue: {}", e);
                continue;
            }
            
            // Add fee collection task if needed
            if self.config.use_fee {
                let fee_collection = FeeCollection {
                    use_collateral: self.config.use_collateral,
                    memo: memo.clone(),
                    signatures: signatures.clone(),
                };
                
                let serialized_fee_collection = match serde_json::to_string(&SerializableFeeCollection { fee_collection }) {
                    Ok(collection) => collection,
                    Err(e) => {
                        log::error!("Failed to serialize fee collection: {}", e);
                        continue;
                    }
                };
                
                if let Err(e) = conn.rpush::<_, _, ()>(&self.fee_collection_tasks_key, &serialized_fee_collection).await {
                    log::error!("Failed to add fee collection task: {}", e);
                    continue;
                }
            }
            
            // Remove memo and signatures
            if let Err(e) = conn.hdel::<_, _, i32>(&self.memos_key, &block_id).await {
                log::error!("Failed to delete memo for block_id {}: {}", block_id, e);
            }
            
            if let Err(e) = conn.del::<_, i32>(&signatures_key).await {
                log::error!("Failed to delete signatures for block_id {}: {}", block_id, e);
            }
        }
    }

    async fn process_fee_collection(
        &self,
        store_vault_server_client: &StoreVaultServerClient,
    ) -> Result<(), StateError> {
        let mut conn = self.get_conn().await?;
        
        // Use BLPOP with a short timeout to avoid race conditions between multiple instances
        let serialized_fee_collection: Option<(String, String)> = conn.blpop(&self.fee_collection_tasks_key, 1).await?;
        
        // Return if there's no task
        let serialized_fee_collection = match serialized_fee_collection {
            Some((_, value)) => value,
            None => return Ok(()),
        };
        
        // Deserialize the fee collection task
        let fee_collection = serde_json::from_str::<SerializableFeeCollection>(&serialized_fee_collection)?
            .fee_collection;
        
        // Process the fee collection
        let block_post_tasks = collect_fee(
            store_vault_server_client,
            self.config.fee_beneficiary,
            &fee_collection,
        ).await?;
        
        // Add resulting block post tasks to low priority queue
        for task in block_post_tasks {
            let serialized_task = serde_json::to_string(&SerializableBlockPostTask { task })?;
            
            let _: () = conn.rpush(&self.block_post_tasks_lo_key, &serialized_task).await?;
        }
        
        Ok(())
    }

    async fn dequeue_block_post_task(&self) -> Option<BlockPostTask> {
        let mut conn = match self.get_conn().await {
            Ok(conn) => conn,
            Err(e) => {
                log::error!("Failed to get Redis connection: {}", e);
                return None;
            }
        };
        
        // Try to get a task from high priority queue first using BLPOP with a short timeout
        let serialized_task: Option<(String, String)> = match conn.blpop(&self.block_post_tasks_hi_key, 1).await {
            Ok(result) => result,
            Err(e) => {
                log::error!("Failed to pop from high priority queue: {}", e);
                return None;
            }
        };
        
        // If no high priority task, try low priority queue
        let serialized_task = match serialized_task {
            Some((_, value)) => value,
            None => {
                // Try low priority queue
                let serialized_task: Option<(String, String)> = match conn.blpop(&self.block_post_tasks_lo_key, 1).await {
                    Ok(result) => result,
                    Err(e) => {
                        log::error!("Failed to pop from low priority queue: {}", e);
                        return None;
                    }
                };
                
                match serialized_task {
                    Some((_, value)) => value,
                    None => return None,
                }
            }
        };
        
        // Deserialize the task
        match serde_json::from_str::<SerializableBlockPostTask>(&serialized_task) {
            Ok(task) => Some(task.task),
            Err(e) => {
                log::error!("Failed to deserialize block post task: {}", e);
                None
            }
        }
    }
}
