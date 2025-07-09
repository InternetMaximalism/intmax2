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
    storage::{nonce_manager::NonceManager, utils::remove_duplicate_signatures},
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

/// Redis key manager for consistent key naming
struct RedisKeyManager {
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

impl RedisKeyManager {
    fn new(cluster_id: &str) -> Self {
        let prefix = format!("block_builder:{cluster_id}");
        Self {
            prefix: prefix.clone(),
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

    fn lock_key(&self, lock_name: &str) -> String {
        format!("{}:lock:{}", self.prefix, lock_name)
    }

    fn signatures_key(&self, block_id: &str) -> String {
        format!("{}:{}", self.signatures_key, block_id)
    }

    fn empty_block_posted_at_key(&self) -> String {
        format!("{}:empty_block_posted_at", self.prefix)
    }
}

/// Lock manager for distributed locking
struct RedisLockManager<'a> {
    storage: &'a RedisStorage,
    keys: &'a RedisKeyManager,
}

impl<'a> RedisLockManager<'a> {
    fn new(storage: &'a RedisStorage, keys: &'a RedisKeyManager) -> Self {
        Self { storage, keys }
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
        let mut conn = self.storage.get_conn().await?;
        let lock_key = self.keys.lock_key(lock_name);
        let instance_id = &self.storage.config.block_builder_id;
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
            log::debug!("Lock already held: {lock_name}");
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
        let mut conn = self.storage.get_conn().await?;
        let lock_key = self.keys.lock_key(lock_name);
        let instance_id = &self.storage.config.block_builder_id;

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

    async fn with_lock<F, Fut, R>(&self, lock_name: &str, f: F) -> Result<R>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<R>>,
    {
        if !self.acquire_lock(lock_name).await? {
            return Err(StorageError::LockNotAcquired(lock_name.to_string()));
        }

        let result = f().await;

        if let Err(e) = self.release_lock(lock_name).await {
            log::error!("Failed to release lock for {lock_name}: {e}");
        }

        result
    }
}

pub struct RedisStorage {
    pub config: StorageConfig,
    conn_manager: Arc<Mutex<ConnectionManager>>,
    pub nonce_manager: RedisNonceManager,
    keys: RedisKeyManager,
}

impl RedisStorage {
    pub async fn new(config: &StorageConfig, nonce_manager: RedisNonceManager) -> Result<Self> {
        let cluster_id = config
            .cluster_id
            .clone()
            .unwrap_or_else(|| "default".to_string());

        let redis_url = config
            .redis_url
            .clone()
            .ok_or_else(|| StorageError::ConfigurationError("redis_url not found".to_string()))?;

        let client = Client::open(redis_url).map_err(|e| StorageError::RedisError(e))?;

        let conn_manager = ConnectionManager::new(client)
            .await
            .map_err(|e| StorageError::RedisError(e))?;

        log::info!("Redis storage initialized");

        Ok(Self {
            config: config.clone(),
            conn_manager: Arc::new(Mutex::new(conn_manager)),
            nonce_manager,
            keys: RedisKeyManager::new(&cluster_id),
        })
    }

    async fn get_conn(&self) -> RedisResult<ConnectionManager> {
        let conn = self.conn_manager.lock().await;
        Ok(conn.clone())
    }

    fn lock_manager(&self) -> RedisLockManager<'_> {
        RedisLockManager::new(self, &self.keys)
    }

    async fn process_requests_inner(&self, is_registration: bool) -> Result<()> {
        let tx_requests = self.get_and_validate_tx_requests(is_registration).await?;
        if tx_requests.is_empty() {
            return Ok(());
        }

        let nonce = self.nonce_manager.reserve_nonce(is_registration).await?;
        let memo = self.create_proposal_memo(is_registration, nonce, &tx_requests);
        self.store_memo_and_update_mappings(&memo, &tx_requests, is_registration)
            .await?;

        Ok(())
    }

    async fn get_and_validate_tx_requests(&self, is_registration: bool) -> Result<Vec<TxRequest>> {
        let (requests_key, last_processed_key) = self.get_queue_keys(is_registration);
        let mut conn = self.get_conn().await?;

        let last_processed = self
            .get_last_processed_timestamp(&mut conn, last_processed_key)
            .await?;
        let queue_len: usize = conn.llen(requests_key).await?;

        if !self.should_process_requests(queue_len, last_processed) {
            return Ok(Vec::new());
        }

        let num_to_process = std::cmp::min(queue_len, NUM_SENDERS_IN_BLOCK);
        let serialized_requests: Vec<String> = conn
            .lrange(requests_key, 0, num_to_process as isize - 1)
            .await?;

        let mut tx_requests = Vec::with_capacity(num_to_process);
        for serialized in &serialized_requests {
            let request_with_timestamp: TxRequestWithTimestamp = serde_json::from_str(serialized)?;
            tx_requests.push(request_with_timestamp.request);
        }

        Ok(tx_requests)
    }

    async fn store_memo_and_update_mappings(
        &self,
        memo: &ProposalMemo,
        tx_requests: &[TxRequest],
        is_registration: bool,
    ) -> Result<()> {
        let (requests_key, last_processed_key) = self.get_queue_keys(is_registration);
        let serialized_memo = serde_json::to_string(memo)?;
        let mut conn = self.get_conn().await?;
        let current_time = chrono::Utc::now().timestamp() as u64;

        let mut pipe = redis::pipe();
        pipe.atomic();

        // Store memo by block ID
        pipe.hset(&self.keys.memos_key, &memo.block_id, &serialized_memo);
        pipe.expire(&self.keys.memos_key, GENERAL_KEY_TTL_SECONDS as i64);

        // Update request_id -> block_id mapping for each transaction
        for tx_request in tx_requests {
            pipe.hset(
                &self.keys.request_id_to_block_id_key,
                &tx_request.request_id,
                &memo.block_id,
            );
        }
        pipe.expire(
            &self.keys.request_id_to_block_id_key,
            GENERAL_KEY_TTL_SECONDS as i64,
        );

        // Remove processed requests from the queue
        pipe.ltrim(requests_key, tx_requests.len() as isize, -1);

        // Update last processed timestamp
        pipe.set(last_processed_key, current_time.to_string());
        pipe.expire(last_processed_key, GENERAL_KEY_TTL_SECONDS as i64);

        // Execute the transaction
        let _: () = pipe.query_async(&mut conn).await?;
        Ok(())
    }

    fn get_queue_keys(&self, is_registration: bool) -> (&str, &str) {
        if is_registration {
            (
                &self.keys.registration_tx_requests_key,
                &self.keys.registration_tx_last_processed_key,
            )
        } else {
            (
                &self.keys.non_registration_tx_requests_key,
                &self.keys.non_registration_tx_last_processed_key,
            )
        }
    }

    async fn get_last_processed_timestamp(
        &self,
        conn: &mut ConnectionManager,
        key: &str,
    ) -> Result<u64> {
        let last_processed: Option<String> = conn.get(key).await?;
        Ok(last_processed
            .map(|s| s.parse::<u64>().unwrap_or(0))
            .unwrap_or(0))
    }

    fn should_process_requests(&self, queue_len: usize, last_processed: u64) -> bool {
        let current_time = chrono::Utc::now().timestamp() as u64;

        if queue_len == 0 {
            return false;
        }

        // Process if queue is full or enough time has passed
        queue_len >= NUM_SENDERS_IN_BLOCK
            || current_time >= last_processed + self.config.accepting_tx_interval
    }

    fn create_proposal_memo(
        &self,
        is_registration: bool,
        nonce: u32,
        tx_requests: &[TxRequest],
    ) -> ProposalMemo {
        let memo = ProposalMemo::from_tx_requests(
            is_registration,
            self.config.block_builder_address,
            nonce,
            tx_requests,
            self.config.tx_timeout,
        );
        log::info!(
            "constructed proposal block_id: {}, payload: {:?}",
            memo.block_id,
            memo.block_sign_payload.clone()
        );
        memo
    }

    async fn process_signatures_inner(&self) -> Result<()> {
        let mut conn = self.get_conn().await?;
        let memo_keys: Vec<String> = conn.hkeys(&self.keys.memos_key).await?;
        let current_time = chrono::Utc::now().timestamp() as u64;

        for block_id in memo_keys {
            if let Err(e) = self
                .process_single_memo(&mut conn, &block_id, current_time)
                .await
            {
                log::error!("Failed to process memo for block_id {block_id}: {e}");
                continue;
            }
        }

        Ok(())
    }

    async fn process_single_memo(
        &self,
        conn: &mut ConnectionManager,
        block_id: &str,
        current_time: u64,
    ) -> Result<()> {
        let memo = self
            .get_memo_for_processing(conn, block_id, current_time)
            .await?;
        let memo = match memo {
            Some(m) => m,
            None => return Ok(()),
        };

        let signatures = self.get_and_clean_signatures(conn, block_id).await?;

        if !signatures.is_empty() {
            self.enqueue_block_post_task(conn, &memo, &signatures)
                .await?;
        }

        self.cleanup_memo_and_create_fee_task(conn, block_id, &memo, &signatures)
            .await?;
        Ok(())
    }

    async fn get_memo_for_processing(
        &self,
        conn: &mut ConnectionManager,
        block_id: &str,
        current_time: u64,
    ) -> Result<Option<ProposalMemo>> {
        let serialized_memo: Option<String> = conn.hget(&self.keys.memos_key, block_id).await?;

        let memo = match serialized_memo {
            Some(serialized) => serde_json::from_str::<ProposalMemo>(&serialized)?,
            None => return Ok(None),
        };

        // Check if it's time to process this memo
        if current_time <= memo.created_at + self.config.proposing_block_interval {
            return Ok(None);
        }

        Ok(Some(memo))
    }

    async fn get_and_clean_signatures(
        &self,
        conn: &mut ConnectionManager,
        block_id: &str,
    ) -> Result<Vec<UserSignature>> {
        let signatures_key = self.keys.signatures_key(block_id);
        let serialized_signatures: Vec<String> = conn.lrange(&signatures_key, 0, -1).await?;
        let mut signatures = serialized_signatures
            .iter()
            .map(|s| serde_json::from_str::<UserSignature>(s))
            .collect::<serde_json::Result<Vec<_>>>()?;

        remove_duplicate_signatures(&mut signatures);
        Ok(signatures)
    }

    async fn enqueue_block_post_task(
        &self,
        conn: &mut ConnectionManager,
        memo: &ProposalMemo,
        signatures: &[UserSignature],
    ) -> Result<()> {
        let block_post_task = BlockPostTask::from_memo(memo, signatures);
        let serialized_task = serde_json::to_string(&block_post_task)?;
        let mut pipe = redis::pipe();
        pipe.atomic();
        pipe.rpush(&self.keys.block_post_tasks_hi_key, &serialized_task);
        pipe.expire(
            &self.keys.block_post_tasks_hi_key,
            GENERAL_KEY_TTL_SECONDS as i64,
        );
        pipe.query_async::<()>(conn).await?;
        Ok(())
    }

    async fn cleanup_memo_and_create_fee_task(
        &self,
        conn: &mut ConnectionManager,
        block_id: &str,
        memo: &ProposalMemo,
        signatures: &[UserSignature],
    ) -> Result<()> {
        let signatures_key = self.keys.signatures_key(block_id);
        let mut pipe = redis::pipe();
        pipe.atomic();

        // Add fee collection task if needed
        if self.config.use_fee {
            let fee_collection = FeeCollection {
                use_collateral: self.config.use_collateral,
                memo: memo.clone(),
                signatures: signatures.to_vec(),
            };
            let serialized_fee_collection = serde_json::to_string(&fee_collection)?;
            pipe.rpush(
                &self.keys.fee_collection_tasks_key,
                &serialized_fee_collection,
            );
            pipe.expire(
                &self.keys.fee_collection_tasks_key,
                GENERAL_KEY_TTL_SECONDS as i64,
            );
        }

        // Remove memo and signatures
        pipe.hdel(&self.keys.memos_key, block_id);
        pipe.del(&signatures_key);

        pipe.query_async::<()>(conn).await?;
        Ok(())
    }

    async fn process_fee_collection_inner(
        &self,
        store_vault_server_client: &dyn StoreVaultClientInterface,
    ) -> Result<()> {
        let mut conn = self.get_conn().await?;

        let serialized_fee_collection: Option<(String, String)> =
            conn.blpop(&self.keys.fee_collection_tasks_key, 1.0).await?;

        let serialized_fee_collection = match serialized_fee_collection {
            Some((_, value)) => value,
            None => return Ok(()),
        };

        let fee_collection: FeeCollection = serde_json::from_str(&serialized_fee_collection)?;
        let block_post_tasks = collect_fee(
            store_vault_server_client,
            self.config.beneficiary,
            &fee_collection,
        )
        .await?;

        if !block_post_tasks.is_empty() {
            self.enqueue_block_post_tasks(&mut conn, &block_post_tasks)
                .await?;
        }

        Ok(())
    }

    async fn enqueue_block_post_tasks(
        &self,
        conn: &mut ConnectionManager,
        tasks: &[BlockPostTask],
    ) -> Result<()> {
        let mut pipe = redis::pipe();
        pipe.atomic();

        for task in tasks {
            let serialized_task = serde_json::to_string(task)?;
            pipe.rpush(&self.keys.block_post_tasks_lo_key, &serialized_task);
        }
        pipe.expire(
            &self.keys.block_post_tasks_lo_key,
            GENERAL_KEY_TTL_SECONDS as i64,
        );
        pipe.query_async::<()>(conn).await?;
        Ok(())
    }

    async fn enqueue_empty_block_inner(&self) -> Result<()> {
        let mut conn = self.get_conn().await?;
        let empty_block_posted_at_key = self.keys.empty_block_posted_at_key();

        if !self
            .should_enqueue_empty_block(&mut conn, &empty_block_posted_at_key)
            .await?
        {
            return Ok(());
        }

        let block_post_task = BlockPostTask::default();
        let serialized_task = serde_json::to_string(&block_post_task)?;
        let current_time = chrono::Utc::now().timestamp() as u64;

        let mut pipe = redis::pipe();
        pipe.atomic();
        pipe.rpush(&self.keys.block_post_tasks_lo_key, &serialized_task);
        pipe.expire(
            &self.keys.block_post_tasks_lo_key,
            GENERAL_KEY_TTL_SECONDS as i64,
        );
        pipe.set(&empty_block_posted_at_key, current_time.to_string());
        pipe.expire(&empty_block_posted_at_key, GENERAL_KEY_TTL_SECONDS as i64);
        pipe.query_async::<()>(&mut conn).await?;

        Ok(())
    }

    async fn should_enqueue_empty_block(
        &self,
        conn: &mut ConnectionManager,
        key: &str,
    ) -> Result<bool> {
        let empty_block_posted_at: Option<String> = conn.get(key).await?;
        let empty_block_posted_at = empty_block_posted_at
            .map(|s| s.parse::<u64>().unwrap_or(0))
            .unwrap_or(0);

        let multiplier = rand::thread_rng().gen_range(0.5..=1.5);
        let deposit_check_interval =
            (self.config.deposit_check_interval.unwrap() as f64 * multiplier) as u64;
        let current_time = chrono::Utc::now().timestamp() as u64;

        Ok(empty_block_posted_at == 0
            || current_time >= empty_block_posted_at + deposit_check_interval)
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
                &self.keys.registration_tx_requests_key
            } else {
                &self.keys.non_registration_tx_requests_key
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
                .hget(&self.keys.request_id_to_block_id_key, request_id)
                .await?;

            let block_id = match block_id {
                Some(id) => id,
                None => return Result::Ok(None), // No block ID found for this request
            };

            // Get memo for block_id
            let serialized_memo: Option<String> =
                conn.hget(&self.keys.memos_key, &block_id).await?;

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
        let lock_manager = self.lock_manager();
        let process_result = lock_manager
            .with_lock(lock_name, || {
                Box::pin(self.process_requests_inner(is_registration))
            })
            .await;

        match process_result {
            Ok(()) => {
                log::info!(
                    "Finished processing {} transaction requests",
                    if is_registration {
                        "registration"
                    } else {
                        "non-registration"
                    }
                );
                Ok(())
            }
            Err(StorageError::LockNotAcquired(_)) => {
                // Another instance is already processing, just return
                Ok(())
            }
            Err(e) => Err(e),
        }
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
                .hget(&self.keys.request_id_to_block_id_key, request_id)
                .await?;

            let block_id = block_id.ok_or_else(|| {
                StorageError::AddSignatureError(format!(
                    "block_id not found for request_id: {request_id}"
                ))
            })?;

            // Get memo for block_id
            let serialized_memo: Option<String> =
                conn.hget(&self.keys.memos_key, &block_id).await?;

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
            let signatures_key = self.keys.signatures_key(&block_id);
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
        let lock_manager = self.lock_manager();
        match lock_manager
            .with_lock("process_signatures", || {
                Box::pin(self.process_signatures_inner())
            })
            .await
        {
            Ok(()) => Ok(()),
            Err(StorageError::LockNotAcquired(_)) => Ok(()),
            Err(e) => {
                log::error!("Failed to process signatures: {e}");
                Ok(())
            }
        }
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
        let lock_manager = self.lock_manager();
        match lock_manager
            .with_lock("process_fee_collection", || {
                Box::pin(self.process_fee_collection_inner(store_vault_server_client))
            })
            .await
        {
            Ok(()) => Ok(()),
            Err(StorageError::LockNotAcquired(_)) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Enqueue empty block for deposit checking
    ///
    /// Adds empty block task if enough time passed since last check.
    async fn enqueue_empty_block(&self) -> Result<()> {
        if self.config.deposit_check_interval.is_none() {
            return Ok(());
        }

        let lock_manager = self.lock_manager();
        match lock_manager
            .with_lock("enqueue_empty_block", || {
                Box::pin(self.enqueue_empty_block_inner())
            })
            .await
        {
            Ok(()) => Ok(()),
            Err(StorageError::LockNotAcquired(_)) => Ok(()),
            Err(e) => Err(e),
        }
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

        // Try high-priority queue first
        if let Some(task) = self.try_dequeue_high_priority_task(&mut conn).await? {
            return Ok(Some(task));
        }

        // Try low-priority queue
        self.try_dequeue_low_priority_task(&mut conn).await
    }
}

impl RedisStorage {
    async fn try_dequeue_high_priority_task(
        &self,
        conn: &mut ConnectionManager,
    ) -> Result<Option<BlockPostTask>> {
        let task_json = conn
            .lindex::<_, Option<String>>(&self.keys.block_post_tasks_hi_key, 0)
            .await?;

        let task_json = match task_json {
            Some(json) => json,
            None => return Ok(None),
        };

        let peek_task: BlockPostTask = serde_json::from_str(&task_json)?;
        let is_registration = peek_task.block_sign_payload.is_registration_block;
        let block_nonce = peek_task.block_sign_payload.block_builder_nonce;

        let smallest_reserved_nonce = self
            .nonce_manager
            .smallest_reserved_nonce(is_registration)
            .await?;

        let should_wait = smallest_reserved_nonce != Some(block_nonce);

        if should_wait {
            log::info!(
                "High-priority head nonce {} ≠ smallest {:?}. Waiting {} then processing anyway.",
                block_nonce,
                smallest_reserved_nonce,
                self.config.nonce_waiting_time,
            );
            sleep_for(self.config.nonce_waiting_time).await;
        }

        if let Some(popped_json) = conn
            .lpop::<_, Option<String>>(&self.keys.block_post_tasks_hi_key, None)
            .await?
        {
            let task: BlockPostTask = serde_json::from_str(&popped_json)?;
            self.nonce_manager
                .release_nonce(
                    task.block_sign_payload.block_builder_nonce,
                    task.block_sign_payload.is_registration_block,
                )
                .await?;

            let message = if should_wait {
                "Dequeued high-priority task after wait"
            } else {
                "Dequeued high-priority task (nonce match)"
            };

            log::info!("{}: id={}", message, task.block_id);
            return Ok(Some(task));
        }

        Ok(None)
    }

    async fn try_dequeue_low_priority_task(
        &self,
        conn: &mut ConnectionManager,
    ) -> Result<Option<BlockPostTask>> {
        const BLPOP_TIMEOUT_SEC: f64 = 1.0;
        if let Some((_key, task_json)) = conn
            .blpop::<_, Option<(String, String)>>(
                &self.keys.block_post_tasks_lo_key,
                BLPOP_TIMEOUT_SEC,
            )
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
    use intmax2_interfaces::utils::address::IntmaxAddress;
    use intmax2_zkp::ethereum_types::{address::Address, u32limb_trait::U32LimbTrait};
    use uuid::Uuid;

    use test_redis_helper::{assert_and_stop, find_free_port, run_redis_docker, stop_redis_docker};

    async fn setup_test_storage(instance_id: &str, redis_port: &str) -> RedisStorage {
        let config = StorageConfig {
            use_fee: true,
            use_collateral: true,
            block_builder_address: Address::zero(),
            beneficiary: IntmaxAddress::default(),
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
        RedisStorage::new(&config, nonce_manager).await.unwrap()
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

        let acquired1 = redis1
            .lock_manager()
            .acquire_lock("test_lock")
            .await
            .unwrap();
        assert_and_stop(cont_name, || {
            assert!(acquired1, "Couldn't acquire lock for redis1")
        });

        let acquired2 = redis2
            .lock_manager()
            .acquire_lock("test_lock")
            .await
            .unwrap();
        assert_and_stop(cont_name, || {
            assert!(!acquired2, "Could acquire lock for redis2")
        });

        redis1
            .lock_manager()
            .release_lock("test_lock")
            .await
            .unwrap();

        let acquired2_after = redis2
            .lock_manager()
            .acquire_lock("test_lock")
            .await
            .unwrap();
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
