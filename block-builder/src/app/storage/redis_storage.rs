//! Redis-based implementation of the Storage trait for block builder state.
//!
//! This module provides a distributed storage solution that allows multiple block builder
//! instances to safely share state using Redis. It implements distributed locking,
//! atomic operations, and retry mechanisms to ensure data consistency in a scaled-out environment.

use std::{sync::Arc, time::Duration};

use intmax2_client_sdk::external_api::store_vault_server::StoreVaultServerClient;
use intmax2_zkp::{
    common::block_builder::{BlockProposal, UserSignature},
    constants::NUM_SENDERS_IN_BLOCK,
};
use redis::{aio::ConnectionManager, AsyncCommands, Client, RedisResult, Script};
use serde::{Deserialize, Serialize};
use tokio::{sync::Mutex, time::sleep};

use crate::app::{
    block_post::BlockPostTask,
    fee::{collect_fee, FeeCollection},
    types::{ProposalMemo, TxRequest},
};

use super::{config::StorageConfig, error::StorageError, Storage};

//-----------------------------------------------------------------------------
// Constants
//-----------------------------------------------------------------------------

/// Maximum number of retry attempts for Redis operations
const MAX_RETRIES: usize = 3;

/// Delay between retry attempts in milliseconds (increases with each retry)
const RETRY_DELAY_MS: u64 = 100;

/// Timeout for distributed locks in seconds
const LOCK_TIMEOUT_SECONDS: usize = 10;

//-----------------------------------------------------------------------------
// Helper Types
//-----------------------------------------------------------------------------

/// Wrapper for transaction requests with timestamp information
#[derive(Serialize, Deserialize, Clone, Debug)]
struct TxRequestWithTimestamp {
    /// The original transaction request
    request: TxRequest,

    /// Timestamp when the request was received (Unix timestamp)
    timestamp: u64,
}

//-----------------------------------------------------------------------------
// RedisStorage Implementation
//-----------------------------------------------------------------------------

/// Redis-based implementation of the Storage trait
///
/// This implementation allows multiple block builder instances to share state
/// using Redis as a distributed storage and coordination mechanism.
pub struct RedisStorage {
    /// Configuration for the storage system
    pub config: StorageConfig,

    /// Connection manager for Redis (thread-safe)
    conn_manager: Arc<Mutex<ConnectionManager>>,

    //-------------------------------------------------------------------------
    // Redis key names - shared across all block builder instances
    //-------------------------------------------------------------------------
    /// Common prefix for all Redis keys
    prefix: String,

    /// Queue for registration transaction requests
    registration_tx_requests_key: String,

    /// Timestamp of last processed registration batch
    registration_tx_last_processed_key: String,

    /// Queue for non-registration transaction requests
    non_registration_tx_requests_key: String,

    /// Timestamp of last processed non-registration batch
    non_registration_tx_last_processed_key: String,

    /// Mapping from request ID to block ID
    request_id_to_block_id_key: String,

    /// Storage for proposal memos
    memos_key: String,

    /// Storage for user signatures
    signatures_key: String,

    /// Queue for fee collection tasks
    fee_collection_tasks_key: String,

    /// High priority queue for block posting tasks
    block_post_tasks_hi_key: String,

    /// Low priority queue for block posting tasks
    block_post_tasks_lo_key: String,
}

impl RedisStorage {
    //-------------------------------------------------------------------------
    // Helper Methods
    //-------------------------------------------------------------------------

    /// Get a connection from the connection pool
    ///
    /// This method acquires a lock on the connection manager and returns a clone
    /// of the connection, which can be used for Redis operations.
    async fn get_conn(&self) -> RedisResult<ConnectionManager> {
        let conn = self.conn_manager.lock().await;
        Ok(conn.clone())
    }

    /// Acquire a distributed lock
    ///
    /// This method implements a distributed lock using Redis's SET NX command.
    /// It sets a key with the instance ID as the value, which ensures that only
    /// one instance can hold the lock at a time.
    ///
    /// # Arguments
    /// * `lock_name` - Name of the lock to acquire
    ///
    /// # Returns
    /// * `Ok(true)` if the lock was acquired
    /// * `Ok(false)` if the lock is already held by another instance
    /// * `Err` if there was an error communicating with Redis
    async fn acquire_lock(&self, lock_name: &str) -> Result<bool, StorageError> {
        let mut conn = self.get_conn().await?;
        let lock_key = format!("{}:lock:{}", self.prefix, lock_name);
        let instance_id = &self.config.block_builder_id;

        // Use SET NX with expiration to implement a distributed lock
        let result: bool = conn.set_nx(&lock_key, instance_id).await?;
        if result {
            // Set expiration separately if we got the lock
            // This prevents lock leakage if the instance crashes
            let _: () = conn.expire(&lock_key, LOCK_TIMEOUT_SECONDS).await?;
        }

        Ok(result)
    }

    /// Release a distributed lock
    ///
    /// This method releases a previously acquired lock, but only if it's owned
    /// by this instance. It uses a Lua script to ensure atomicity.
    ///
    /// # Arguments
    /// * `lock_name` - Name of the lock to release
    async fn release_lock(&self, lock_name: &str) -> Result<(), StorageError> {
        let mut conn = self.get_conn().await?;
        let lock_key = format!("{}:lock:{}", self.prefix, lock_name);
        let instance_id = &self.config.block_builder_id;

        // Use a Lua script to ensure we only delete the lock if we own it
        let script = Script::new(
            r"
            if redis.call('get', KEYS[1]) == ARGV[1] then
                return redis.call('del', KEYS[1])
            else
                return 0
            end
        ",
        );

        let _: () = script
            .key(lock_key)
            .arg(instance_id)
            .invoke_async(&mut conn)
            .await?;
        Ok(())
    }

    /// Execute an operation with automatic retries
    ///
    /// This method wraps an async operation and automatically retries it if it fails,
    /// with exponential backoff. It's used to make Redis operations more resilient.
    ///
    /// # Type Parameters
    /// * `F` - Type of the operation function
    /// * `T` - Return type of the operation
    /// * `Fut` - Future type returned by the operation
    ///
    /// # Arguments
    /// * `operation` - The operation to execute with retries
    async fn with_retry<F, T, Fut>(&self, mut operation: F) -> Result<T, StorageError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, StorageError>>,
    {
        let mut retries = 0;
        loop {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    retries += 1;
                    if retries >= MAX_RETRIES {
                        return Err(e);
                    }

                    // Log the error and retry with exponential backoff
                    log::warn!(
                        "Redis operation failed (retry {}/{}): {}",
                        retries,
                        MAX_RETRIES,
                        e
                    );
                    sleep(Duration::from_millis(RETRY_DELAY_MS * retries as u64)).await;
                }
            }
        }
    }

    /// Add a transaction to the appropriate queue
    ///
    /// This is an internal method used by `add_tx` to add a transaction to either
    /// the registration or non-registration queue.
    ///
    /// # Arguments
    /// * `is_registration` - Whether this is a registration transaction
    /// * `tx_request` - The transaction request to add
    async fn add_tx_inner(
        &self,
        is_registration: bool,
        tx_request: TxRequest,
    ) -> Result<(), StorageError> {
        // Select the appropriate queue based on transaction type
        let key = if is_registration {
            &self.registration_tx_requests_key
        } else {
            &self.non_registration_tx_requests_key
        };

        // Add timestamp information
        let request_with_timestamp = TxRequestWithTimestamp {
            request: tx_request,
            timestamp: chrono::Utc::now().timestamp() as u64,
        };

        // Serialize the request
        let serialized = serde_json::to_string(&request_with_timestamp)?;

        // Get a Redis connection
        let mut conn = self.get_conn().await?;

        // Push to the list (queue)
        let _: () = conn.rpush(key, serialized).await?;

        Ok(())
    }

    /// Create a new RedisStorage instance
    ///
    /// This initializes the Redis connection and sets up all the keys used for
    /// shared state across block builder instances.
    pub async fn new(config: &StorageConfig) -> Self {
        // Create a common prefix for all block builder instances to share the same state
        let prefix = "block_builder:shared";

        // Create Redis client with fallback to localhost if URL not provided
        let redis_url = config.redis_url.clone().expect("redis_url not found");
        let client = Client::open(redis_url).expect("Failed to create Redis client");

        // Create connection manager asynchronously
        let conn_manager = ConnectionManager::new(client)
            .await
            .expect("Failed to create Redis connection manager");

        Self {
            config: config.clone(),
            conn_manager: Arc::new(Mutex::new(conn_manager)),

            // Store prefix for all keys
            prefix: prefix.to_string(),

            // Define Redis keys with shared prefix for consistent naming
            registration_tx_requests_key: format!("{}:registration_tx_requests", prefix),
            registration_tx_last_processed_key: format!(
                "{}:registration_tx_last_processed",
                prefix
            ),
            non_registration_tx_requests_key: format!("{}:non_registration_tx_requests", prefix),
            non_registration_tx_last_processed_key: format!(
                "{}:non_registration_tx_last_processed",
                prefix
            ),
            request_id_to_block_id_key: format!("{}:request_id_to_block_id", prefix),
            memos_key: format!("{}:memos", prefix),
            signatures_key: format!("{}:signatures", prefix),
            fee_collection_tasks_key: format!("{}:fee_collection_tasks", prefix),
            block_post_tasks_hi_key: format!("{}:block_post_tasks_hi", prefix),
            block_post_tasks_lo_key: format!("{}:block_post_tasks_lo", prefix),
        }
    }
}

//-----------------------------------------------------------------------------
// Storage Trait Implementation
//-----------------------------------------------------------------------------

/// Suppress warning about Redis's never type fallback
#[allow(dependency_on_unit_never_type_fallback)]
#[async_trait::async_trait(?Send)]
impl Storage for RedisStorage {
    /// Add a transaction to the appropriate queue
    ///
    /// This method adds a transaction request to either the registration or
    /// non-registration queue, depending on the transaction type.
    ///
    /// # Arguments
    /// * `is_registration` - Whether this is a registration transaction
    /// * `tx_request` - The transaction request to add
    async fn add_tx(
        &self,
        is_registration: bool,
        tx_request: TxRequest,
    ) -> Result<(), StorageError> {
        // Implement retry logic directly for this method
        let mut retries = 0;
        loop {
            let result = self.add_tx_inner(is_registration, tx_request.clone()).await;
            match result {
                Ok(_) => return Ok(()),
                Err(e) => {
                    retries += 1;
                    if retries >= MAX_RETRIES {
                        return Err(e);
                    }

                    // Log the error and retry with exponential backoff
                    log::warn!(
                        "Redis operation failed (retry {}/{}): {}",
                        retries,
                        MAX_RETRIES,
                        e
                    );
                    sleep(Duration::from_millis(RETRY_DELAY_MS * retries as u64)).await;
                }
            }
        }
    }

    /// Query a proposal for a transaction request
    ///
    /// This method retrieves a block proposal for a specific transaction request.
    /// It looks up the block ID associated with the request ID, then retrieves
    /// the memo for that block, and finally finds the proposal for the request.
    ///
    /// # Arguments
    /// * `request_id` - ID of the transaction request
    ///
    /// # Returns
    /// * `Some(BlockProposal)` if a proposal was found
    /// * `None` if no proposal exists for this request
    async fn query_proposal(
        &self,
        request_id: &str,
    ) -> Result<Option<BlockProposal>, StorageError> {
        self.with_retry(|| async {
            let mut conn = self.get_conn().await?;

            // Get block_id for request_id
            let block_id: Option<String> = conn
                .hget(&self.request_id_to_block_id_key, request_id)
                .await?;

            let block_id = match block_id {
                Some(id) => id,
                None => return Ok(None), // No block ID found for this request
            };

            // Get memo for block_id
            let serialized_memo: Option<String> = conn.hget(&self.memos_key, &block_id).await?;

            match serialized_memo {
                Some(serialized) => {
                    let memo: ProposalMemo = serde_json::from_str(&serialized)?;

                    // Find the position of the request_id in the memo
                    let position = memo
                        .tx_requests
                        .iter()
                        .position(|r| r.request_id == request_id);

                    match position {
                        Some(pos) => Ok(Some(memo.proposals[pos].clone())),
                        None => Ok(None), // Request ID not found in memo
                    }
                }
                None => Ok(None), // No memo found for this block ID
            }
        })
        .await
    }

    /// Process transaction requests and create proposal memos
    ///
    /// This method processes a batch of transaction requests from the queue,
    /// creates a proposal memo, and stores it for later use. It uses distributed
    /// locking to ensure that only one instance processes requests at a time.
    ///
    /// # Arguments
    /// * `is_registration` - Whether to process registration or non-registration transactions
    async fn process_requests(&self, is_registration: bool) -> Result<(), StorageError> {
        // Use a lock to prevent multiple instances from processing the same requests
        let lock_name = if is_registration {
            "process_registration_requests"
        } else {
            "process_non_registration_requests"
        };

        // Try to acquire the lock - if we can't, another instance is already processing
        if !self.acquire_lock(lock_name).await? {
            // Another instance is already processing, just return
            return Ok(());
        }

        // Make sure we release the lock when we're done
        let _result = self
            .with_retry(|| async {
                // Select the appropriate keys based on transaction type
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

                // Check if we should process requests:
                // 1. If queue is empty, nothing to process
                // 2. If queue is not full and we haven't waited long enough, wait for more transactions
                let current_time = chrono::Utc::now().timestamp() as u64;
                if (queue_len < NUM_SENDERS_IN_BLOCK
                    && current_time < last_processed + self.config.accepting_tx_interval)
                    || queue_len == 0
                {
                    return Ok(());
                }

                // Get up to NUM_SENDERS_IN_BLOCK requests
                let num_to_process = std::cmp::min(queue_len, NUM_SENDERS_IN_BLOCK);
                let serialized_requests: Vec<String> = conn
                    .lrange(requests_key, 0, num_to_process as isize - 1)
                    .await?;

                // Deserialize requests
                let mut tx_requests = Vec::with_capacity(num_to_process);
                for serialized in &serialized_requests {
                    let request_with_timestamp: TxRequestWithTimestamp =
                        serde_json::from_str(serialized)?;
                    tx_requests.push(request_with_timestamp.request);
                }

                // Create memo from the transaction requests
                let memo = ProposalMemo::from_tx_requests(
                    is_registration,
                    &tx_requests,
                    self.config.tx_timeout,
                );

                // Serialize the memo for storage
                let serialized_memo = serde_json::to_string(&memo)?;

                // Use a transaction to ensure atomicity of the following operations
                let mut pipe = redis::pipe();
                pipe.atomic();

                // Store memo by block ID
                pipe.hset(&self.memos_key, &memo.block_id, &serialized_memo);

                // Update request_id -> block_id mapping for each transaction
                for tx_request in &tx_requests {
                    pipe.hset(
                        &self.request_id_to_block_id_key,
                        &tx_request.request_id,
                        &memo.block_id,
                    );
                }

                // Remove processed requests from the queue
                pipe.ltrim(requests_key, num_to_process as isize, -1);

                // Update last processed timestamp
                pipe.set(last_processed_key, current_time.to_string());

                // Execute the transaction
                pipe.query_async(&mut conn).await?;

                Ok(())
            })
            .await;

        // Release the lock regardless of the result
        let _ = self.release_lock(lock_name).await;

        _result
    }

    /// Add a user signature for a transaction request
    ///
    /// This method adds a user signature for a specific transaction request.
    /// It verifies the signature against the memo before adding it.
    ///
    /// # Arguments
    /// * `request_id` - ID of the transaction request
    /// * `signature` - User signature to add
    async fn add_signature(
        &self,
        request_id: &str,
        signature: UserSignature,
    ) -> Result<(), StorageError> {
        self.with_retry(|| async {
            let mut conn = self.get_conn().await?;

            // Get block_id for request_id
            let block_id: Option<String> = conn
                .hget(&self.request_id_to_block_id_key, request_id)
                .await?;

            let block_id = block_id.ok_or_else(|| {
                StorageError::AddSignatureError(format!(
                    "block_id not found for request_id: {}",
                    request_id
                ))
            })?;

            // Get memo for block_id
            let serialized_memo: Option<String> = conn.hget(&self.memos_key, &block_id).await?;

            let serialized_memo = serialized_memo.ok_or_else(|| {
                StorageError::AddSignatureError(format!(
                    "memo not found for block_id: {}",
                    block_id
                ))
            })?;

            let memo: ProposalMemo = serde_json::from_str(&serialized_memo)?;

            // Verify signature
            signature
                .verify(memo.tx_tree_root, memo.expiry, memo.pubkey_hash)
                .map_err(|e| {
                    StorageError::AddSignatureError(format!("signature verification failed: {}", e))
                })?;

            // Serialize signature
            let serialized_signature = serde_json::to_string(&signature)?;

            // Add signature to the list for this block_id
            let signatures_key = format!("{}:{}", self.signatures_key, block_id);
            let _: () = conn.rpush(&signatures_key, serialized_signature).await?;

            Ok(())
        })
        .await
    }

    /// Process signatures and create block post tasks
    ///
    /// This method processes signatures for memos that have reached their
    /// proposing interval. It creates block post tasks and fee collection
    /// tasks as needed.
    async fn process_signatures(&self) -> Result<(), StorageError> {
        // Try to acquire the lock
        let lock_acquired = match self.acquire_lock("process_signatures").await {
            Ok(acquired) => acquired,
            Err(e) => {
                log::error!("Failed to acquire lock for process_signatures: {}", e);
                return Ok(());
            }
        };

        if !lock_acquired {
            // Another instance is already processing signatures
            return Ok(());
        }

        // Make sure we release the lock when we're done
        let result = self
            .with_retry(|| async {
                let mut conn = self.get_conn().await?;

                // Get all memo keys
                let memo_keys: Vec<String> = conn.hkeys(&self.memos_key).await?;

                let current_time = chrono::Utc::now().timestamp() as u64;

                for block_id in memo_keys {
                    // Get memo
                    let serialized_memo: Option<String> =
                        conn.hget(&self.memos_key, &block_id).await?;

                    let memo = match serialized_memo {
                        Some(serialized) => match serde_json::from_str::<ProposalMemo>(&serialized)
                        {
                            Ok(memo) => memo,
                            Err(e) => {
                                log::error!(
                                    "Failed to deserialize memo for block_id {}: {}",
                                    block_id,
                                    e
                                );
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
                    let serialized_signatures: Vec<String> =
                        conn.lrange(&signatures_key, 0, -1).await?;

                    // Skip if no signatures
                    if serialized_signatures.is_empty() {
                        continue;
                    }

                    // Deserialize signatures
                    let mut signatures = Vec::with_capacity(serialized_signatures.len());
                    for serialized in serialized_signatures {
                        match serde_json::from_str::<UserSignature>(&serialized) {
                            Ok(sig) => signatures.push(sig),
                            Err(e) => {
                                log::error!("Failed to deserialize signature: {}", e);
                                continue;
                            }
                        }
                    }

                    // Create block post task
                    let block_post_task = BlockPostTask::from_memo(&memo, &signatures);
                    let serialized_task = match serde_json::to_string(&block_post_task) {
                        Ok(task) => task,
                        Err(e) => {
                            log::error!("Failed to serialize block post task: {}", e);
                            continue;
                        }
                    };

                    // Use a transaction to ensure atomicity
                    let mut pipe = redis::pipe();
                    pipe.atomic();

                    // Add to high priority queue
                    pipe.rpush(&self.block_post_tasks_hi_key, &serialized_task);

                    // Add fee collection task if needed
                    if self.config.use_fee {
                        let fee_collection = FeeCollection {
                            use_collateral: self.config.use_collateral,
                            memo: memo.clone(),
                            signatures: signatures.clone(),
                        };

                        let serialized_fee_collection = match serde_json::to_string(&fee_collection)
                        {
                            Ok(collection) => collection,
                            Err(e) => {
                                log::error!("Failed to serialize fee collection: {}", e);
                                continue;
                            }
                        };

                        pipe.rpush(&self.fee_collection_tasks_key, &serialized_fee_collection);
                    }

                    // Remove memo and signatures
                    pipe.hdel(&self.memos_key, &block_id);
                    pipe.del(&signatures_key);

                    // Execute the transaction
                    if let Err(e) = pipe.query_async::<_, ()>(&mut conn).await {
                        log::error!(
                            "Failed to execute transaction for block_id {}: {}",
                            block_id,
                            e
                        );
                        continue;
                    }
                }

                Ok(())
            })
            .await;

        // Release the lock regardless of the result
        let _ = self.release_lock("process_signatures").await;

        result
    }

    /// Process fee collection tasks
    ///
    /// This method processes fee collection tasks and creates block post tasks
    /// for fee collection. It uses distributed locking to ensure that only one
    /// instance processes fee collection at a time.
    ///
    /// # Arguments
    /// * `store_vault_server_client` - Client for the store vault server
    async fn process_fee_collection(
        &self,
        store_vault_server_client: &StoreVaultServerClient,
    ) -> Result<(), StorageError> {
        // Try to acquire the lock
        if !self.acquire_lock("process_fee_collection").await? {
            // Another instance is already processing, just return
            return Ok(());
        }

        // Make sure we release the lock when we're done
        let _result = self
            .with_retry(|| async {
                let mut conn = self.get_conn().await?;

                // Use BLPOP with a short timeout to avoid race conditions between multiple instances
                let serialized_fee_collection: Option<(String, String)> =
                    conn.blpop(&self.fee_collection_tasks_key, 1).await?;

                // Return if there's no task
                let serialized_fee_collection = match serialized_fee_collection {
                    Some((_, value)) => value,
                    None => return Ok(()),
                };

                // Deserialize the fee collection task
                let fee_collection: FeeCollection =
                    serde_json::from_str(&serialized_fee_collection)?;

                // Process the fee collection
                let block_post_tasks = collect_fee(
                    store_vault_server_client,
                    self.config.fee_beneficiary,
                    &fee_collection,
                )
                .await?;

                // Use a transaction to add all tasks atomically
                if !block_post_tasks.is_empty() {
                    let mut pipe = redis::pipe();
                    pipe.atomic();

                    // Add resulting block post tasks to low priority queue
                    for task in block_post_tasks {
                        let serialized_task = serde_json::to_string(&task)?;
                        pipe.rpush(&self.block_post_tasks_lo_key, &serialized_task);
                    }

                    // Execute the transaction
                    pipe.query_async::<_, ()>(&mut conn).await?;
                }

                Ok(())
            })
            .await;

        // Release the lock regardless of the result
        let _ = self.release_lock("process_fee_collection").await;

        _result
    }

    /// Dequeue a block post task
    ///
    /// This method dequeues a block post task from either the high priority
    /// or low priority queue. It tries the high priority queue first, and
    /// falls back to the low priority queue if no tasks are available.
    ///
    /// # Returns
    /// * `Some(BlockPostTask)` if a task was dequeued
    /// * `None` if no tasks are available
    async fn dequeue_block_post_task(&self) -> Result<Option<BlockPostTask>, StorageError> {
        // We don't need a distributed lock here since BLPOP is atomic
        // and each instance should be able to dequeue tasks

        self.with_retry(|| async {
            let mut conn = self.get_conn().await?;

            // Try to get a task from high priority queue first using BLPOP with a short timeout
            let serialized_task: Option<(String, String)> =
                conn.blpop(&self.block_post_tasks_hi_key, 1).await?;

            // If no high priority task, try low priority queue
            let serialized_task = match serialized_task {
                Some((_, value)) => value,
                None => {
                    // Try low priority queue
                    let serialized_task: Option<(String, String)> =
                        conn.blpop(&self.block_post_tasks_lo_key, 1).await?;

                    match serialized_task {
                        Some((_, value)) => value,
                        None => return Ok(None),
                    }
                }
            };

            // Deserialize the task
            match serde_json::from_str::<BlockPostTask>(&serialized_task) {
                Ok(task) => Ok(Some(task)),
                Err(e) => {
                    log::error!("Failed to deserialize block post task: {}", e);
                    Ok(None)
                }
            }
        })
        .await
    }

    /// Enqueue an empty block for deposit checking
    ///
    /// This method adds an empty block post task to the low priority queue
    /// if enough time has passed since the last empty block was posted.
    /// It's used to periodically check for deposits in the L1 contract.
    async fn enqueue_empty_block(&self) -> Result<(), StorageError> {
        // If deposit check is disabled, do nothing
        if self.config.deposit_check_interval.is_none() {
            return Ok(());
        }

        let deposit_check_interval = self.config.deposit_check_interval.unwrap();

        // Try to acquire a lock to prevent multiple instances from posting empty blocks
        if !self.acquire_lock("enqueue_empty_block").await? {
            // Another instance is already processing, just return
            return Ok(());
        }

        // Make sure we release the lock when we're done
        let result = self
            .with_retry(|| async {
                let mut conn = self.get_conn().await?;

                // Key for storing the timestamp of the last empty block post
                let empty_block_posted_at_key = format!("{}:empty_block_posted_at", self.prefix);

                // Get the timestamp of the last empty block post
                let empty_block_posted_at: Option<String> =
                    conn.get(&empty_block_posted_at_key).await?;
                let empty_block_posted_at = empty_block_posted_at
                    .map(|s| s.parse::<u64>().unwrap_or(0))
                    .unwrap_or(0);

                let current_time = chrono::Utc::now().timestamp() as u64;

                // Check if enough time has passed since the last empty block post
                if empty_block_posted_at > 0
                    && current_time < empty_block_posted_at + deposit_check_interval
                {
                    // Not enough time has passed, do nothing
                    return Ok(());
                }

                // Create a default block post task (empty block)
                let block_post_task = BlockPostTask::default();
                let serialized_task = serde_json::to_string(&block_post_task)?;

                // Use a transaction to ensure atomicity
                let mut pipe = redis::pipe();
                pipe.atomic();

                // Add to low priority queue
                pipe.rpush(&self.block_post_tasks_lo_key, &serialized_task);

                // Update the timestamp of the last empty block post
                pipe.set(&empty_block_posted_at_key, current_time.to_string());

                // Execute the transaction
                pipe.query_async::<_, ()>(&mut conn).await?;

                Ok(())
            })
            .await;

        // Release the lock regardless of the result
        let _ = self.release_lock("enqueue_empty_block").await;

        result
    }
}
