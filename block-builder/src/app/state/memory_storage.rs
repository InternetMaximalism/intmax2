use std::{collections::HashMap, sync::Arc};

use intmax2_zkp::{common::block_builder::UserSignature, constants::NUM_SENDERS_IN_BLOCK};
use tokio::sync::RwLock;

use crate::app::types::{ProposalMemo, TxRequest};

use super::{
    error::StateError,
    models::{BlockPostTask, ProposingBlockState},
};

type AR<T> = Arc<RwLock<T>>;

pub struct InMemoryStorage {
    pub registration_tx_requests: AR<Vec<TxRequest>>, // registration tx requests
    pub non_registration_tx_requests: AR<Vec<TxRequest>>, // non-registration tx requests
    pub request_id_to_block_id: AR<HashMap<String, String>>, // request_id -> block_id
    pub memos: AR<HashMap<String, ProposalMemo>>,     // block_id -> memo
    pub signatures: AR<HashMap<String, Vec<UserSignature>>>, // block_id -> user signature
    pub block_post_tasks_hi: AR<Vec<BlockPostTask>>,  // high priority tasks
    pub block_post_tasks_lo: AR<Vec<BlockPostTask>>,  // low priority tasks
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self {
            registration_tx_requests: Arc::new(RwLock::new(Vec::new())),
            non_registration_tx_requests: Arc::new(RwLock::new(Vec::new())),
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

    pub async fn process_requests(&self, is_registration: bool, expiry: u64) {
        let tx_requests = if is_registration {
            &self.registration_tx_requests
        } else {
            &self.non_registration_tx_requests
        };
        let mut tx_requests = tx_requests.write().await;
        let chunk: Vec<TxRequest> = tx_requests.drain(..NUM_SENDERS_IN_BLOCK).collect();
        if chunk.is_empty() {
            return;
        }

        let memo = ProposalMemo::from_tx_requests(is_registration, &tx_requests, expiry);
        let block_id = uuid::Uuid::new_v4().to_string();

        // update request_id -> block_id
        let mut request_id_to_block_id = self.request_id_to_block_id.write().await;
        for tx_request in &chunk {
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

    pub async fn process_signatures(&self) {}
}
