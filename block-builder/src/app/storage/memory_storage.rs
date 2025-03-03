use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use intmax2_client_sdk::external_api::store_vault_server::StoreVaultServerClient;
use intmax2_zkp::{
    common::block_builder::{BlockProposal, UserSignature},
    constants::NUM_SENDERS_IN_BLOCK,
};
use tokio::sync::RwLock;

use crate::app::{
    block_post::BlockPostTask,
    fee::{collect_fee, FeeCollection},
    types::{ProposalMemo, TxRequest},
};

use super::{config::StorageConfig, error::StorageError, Storage};

type AR<T> = Arc<RwLock<T>>;
type ARQueue<T> = AR<VecDeque<T>>;
type ARMap<K, V> = AR<HashMap<K, V>>;

pub struct InMemoryStorage {
    pub config: StorageConfig,

    pub registration_tx_requests: ARQueue<TxRequest>, // registration tx requests queue
    pub registration_tx_last_processed: AR<u64>,      // last processed timestamp
    pub non_registration_tx_requests: ARQueue<TxRequest>, // non-registration tx requests queue
    pub non_registration_tx_last_processed: AR<u64>,  // last processed timestamp

    pub empty_block_posted_at: AR<Option<u64>>, // timestamp of the last empty block post

    pub request_id_to_block_id: ARMap<String, String>, // request_id -> block_id
    pub memos: ARMap<String, ProposalMemo>,            // block_id -> memo
    pub signatures: ARMap<String, Vec<UserSignature>>, // block_id -> user signature

    pub fee_collection_tasks: ARQueue<FeeCollection>, // fee collection tasks queue
    pub block_post_tasks_hi: ARQueue<BlockPostTask>,  // high priority tasks queue
    pub block_post_tasks_lo: ARQueue<BlockPostTask>,  // low priority tasks queue
}

impl InMemoryStorage {
    pub async fn new(config: &StorageConfig) -> Self {
        Self {
            config: config.clone(),
            registration_tx_requests: Default::default(),
            registration_tx_last_processed: Default::default(),
            non_registration_tx_requests: Default::default(),
            non_registration_tx_last_processed: Default::default(),

            empty_block_posted_at: Default::default(),

            request_id_to_block_id: Default::default(),
            memos: Default::default(),
            signatures: Default::default(),

            fee_collection_tasks: Default::default(),
            block_post_tasks_hi: Default::default(),
            block_post_tasks_lo: Default::default(),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Storage for InMemoryStorage {
    async fn add_tx(
        &self,
        is_registration: bool,
        tx_request: TxRequest,
    ) -> Result<(), StorageError> {
        let tx_requests = if is_registration {
            &self.registration_tx_requests
        } else {
            &self.non_registration_tx_requests
        };
        let mut tx_requests = tx_requests.write().await;
        tx_requests.push_back(tx_request);

        Ok(())
    }

    async fn process_requests(&self, is_registration: bool) -> Result<(), StorageError> {
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
        let last_processed_ = *last_processed.read().await;
        let mut tx_requests = tx_requests.write().await;
        let current_time = chrono::Utc::now().timestamp() as u64;
        if (tx_requests.len() < NUM_SENDERS_IN_BLOCK
            && current_time < last_processed_ + self.config.accepting_tx_interval)
            || tx_requests.is_empty()
        {
            return Ok(());
        }

        log::info!("process_requests is_registration: {}", is_registration);

        let num_tx_requests = tx_requests.len().min(NUM_SENDERS_IN_BLOCK);
        let tx_requests: Vec<TxRequest> = tx_requests.drain(..num_tx_requests).collect();
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

        // update last_processed
        *last_processed.write().await = current_time;

        Ok(())
    }

    async fn query_proposal(
        &self,
        request_id: &str,
    ) -> Result<Option<BlockProposal>, StorageError> {
        let block_ids = self.request_id_to_block_id.read().await;
        let block_id = block_ids.get(request_id);
        if block_id.is_none() {
            return Ok(None);
        }
        let block_id = block_id.unwrap();
        let memos = self.memos.read().await;
        let memo = memos.get(block_id).cloned();
        let proposal = if let Some(memo) = memo {
            // find the position of the request_id in the memo
            let position = memo
                .tx_requests
                .iter()
                .position(|r| r.request_id == request_id)
                .ok_or(StorageError::QueryProposalError(format!(
                    "request_id {} not found in memo: {}",
                    request_id, memo.block_id
                )))?;
            Some(memo.proposals[position].clone())
        } else {
            None
        };

        Ok(proposal)
    }

    async fn add_signature(
        &self,
        request_id: &str,
        signature: UserSignature,
    ) -> Result<(), StorageError> {
        // get block_id
        let block_ids = self.request_id_to_block_id.read().await;
        let block_id = block_ids
            .get(request_id)
            .ok_or(StorageError::AddSignatureError(format!(
                "block_id not found for request_id: {}",
                request_id
            )))?;

        // get memo
        let memos = self.memos.read().await;
        let memo = memos
            .get(block_id)
            .ok_or(StorageError::AddSignatureError(format!(
                "memo not found for block_id: {}",
                block_id
            )))?;

        // verify signature
        signature
            .verify(memo.tx_tree_root, memo.expiry, memo.pubkey_hash)
            .map_err(|e| {
                StorageError::AddSignatureError(format!("signature verification failed: {}", e))
            })?;

        // add signature
        let mut signatures = self.signatures.write().await;
        let signatures = signatures.entry(block_id.clone()).or_insert_with(Vec::new);
        signatures.push(signature);

        Ok(())
    }

    async fn process_signatures(&self) -> Result<(), StorageError> {
        // get all memos
        let target_memos = {
            let memos = self.memos.read().await;
            let memos = memos.values().cloned().collect::<Vec<_>>();
            // get those that have passed self.config.proposing_block_interval
            let current_time = chrono::Utc::now().timestamp() as u64;
            memos
                .into_iter()
                .filter(|memo| {
                    current_time > memo.created_at + self.config.proposing_block_interval
                })
                .collect::<Vec<_>>()
        };

        for memo in target_memos {
            log::info!("process_signatures block_id: {}", memo.block_id);
            // get signatures
            let signatures = {
                let signatures_guard = self.signatures.read().await;
                signatures_guard
                    .get(&memo.block_id)
                    .cloned()
                    .unwrap_or(Vec::new())
            };

            log::info!("num signatures: {}", signatures.len());

            // if there is no signature, skip
            if signatures.is_empty() {
                continue;
            }

            // add to block_post_tasks_hi
            let block_post_task = BlockPostTask::from_memo(&memo, &signatures);
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
            {
                let mut memos = self.memos.write().await;
                memos.remove(&memo.block_id);
            }
            {
                let mut signatures = self.signatures.write().await;
                signatures.remove(&memo.block_id);
            }
        }

        Ok(())
    }

    async fn process_fee_collection(
        &self,
        store_vault_server_client: &StoreVaultServerClient,
    ) -> Result<(), StorageError> {
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

    async fn dequeue_block_post_task(&self) -> Result<Option<BlockPostTask>, StorageError> {
        let block_post_task = {
            let mut block_post_tasks_hi = self.block_post_tasks_hi.write().await;
            block_post_tasks_hi.pop_front()
        };
        let result = match block_post_task {
            Some(block_post_task) => Some(block_post_task),
            None => {
                // if there is no high priority task, pop from block_post_tasks_lo
                {
                    let mut block_post_tasks_lo = self.block_post_tasks_lo.write().await;
                    block_post_tasks_lo.pop_front()
                }
            }
        };
        Ok(result)
    }

    async fn enqueue_empty_block(&self) -> Result<(), StorageError> {
        if self.config.deposit_check_interval.is_none() {
            // if deposit check is disabled, do nothing
            return Ok(());
        }
        let deposit_check_interval = self.config.deposit_check_interval.unwrap();
        let empty_block_posted_at = *self.empty_block_posted_at.read().await;
        let current_time = chrono::Utc::now().timestamp() as u64;
        if let Some(empty_block_posted_at) = empty_block_posted_at {
            if current_time < empty_block_posted_at + deposit_check_interval {
                // if less than deposit_check_interval seconds have passed since the last empty block post, do nothing
                return Ok(());
            }
        }
        // post an empty block
        *self.empty_block_posted_at.write().await = Some(current_time);
        self.block_post_tasks_lo
            .write()
            .await
            .push_back(BlockPostTask::default());
        Ok(())
    }
}
