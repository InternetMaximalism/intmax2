use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use intmax2_client_sdk::external_api::store_vault_server::StoreVaultServerClient;
use intmax2_zkp::{common::block_builder::UserSignature, constants::NUM_SENDERS_IN_BLOCK};
use tokio::sync::RwLock;

use crate::app::{
    block_post::BlockPostTask,
    fee::{collect_fee, FeeCollection},
    types::{ProposalMemo, TxRequest},
};

use super::{config::StateConfig, error::StateError};

type AR<T> = Arc<RwLock<T>>;

pub struct InMemoryStorage {
    pub config: StateConfig,

    pub registration_tx_requests: AR<VecDeque<TxRequest>>, // registration tx requests queue
    pub registration_tx_last_processed: AR<u64>,           // last processed timestamp
    pub non_registration_tx_requests: AR<VecDeque<TxRequest>>, // non-registration tx requests queue
    pub non_registration_tx_last_processed: AR<u64>,       // last processed timestamp

    pub request_id_to_block_id: AR<HashMap<String, String>>, // request_id -> block_id
    pub memos: AR<HashMap<String, ProposalMemo>>,            // block_id -> memo
    pub signatures: AR<HashMap<String, Vec<UserSignature>>>, // block_id -> user signature

    pub fee_collection_tasks: AR<VecDeque<FeeCollection>>, // fee collection tasks queue
    pub block_post_tasks_hi: AR<VecDeque<BlockPostTask>>,  // high priority tasks queue
    pub block_post_tasks_lo: AR<VecDeque<BlockPostTask>>,  // low priority tasks queue
}

impl InMemoryStorage {
    pub fn new(config: &StateConfig) -> Self {
        Self {
            config: config.clone(),
            registration_tx_requests: Arc::new(RwLock::new(VecDeque::new())),
            registration_tx_last_processed: Arc::new(RwLock::new(0)),
            non_registration_tx_requests: Arc::new(RwLock::new(VecDeque::new())),
            non_registration_tx_last_processed: Arc::new(RwLock::new(0)),

            request_id_to_block_id: Arc::new(RwLock::new(HashMap::new())),
            memos: Arc::new(RwLock::new(HashMap::new())),
            signatures: Arc::new(RwLock::new(HashMap::new())),

            fee_collection_tasks: Arc::new(RwLock::new(VecDeque::new())),
            block_post_tasks_hi: Arc::new(RwLock::new(VecDeque::new())),
            block_post_tasks_lo: Arc::new(RwLock::new(VecDeque::new())),
        }
    }

    pub async fn add_tx(&self, is_registration: bool, tx_request: TxRequest) {
        let tx_requests = if is_registration {
            &self.registration_tx_requests
        } else {
            &self.non_registration_tx_requests
        };
        let mut tx_requests = tx_requests.write().await;
        tx_requests.push_back(tx_request);
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

        // update request_id -> block_id
        let mut request_id_to_block_id = self.request_id_to_block_id.write().await;
        for tx_request in &tx_requests {
            request_id_to_block_id.insert(tx_request.request_id.clone(), memo.block_id.clone());
        }

        // update block_id -> memo
        let mut memos = self.memos.write().await;
        memos.insert(memo.block_id.clone(), memo.clone());
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
        // get all memos
        let memos = self.memos.read().await;
        let memos = memos.values().collect::<Vec<_>>();

        // get those that have passed self.config.proposing_block_interval
        let current_time = chrono::Utc::now().timestamp() as u64;
        let target_memos = memos
            .into_iter()
            .filter(|memo| current_time > memo.created_at + self.config.proposing_block_interval)
            .collect::<Vec<_>>();

        for memo in target_memos {
            // get signatures
            let signatures = self.signatures.read().await;
            let signatures = signatures
                .get(&memo.block_id)
                .cloned()
                .unwrap_or(Vec::new());

            // if there is no signature, skip
            if signatures.is_empty() {
                continue;
            }

            // add to block_post_tasks_hi
            let block_post_task = BlockPostTask::from_memo(memo, &signatures);
            let mut block_post_tasks_hi = self.block_post_tasks_hi.write().await;
            block_post_tasks_hi.push_back(block_post_task);

            // add fee collection task
            if self.config.use_fee {
                let fee_collection = FeeCollection {
                    use_collateral: self.config.use_collateral,
                    memo: memo.clone(),
                    signatures,
                };
                let mut fee_collection_tasks = self.fee_collection_tasks.write().await;
                fee_collection_tasks.push_back(fee_collection);
            }

            // remove memo and signatures
            let mut memos = self.memos.write().await;
            memos.remove(&memo.block_id);
            let mut signatures = self.signatures.write().await;
            signatures.remove(&memo.block_id);
        }
    }

    pub async fn process_fee_collection(
        &self,
        store_vault_server_client: &StoreVaultServerClient,
    ) -> Result<(), StateError> {
        // get first fee collection task
        let fee_collection = {
            let mut fee_collection_tasks = self.fee_collection_tasks.write().await;
            fee_collection_tasks.pop_front()
        };
        let fee_collection = match fee_collection {
            Some(fee_collection) => fee_collection,
            None => return Ok(()),
        };
        let block_post_tasks = collect_fee(
            store_vault_server_client,
            self.config.fee_beneficiary,
            &fee_collection,
        )
        .await?;

        // add to block_post_tasks_lo
        let mut block_post_tasks_lo = self.block_post_tasks_lo.write().await;
        block_post_tasks_lo.extend(block_post_tasks);

        Ok(())
    }

    pub async fn pop_block_post_task(&self) -> Option<BlockPostTask> {
        // pop from block_post_tasks_hi
        let block_post_task = {
            let mut block_post_tasks_hi = self.block_post_tasks_hi.write().await;
            block_post_tasks_hi.pop_front()
        };
        match block_post_task {
            Some(block_post_task) => Some(block_post_task),
            None => {
                // if there is no high priority task, pop from block_post_tasks_lo
                let block_post_task = {
                    let mut block_post_tasks_lo = self.block_post_tasks_lo.write().await;
                    block_post_tasks_lo.pop_front()
                };
                match block_post_task {
                    Some(block_post_task) => Some(block_post_task),
                    None => return None,
                }
            }
        }
    }
}
