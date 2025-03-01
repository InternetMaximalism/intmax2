use std::{collections::HashMap, sync::Arc};

use intmax2_zkp::{common::block_builder::UserSignature, constants::NUM_SENDERS_IN_BLOCK};
use tokio::sync::RwLock;

use crate::app::types::{ProposalMemo, TxRequest};

use super::{config::StateConfig, error::StateError, models::BlockPostTask};

type AR<T> = Arc<RwLock<T>>;

pub struct InMemoryStorage {
    pub config: StateConfig,

    pub registration_tx_requests: AR<Vec<TxRequest>>, // registration tx requests
    pub registration_tx_last_processed: AR<u64>,      // last processed timestamp
    pub non_registration_tx_requests: AR<Vec<TxRequest>>, // non-registration tx requests
    pub non_registration_tx_last_processed: AR<u64>,  // last processed timestamp

    pub request_id_to_block_id: AR<HashMap<String, String>>, // request_id -> block_id
    pub memos: AR<HashMap<String, ProposalMemo>>,            // block_id -> memo
    pub signatures: AR<HashMap<String, Vec<UserSignature>>>, // block_id -> user signature
    pub block_post_tasks_hi: AR<Vec<BlockPostTask>>,         // high priority tasks
    pub block_post_tasks_lo: AR<Vec<BlockPostTask>>,         // low priority tasks
}

impl InMemoryStorage {
    pub fn new(config: &StateConfig) -> Self {
        Self {
            config: config.clone(),
            registration_tx_requests: Arc::new(RwLock::new(Vec::new())),
            registration_tx_last_processed: Arc::new(RwLock::new(0)),
            non_registration_tx_requests: Arc::new(RwLock::new(Vec::new())),
            non_registration_tx_last_processed: Arc::new(RwLock::new(0)),

            request_id_to_block_id: Arc::new(RwLock::new(HashMap::new())),
            memos: Arc::new(RwLock::new(HashMap::new())),
            signatures: Arc::new(RwLock::new(HashMap::new())),
            block_post_tasks_hi: Arc::new(RwLock::new(Vec::new())),
            block_post_tasks_lo: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn enqueue_tx_request(&self, is_registration: bool, tx_request: TxRequest) {
        let tx_requests = if is_registration {
            &self.registration_tx_requests
        } else {
            &self.non_registration_tx_requests
        };
        let mut tx_requests = tx_requests.write().await;
        tx_requests.push(tx_request);
    }

    pub async fn process_requests(&self, is_registration: bool) {
        let tx_requests = if is_registration {
            &self.registration_tx_requests
        } else {
            &self.non_registration_tx_requests
        };
        let last_processed = if is_registration {
            &self.registration_tx_last_processed
        } else {
            &self.non_registration_tx_last_processed
        };

        // If more than self.config.accepting_tx_interval seconds have passed since last_processed,
        // or if there are NUM_SENDERS_IN_BLOCK tx_requests, process them.
        let last_processed = *last_processed.read().await;
        let mut tx_requests = tx_requests.write().await;
        let current_time = chrono::Utc::now().timestamp() as u64;
        if (tx_requests.len() < NUM_SENDERS_IN_BLOCK
            && current_time < last_processed + self.config.accepting_tx_interval)
            || tx_requests.is_empty()
        {
            return;
        }

        let tx_requests: Vec<TxRequest> = tx_requests.drain(..NUM_SENDERS_IN_BLOCK).collect();
        let memo =
            ProposalMemo::from_tx_requests(is_registration, &tx_requests, self.config.tx_timeout);
        let block_id = uuid::Uuid::new_v4().to_string();

        // update request_id -> block_id
        let mut request_id_to_block_id = self.request_id_to_block_id.write().await;
        for tx_request in &tx_requests {
            request_id_to_block_id.insert(tx_request.request_id.clone(), block_id.clone());
        }

        // update block_id -> memo
        let mut memos = self.memos.write().await;
        memos.insert(block_id.clone(), memo.clone());
    }

    pub async fn add_signature(
        &self,
        request_id: &str,
        signature: UserSignature,
    ) -> Result<(), StateError> {
        // get block_id
        let block_ids = self.request_id_to_block_id.read().await;
        let block_id = block_ids
            .get(request_id)
            .ok_or(StateError::AddSignatureError(format!(
                "block_id not found for request_id: {}",
                request_id
            )))?;

        // get memo
        let memos = self.memos.read().await;
        let memo = memos
            .get(block_id)
            .ok_or(StateError::AddSignatureError(format!(
                "memo not found for block_id: {}",
                block_id
            )))?;

        // verify signature
        signature
            .verify(memo.tx_tree_root, memo.expiry, memo.pubkey_hash)
            .map_err(|e| {
                StateError::AddSignatureError(format!("signature verification failed: {}", e))
            })?;

        // add signature
        let mut signatures = self.signatures.write().await;
        let signatures = signatures.entry(block_id.clone()).or_insert_with(Vec::new);
        signatures.push(signature);

        Ok(())
    }

    pub async fn process_signatures(&self) {
        // get memo
    }
}
