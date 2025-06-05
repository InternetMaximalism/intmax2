use std::sync::Arc;

use intmax2_client_sdk::external_api::utils::{retry::with_retry, time::sleep_for};
use intmax2_interfaces::api::store_vault_server::interface::StoreVaultClientInterface;
use intmax2_zkp::{
    common::block_builder::{BlockProposal, UserSignature},
    constants::NUM_SENDERS_IN_BLOCK,
};

use rand::Rng as _;
use redis::{aio::ConnectionManager, AsyncCommands, Client, RedisResult, Script};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::app::{
    block_post::BlockPostTask,
    fee::{collect_fee, FeeCollection},
    storage::nonce_manager::NonceManager,
    types::{ProposalMemo, TxRequest},
};

use super::{
    config::StorageConfig, error::StorageError,
    nonce_manager::redis_nonce_manager::RedisNonceManager, Storage,
};

/// Timeout for distributed locks in seconds
const LOCK_TIMEOUT_SECONDS: usize = 10;

/// TTL for general Redis keys in seconds
const GENERAL_KEY_TTL_SECONDS: usize = 1200; // 20min

type Result<T> = std::result::Result<T, StorageError>;

/// Transaction request with timestamp
#[derive(Serialize, Deserialize, Clone, Debug)]
struct TxRequestWithTimestamp {
    /// Original transaction request
    request: TxRequest,

    /// Received timestamp (Unix timestamp)
    timestamp: u64,
}

pub struct RedisStorage {
    pub config: StorageConfig,
    conn_manager: Arc<Mutex<ConnectionManager>>,
    pub nonce_manager: RedisNonceManager,

    prefix: String,
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
    pub async fn new(config: &StorageConfig, nonce_manager: RedisNonceManager) -> Self {
        let cluster_id = config
            .cluster_id
            .clone()
            .unwrap_or_else(|| "default".to_string());
        let prefix = format!("block_builder:{cluster_id}");
        // Create Redis client with fallback to localhost if URL not provided
        let redis_url = config.redis_url.clone().expect("redis_url not found");
        let client = Client::open(redis_url).expect("Failed to create Redis client");

        // Create connection manager asynchronously
        let conn_manager = ConnectionManager::new(client)
            .await
            .expect("Failed to create Redis connection manager");

        log::info!("Redis storage initialized");

        Self {
            config: config.clone(),
            conn_manager: Arc::new(Mutex::new(conn_manager)),
            nonce_manager,

            // Store prefix for all keys
            prefix: prefix.to_string(),

            // Define Redis keys with shared prefix for consistent naming
            registration_tx_requests_key: format!("{prefix}:registration_tx_requests"),
            registration_tx_last_processed_key: format!("{prefix}:registration_tx_last_processed"),
            non_registration_tx_requests_key: format!("{prefix}:non_registration_tx_requests"),
            non_registration_tx_last_processed_key: format!(
                "{prefix}:non_registration_tx_last_processed"
            ),
            request_id_to_block_id_key: format!("{prefix}:request_id_to_block_id"),
            memos_key: format!("{prefix}:memos"),
            signatures_key: format!("{prefix}:signatures"),
            fee_collection_tasks_key: format!("{prefix}:fee_collection_tasks"),
            block_post_tasks_hi_key: format!("{prefix}:block_post_tasks_hi"),
            block_post_tasks_lo_key: format!("{prefix}:block_post_tasks_lo"),
        }
    }

    async fn get_conn(&self) -> RedisResult<ConnectionManager> {
        let conn = self.conn_manager.lock().await;
        Ok(conn.clone())
    }

    /// Acquire a distributed lock
    ///
    /// Uses Redis SET NX to ensure only one instance holds the lock.
    ///
    /// # Arguments
    /// * `lock_name` - Lock name to acquire
    ///
    /// # Returns
    /// * `Ok(true)` - Lock acquired
    /// * `Ok(false)` - Lock held by another instance
    /// * `Err` - Redis communication error
    async fn acquire_lock(&self, lock_name: &str) -> Result<bool> {
        let mut conn = self.get_conn().await?;
        let lock_key = format!("{}:lock:{}", self.prefix, lock_name);
        let instance_id = &self.config.block_builder_id;
        let result: Option<String> = redis::cmd("SET")
            .arg(&lock_key)
            .arg(instance_id)
            .arg("NX") // set if not exists
            .arg("EX") // expire in seconds
            .arg(LOCK_TIMEOUT_SECONDS)
            .query_async(&mut conn)
            .await?;

        if result.is_some() {
            log::debug!("Lock acquired: {lock_name}");
            Ok(true)
        } else {
            log::debug!("Lock already held: {lock_name}",);
            Ok(false)
        }
    }

    /// Release a distributed lock
    ///
    /// Releases lock only if owned by this instance using Lua for atomicity.
    ///
    /// # Arguments
    /// * `lock_name` - Lock name to release
    async fn release_lock(&self, lock_name: &str) -> Result<()> {
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
        log::debug!("Lock released: {lock_name}");
        Ok(())
    }
}

#[async_trait::async_trait(?Send)]
impl Storage for RedisStorage {
    /// Add transaction to queue
    ///
    /// Adds transaction to registration or non-registration queue.
    ///
    /// # Arguments
    /// * `is_registration` - If this is a registration transaction
    /// * `tx_request` - Transaction request to add
    async fn add_tx(&self, is_registration: bool, tx_request: TxRequest) -> Result<()> {
        log::debug!(
            "Adding transaction to {} queue with retries: {}",
            if is_registration {
                "registration"
            } else {
                "non-registration"
            },
            tx_request.request_id
        );

        with_retry(|| async {
            let tx_request = tx_request.clone();
            let request_id = tx_request.request_id.clone();
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

            // Set TTL for the queue
            let _: () = conn.expire(key, GENERAL_KEY_TTL_SECONDS as i64).await?;

            log::info!(
                "Transaction added to {} queue: {}",
                if is_registration {
                    "registration"
                } else {
                    "non-registration"
                },
                request_id
            );
            Result::Ok(())
        })
        .await?;
        Ok(())
    }

    /// Query proposal for transaction request
    ///
    /// Retrieves block proposal by looking up block ID from request ID.
    ///
    /// # Arguments
    /// * `request_id` - Transaction request ID
    ///
    /// # Returns
    /// * `Some(BlockProposal)` - Proposal found
    /// * `None` - No proposal exists
    async fn query_proposal(&self, request_id: &str) -> Result<Option<BlockProposal>> {
        let block_proposal = with_retry(|| async {
            let mut conn = self.get_conn().await?;

            // Get block_id for request_id
            let block_id: Option<String> = conn
                .hget(&self.request_id_to_block_id_key, request_id)
                .await?;

            let block_id = match block_id {
                Some(id) => id,
                None => return Result::Ok(None), // No block ID found for this request
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
        .await?;
        Ok(block_proposal)
    }

    /// Process transaction requests and create memos
    ///
    /// Processes request batch, creates proposal memo, and stores it with locking.
    ///
    /// # Arguments
    /// * `is_registration` - Process registration or non-registration transactions
    async fn process_requests(&self, is_registration: bool) -> Result<()> {
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
        // we don't use `with_retry` here because we want to ensure the lock is released quickly
        let result = {
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

            let nonce = self.nonce_manager.reserve_nonce(is_registration).await?;

            // Create memo from the transaction requests
            let memo = ProposalMemo::from_tx_requests(
                is_registration,
                self.config.block_builder_address,
                nonce,
                &tx_requests,
                self.config.tx_timeout,
            );
            log::info!(
                "constructed proposal block_id: {}, payload: {:?}",
                memo.block_id,
                memo.block_sign_payload.clone()
            );

            // Serialize the memo for storage
            let serialized_memo = serde_json::to_string(&memo)?;

            // Use a transaction to ensure atomicity of the following operations
            let mut pipe = redis::pipe();
            pipe.atomic();

            // Store memo by block ID
            pipe.hset(&self.memos_key, &memo.block_id, &serialized_memo);
            // Set TTL for memos hash
            pipe.expire(&self.memos_key, GENERAL_KEY_TTL_SECONDS as i64);

            // Update request_id -> block_id mapping for each transaction
            for tx_request in &tx_requests {
                pipe.hset(
                    &self.request_id_to_block_id_key,
                    &tx_request.request_id,
                    &memo.block_id,
                );
            }
            // Set TTL for request_id_to_block_id hash
            pipe.expire(
                &self.request_id_to_block_id_key,
                GENERAL_KEY_TTL_SECONDS as i64,
            );

            // Remove processed requests from the queue
            pipe.ltrim(requests_key, num_to_process as isize, -1);

            // Update last processed timestamp
            pipe.set(last_processed_key, current_time.to_string());
            // Set TTL for last processed timestamp key
            pipe.expire(last_processed_key, GENERAL_KEY_TTL_SECONDS as i64);

            // Execute the transaction
            let _: () = pipe.query_async(&mut conn).await?;

            Ok(())
        };

        // Release the lock regardless of the result
        let release_result = self.release_lock(lock_name).await;

        // If releasing the lock failed, log the error but still return the original result
        if let Err(e) = release_result {
            log::error!("Failed to release lock for {lock_name}: {e}");
        }

        log::info!(
            "Finished processing {} transaction requests",
            if is_registration {
                "registration"
            } else {
                "non-registration"
            }
        );
        result
    }

    /// Add user signature for transaction request
    ///
    /// Verifies signature against memo before adding it.
    ///
    /// # Arguments
    /// * `request_id` - Transaction request ID
    /// * `signature` - User signature to add
    async fn add_signature(&self, request_id: &str, signature: UserSignature) -> Result<()> {
        with_retry(|| async {
            let mut conn = self.get_conn().await?;

            // Get block_id for request_id
            let block_id: Option<String> = conn
                .hget(&self.request_id_to_block_id_key, request_id)
                .await?;

            let block_id = block_id.ok_or_else(|| {
                StorageError::AddSignatureError(format!(
                    "block_id not found for request_id: {request_id}"
                ))
            })?;

            // Get memo for block_id
            let serialized_memo: Option<String> = conn.hget(&self.memos_key, &block_id).await?;

            let serialized_memo = serialized_memo.ok_or_else(|| {
                StorageError::AddSignatureError(format!("memo not found for block_id: {block_id}"))
            })?;

            let memo: ProposalMemo = serde_json::from_str(&serialized_memo)?;

            // Verify signature
            signature
                .verify(&memo.block_sign_payload, memo.pubkey_hash)
                .map_err(|e| {
                    StorageError::AddSignatureError(format!("signature verification failed: {e}"))
                })?;

            // Serialize signature
            let serialized_signature = serde_json::to_string(&signature)?;

            // Add signature to the list for this block_id
            let signatures_key = format!("{}:{}", self.signatures_key, block_id);
            let _: () = conn.rpush(&signatures_key, serialized_signature).await?;

            // Set TTL for signatures key
            let _: () = conn
                .expire(&signatures_key, GENERAL_KEY_TTL_SECONDS as i64)
                .await?;

            Ok(())
        })
        .await
    }

    /// Process signatures and create block post tasks
    ///
    /// Processes signatures for ready memos and creates necessary tasks.
    async fn process_signatures(&self) -> Result<()> {
        // Try to acquire the lock
        let lock_acquired = match self.acquire_lock("process_signatures").await {
            Ok(acquired) => acquired,
            Err(e) => {
                log::error!("Failed to acquire lock for process_signatures: {e}");
                return Ok(());
            }
        };

        if !lock_acquired {
            // Another instance is already processing signatures
            return Ok(());
        }

        // Make sure we release the lock when we're done
        // we don't use `with_retry` here because we want to ensure the lock is released quickly
        let result = {
            let mut conn = self.get_conn().await?;

            // Get all memo keys
            let memo_keys: Vec<String> = conn.hkeys(&self.memos_key).await?;

            let current_time = chrono::Utc::now().timestamp() as u64;

            for block_id in memo_keys {
                // Get memo
                let serialized_memo: Option<String> = conn.hget(&self.memos_key, &block_id).await?;

                let memo = match serialized_memo {
                    Some(serialized) => match serde_json::from_str::<ProposalMemo>(&serialized) {
                        Ok(memo) => memo,
                        Err(e) => {
                            log::error!("Failed to deserialize memo for block_id {block_id}: {e}");
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
                            log::error!("Failed to deserialize signature: {e}");
                            continue;
                        }
                    }
                }

                // Create block post task
                let block_post_task = BlockPostTask::from_memo(&memo, &signatures);
                let serialized_task = match serde_json::to_string(&block_post_task) {
                    Ok(task) => task,
                    Err(e) => {
                        log::error!("Failed to serialize block post task: {e}");
                        continue;
                    }
                };

                // Use a transaction to ensure atomicity
                let mut pipe = redis::pipe();
                pipe.atomic();

                // Add to high priority queue
                pipe.rpush(&self.block_post_tasks_hi_key, &serialized_task);
                // Set TTL for high priority queue
                pipe.expire(
                    &self.block_post_tasks_hi_key,
                    GENERAL_KEY_TTL_SECONDS as i64,
                );

                // Add fee collection task if needed
                if self.config.use_fee {
                    let fee_collection = FeeCollection {
                        use_collateral: self.config.use_collateral,
                        memo: memo.clone(),
                        signatures: signatures.clone(),
                    };

                    let serialized_fee_collection = match serde_json::to_string(&fee_collection) {
                        Ok(collection) => collection,
                        Err(e) => {
                            log::error!("Failed to serialize fee collection: {e}");
                            continue;
                        }
                    };

                    pipe.rpush(&self.fee_collection_tasks_key, &serialized_fee_collection);
                    // Set TTL for fee collection tasks queue
                    pipe.expire(
                        &self.fee_collection_tasks_key,
                        GENERAL_KEY_TTL_SECONDS as i64,
                    );
                }

                // Remove memo and signatures
                pipe.hdel(&self.memos_key, &block_id);
                pipe.del(&signatures_key);

                // Execute the transaction
                if let Err(e) = pipe.query_async::<()>(&mut conn).await {
                    log::error!("Failed to execute transaction for block_id {block_id}: {e}");
                    continue;
                }
            }

            Ok(())
        };

        // Release the lock regardless of the result
        let release_result = self.release_lock("process_signatures").await;

        // If releasing the lock failed, log the error but still return the original result
        if let Err(e) = release_result {
            log::error!("Failed to release lock for process_signatures: {e}");
        }

        result
    }

    /// Process fee collection tasks
    ///
    /// Processes fee collection and creates block post tasks with locking.
    ///
    /// # Arguments
    /// * `store_vault_server_client` - Store vault server client
    async fn process_fee_collection(
        &self,
        store_vault_server_client: &dyn StoreVaultClientInterface,
    ) -> Result<()> {
        // Try to acquire the lock
        if !self.acquire_lock("process_fee_collection").await? {
            // Another instance is already processing, just return
            return Ok(());
        }

        // Make sure we release the lock when we're done
        // we don't use `with_retry` here because we want to ensure the lock is released quickly
        let result = {
            let mut conn = self.get_conn().await?;

            // Use BLPOP with a short timeout to avoid race conditions between multiple instances
            let serialized_fee_collection: Option<(String, String)> =
                conn.blpop(&self.fee_collection_tasks_key, 1.0).await?;

            // Return if there's no task
            let serialized_fee_collection = match serialized_fee_collection {
                Some((_, value)) => value,
                None => return Ok(()),
            };

            // Deserialize the fee collection task
            let fee_collection: FeeCollection = serde_json::from_str(&serialized_fee_collection)?;

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
                // Set TTL for low priority queue
                pipe.expire(
                    &self.block_post_tasks_lo_key,
                    GENERAL_KEY_TTL_SECONDS as i64,
                );

                // Execute the transaction
                let _: () = pipe.query_async(&mut conn).await?;
            }

            Ok(())
        };

        // Release the lock regardless of the result
        let release_result = self.release_lock("process_fee_collection").await;

        // If releasing the lock failed, log the error but still return the original result
        if let Err(e) = release_result {
            log::error!("Failed to release lock for process_fee_collection: {e}");
        }

        result
    }

    /// Enqueue empty block for deposit checking
    ///
    /// Adds empty block task if enough time passed since last check.
    async fn enqueue_empty_block(&self) -> Result<()> {
        // If deposit check is disabled, do nothing
        if self.config.deposit_check_interval.is_none() {
            return Ok(());
        }

        // Try to acquire a lock to prevent multiple instances from posting empty blocks
        if !self.acquire_lock("enqueue_empty_block").await? {
            // Another instance is already processing, just return
            return Ok(());
        }

        // Make sure we release the lock when we're done
        // we don't use `with_retry` here because we want to ensure the lock is released quickly
        let result = {
            let mut conn = self.get_conn().await?;

            // Key for storing the timestamp of the last empty block post
            let empty_block_posted_at_key = format!("{}:empty_block_posted_at", self.prefix);

            // Get the timestamp of the last empty block post
            let empty_block_posted_at: Option<String> =
                conn.get(&empty_block_posted_at_key).await?;
            let empty_block_posted_at = empty_block_posted_at
                .map(|s| s.parse::<u64>().unwrap_or(0))
                .unwrap_or(0);
            let multiplier = rand::thread_rng().gen_range(0.5..=1.5);
            let deposit_check_interval =
                (self.config.deposit_check_interval.unwrap() as f64 * multiplier) as u64;

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
            // Set TTL for low priority queue
            pipe.expire(
                &self.block_post_tasks_lo_key,
                GENERAL_KEY_TTL_SECONDS as i64,
            );

            // Update the timestamp of the last empty block post
            pipe.set(&empty_block_posted_at_key, current_time.to_string());
            // Set TTL for empty block posted timestamp key
            pipe.expire(&empty_block_posted_at_key, GENERAL_KEY_TTL_SECONDS as i64);

            // Execute the transaction
            let _: () = pipe.query_async(&mut conn).await?;

            Ok(())
        };

        // Release the lock regardless of the result
        let release_result = self.release_lock("enqueue_empty_block").await;

        // If releasing the lock failed, log the error but still return the original result
        if let Err(e) = release_result {
            log::error!("Failed to release lock for enqueue_empty_block: {e}");
        }

        result
    }

    /// Dequeue block post task
    ///
    /// Gets task from high priority queue first, then low priority if none available.
    ///
    /// # Returns
    /// * `Some(BlockPostTask)` - Task dequeued
    /// * `None` - No tasks available
    async fn dequeue_block_post_task(&self) -> Result<Option<BlockPostTask>> {
        let mut conn = self.get_conn().await?;

        /* ---------- 1. High-priority queue: peek (non-destructive) ---------- */
        if let Some(task_json) = conn
            .lindex::<_, Option<String>>(&self.block_post_tasks_hi_key, 0)
            .await?
        {
            // ------------- decode task & nonce check -------------
            let peek_task: BlockPostTask = serde_json::from_str(&task_json)?;
            let is_registration = peek_task.block_sign_payload.is_registration_block;
            let block_nonce = peek_task.block_sign_payload.block_builder_nonce;

            let smallest_reserved_nonce = self
                .nonce_manager
                .smallest_reserved_nonce(is_registration)
                .await?;

            /* ----- 2A. Nonce matches: pop immediately ----- */
            if smallest_reserved_nonce == Some(block_nonce) {
                if let Some(popped_json) = conn
                    .lpop::<_, Option<String>>(&self.block_post_tasks_hi_key, None)
                    .await?
                {
                    let task: BlockPostTask = serde_json::from_str(&popped_json)?;
                    self.nonce_manager
                        .release_nonce(
                            task.block_sign_payload.block_builder_nonce,
                            task.block_sign_payload.is_registration_block,
                        )
                        .await?;
                    log::info!(
                        "Dequeued high-priority task (nonce match): id={}",
                        task.block_id
                    );
                    return Ok(Some(task));
                }
            } else {
                log::info!(
                    "High-priority head nonce {} ≠ smallest {:?}. Waiting {} then processing anyway.",
                    block_nonce,
                    smallest_reserved_nonce,
                    self.config.nonce_waiting_time,
                );
                // sleep for nonce waiting time
                sleep_for(self.config.nonce_waiting_time).await;
                if let Some(popped_json) = conn
                    .lpop::<_, Option<String>>(&self.block_post_tasks_hi_key, None)
                    .await?
                {
                    let task: BlockPostTask = serde_json::from_str(&popped_json)?;
                    self.nonce_manager
                        .release_nonce(
                            task.block_sign_payload.block_builder_nonce,
                            task.block_sign_payload.is_registration_block,
                        )
                        .await?;
                    log::info!(
                        "Dequeued high-priority task after wait: id={}",
                        task.block_id
                    );
                    return Ok(Some(task));
                }
            }
        }
        const BLPOP_TIMEOUT_SEC: f64 = 1.0;
        if let Some((_key, task_json)) = conn
            .blpop::<_, Option<(String, String)>>(&self.block_post_tasks_lo_key, BLPOP_TIMEOUT_SEC)
            .await?
        {
            let task = serde_json::from_str::<BlockPostTask>(&task_json)?;
            log::info!("Dequeued low-priority task: id={}", task.block_id);
            return Ok(Some(task));
        }
        Ok(None)
    }
}

#[cfg(test)]
pub mod test_redis_helper {
    use std::panic;
    // For redis
    use std::{
        net::TcpListener,
        process::{Command, Output, Stdio},
    };

    pub fn run_redis_docker(port: u16, container_name: &str) -> Output {
        let port_arg = format!("{port}:6379");

        let output = Command::new("docker")
            .args([
                "run",
                "-d",
                "--rm",
                "--name",
                container_name,
                "-p",
                &port_arg,
                "redis:latest",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("Error during Redis container startup");

        output
    }

    pub fn stop_redis_docker(container_name: &str) -> Output {
        let output = Command::new("docker")
            .args(["stop", container_name])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("Error during Redis container stopping");

        output
    }

    pub fn find_free_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .expect("Failed to bind to address")
            .local_addr()
            .unwrap()
            .port()
    }

    pub fn assert_and_stop<F: FnOnce() + panic::UnwindSafe>(cont_name: &str, f: F) {
        let res = panic::catch_unwind(f);

        if let Err(panic_info) = res {
            stop_redis_docker(cont_name);
            panic::resume_unwind(panic_info);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::storage::nonce_manager::config::NonceManagerConfig;
    use std::panic::AssertUnwindSafe;

    use super::*;
    use alloy::{
        providers::{mock::Asserter, ProviderBuilder},
        sol_types::SolCall,
    };
    use intmax2_client_sdk::{
        client::error::ClientError,
        external_api::contract::{
            convert::convert_address_to_alloy,
            rollup_contract::{Rollup, RollupContract},
        },
    };
    use intmax2_zkp::ethereum_types::{address::Address, u256::U256, u32limb_trait::U32LimbTrait};
    use uuid::Uuid;

    use test_redis_helper::{assert_and_stop, find_free_port, run_redis_docker, stop_redis_docker};

    async fn setup_test_storage(instance_id: &str, redis_port: &str) -> RedisStorage {
        let config = StorageConfig {
            use_fee: true,
            use_collateral: true,
            block_builder_address: Address::zero(),
            fee_beneficiary: U256::default(),
            tx_timeout: 80,
            accepting_tx_interval: 40,
            proposing_block_interval: 10,
            deposit_check_interval: Some(20),
            nonce_waiting_time: 5,
            redis_url: Some(redis_port.to_string()),
            cluster_id: Some(instance_id.to_string()),
            block_builder_id: Uuid::new_v4().to_string(),
        };
        let nonce_config = NonceManagerConfig {
            block_builder_address: convert_address_to_alloy(config.block_builder_address),
            redis_url: config.redis_url.clone(),
            cluster_id: config.cluster_id.clone(),
        };
        let provider_asserter = Asserter::new();
        // add nonce assertions
        let reg_nonce_return = Rollup::builderRegistrationNonceCall::abi_encode_returns(&1);
        provider_asserter.push_success(&reg_nonce_return);
        let non_reg_nonce_return = Rollup::builderNonRegistrationNonceCall::abi_encode_returns(&1);
        provider_asserter.push_success(&non_reg_nonce_return);
        let provider = ProviderBuilder::default()
            .with_gas_estimation()
            .with_simple_nonce_management()
            .fetch_chain_id()
            .connect_mocked_client(provider_asserter);
        let rollup = RollupContract::new(provider, Default::default());
        let nonce_manager = RedisNonceManager::new(nonce_config, rollup).await;
        RedisStorage::new(&config, nonce_manager).await
    }

    #[tokio::test]
    async fn test_acquire_release_lock() {
        let port: u16 = 6381;
        let cont_name = "redis-test-acquire-release";

        // Run docker image
        stop_redis_docker(cont_name);
        let output = run_redis_docker(port, cont_name);
        assert!(
            output.status.success(),
            "Couldn't start {}: {}",
            cont_name,
            String::from_utf8_lossy(&output.stderr)
        );

        // Create RedisStorage and test locks
        let redis1 = setup_test_storage("redis-test", "redis://localhost:6381").await;
        let redis2 = setup_test_storage("redis-test", "redis://localhost:6381").await;

        let acquired1 = redis1.acquire_lock("test_lock").await.unwrap();
        assert_and_stop(cont_name, || {
            assert!(acquired1, "Couldn't acquire lock for redis1")
        });

        let acquired2 = redis2.acquire_lock("test_lock").await.unwrap();
        assert_and_stop(cont_name, || {
            assert!(!acquired2, "Could acquire lock for redis2")
        });

        redis1.release_lock("test_lock").await.unwrap();

        let acquired2_after = redis2.acquire_lock("test_lock").await.unwrap();
        assert_and_stop(cont_name, || {
            assert!(acquired2_after, "Couldn't acquire lock for redis-test-2")
        });

        // Stop docker image
        let output = stop_redis_docker(cont_name);
        assert!(
            output.status.success(),
            "Couldn't stop {}: {}",
            cont_name,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[tokio::test]
    async fn test_empty_process_requests() {
        let port = find_free_port();
        let cont_name = "redis-test-process-requests";

        // Run docker image
        stop_redis_docker(cont_name);
        let output = run_redis_docker(port, cont_name);
        assert!(
            output.status.success(),
            "Couldn't start {}: {}",
            cont_name,
            String::from_utf8_lossy(&output.stderr)
        );

        // Create redis storage
        let redis_storage =
            setup_test_storage("redis-test", &format!("redis://localhost:{port}")).await;
        let res = redis_storage.process_requests(true).await;
        assert_and_stop(cont_name, AssertUnwindSafe(|| assert!(res.is_ok())));

        // Stop docker image
        let output = stop_redis_docker(cont_name);
        assert!(
            output.status.success(),
            "Couldn't stop {}: {}",
            cont_name,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[tokio::test]
    async fn test_non_empty_process_requests() {
        let port = find_free_port();
        let cont_name = "redis-test-non-empty-process-requests";

        // Run docker image
        stop_redis_docker(cont_name);
        let output = run_redis_docker(port, cont_name);
        assert!(
            output.status.success(),
            "Couldn't start {}: {}",
            cont_name,
            String::from_utf8_lossy(&output.stderr)
        );

        // Create redis storage
        let redis_storage =
            setup_test_storage("redis-test", &format!("redis://localhost:{port}")).await;

        let res = redis_storage.add_tx(true, TxRequest::default()).await;
        assert_and_stop(cont_name, AssertUnwindSafe(|| assert!(res.is_ok())));

        let res = redis_storage.process_requests(true).await;
        assert_and_stop(cont_name, AssertUnwindSafe(|| assert!(res.is_ok())));

        let res = redis_storage
            .query_proposal(Uuid::default().to_string().as_str())
            .await;
        assert_and_stop(cont_name, AssertUnwindSafe(|| assert!(res.is_ok())));

        let block_proposal = res.unwrap().unwrap();
        assert_and_stop(cont_name, || {
            assert!(block_proposal.block_sign_payload.is_registration_block)
        });
        assert_and_stop(cont_name, || {
            assert_eq!(block_proposal.pubkeys.len(), NUM_SENDERS_IN_BLOCK)
        });

        let res = block_proposal
            .verify(TxRequest::default().tx)
            .map_err(|e| ClientError::InvalidBlockProposal(format!("{e}")));
        assert_and_stop(cont_name, AssertUnwindSafe(|| assert!(res.is_ok())));

        // Stop docker image
        let output = stop_redis_docker(cont_name);
        assert!(
            output.status.success(),
            "Couldn't stop {}: {}",
            cont_name,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[tokio::test]
    async fn test_enqueue_dequeue_empty_block_post() {
        let port = find_free_port();
        let cont_name = "redis-test-enqueue-dequeue-empty-block-post";

        // Run docker image
        stop_redis_docker(cont_name);
        let output = run_redis_docker(port, cont_name);
        assert!(
            output.status.success(),
            "Couldn't start {}: {}",
            cont_name,
            String::from_utf8_lossy(&output.stderr)
        );

        // Create redis storage
        let redis_storage =
            setup_test_storage("redis-test", &format!("redis://localhost:{port}")).await;

        // Test enqueue and dequeue block post task
        let res = redis_storage.enqueue_empty_block().await;
        assert_and_stop(cont_name, AssertUnwindSafe(|| assert!(res.is_ok())));

        let res = redis_storage.dequeue_block_post_task().await;
        assert_and_stop(cont_name, AssertUnwindSafe(|| assert!(res.is_ok())));

        let block_post_task = res.unwrap().unwrap();

        assert_and_stop(cont_name, || assert!(block_post_task.force_post));

        assert_and_stop(cont_name, || {
            assert!(!block_post_task.block_sign_payload.is_registration_block)
        });
        assert_and_stop(cont_name, || {
            assert_eq!(
                block_post_task.block_sign_payload.block_builder_address,
                Address::default()
            )
        });
        assert_and_stop(cont_name, || {
            assert_eq!(
                block_post_task.block_sign_payload.block_builder_nonce,
                u32::default()
            )
        });

        assert_and_stop(cont_name, || {
            assert_eq!(block_post_task.pubkeys.len(), NUM_SENDERS_IN_BLOCK)
        });

        assert_and_stop(cont_name, || assert!(block_post_task.account_ids.is_some()));

        // Stop docker image
        let output = stop_redis_docker(cont_name);
        assert!(
            output.status.success(),
            "Couldn't stop {}: {}",
            cont_name,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
