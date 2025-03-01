use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

use crate::app::types::TxRequest;

use super::models::{BlockPostTask, ProposingBlockState};

type AR<T> = Arc<RwLock<T>>;

pub struct InMemoryStorage {
    pub registration_tx_requests: AR<Vec<TxRequest>>, // registration tx requests
    pub non_registration_tx_requests: AR<Vec<TxRequest>>, // non-registration tx requests
    pub request_id_to_block_id: AR<HashMap<String, String>>, // request_id -> block_id
    pub proposing_states: AR<HashMap<String, ProposingBlockState>>, // block_id -> ProposingBlockState
    pub block_post_tasks_hi: AR<Vec<BlockPostTask>>,                // high priority tasks
    pub block_post_tasks_lo: AR<Vec<BlockPostTask>>,                // low priority tasks
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self {
            registration_tx_requests: Arc::new(RwLock::new(Vec::new())),
            non_registration_tx_requests: Arc::new(RwLock::new(Vec::new())),
            request_id_to_block_id: Arc::new(RwLock::new(HashMap::new())),
            proposing_states: Arc::new(RwLock::new(HashMap::new())),
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

    pub async fn pop_tx_requests_chunk(
        &self,
        is_registration: bool,
        chunk_size: usize,
    ) -> Vec<TxRequest> {
        let tx_requests = if is_registration {
            &self.registration_tx_requests
        } else {
            &self.non_registration_tx_requests
        };
        let mut tx_requests = tx_requests.write().await;
        let chunk = tx_requests.drain(..chunk_size).collect();
        chunk
    }
}
