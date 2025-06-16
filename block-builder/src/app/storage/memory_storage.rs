use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use intmax2_client_sdk::external_api::utils::time::sleep_for;
use intmax2_interfaces::api::store_vault_server::interface::StoreVaultClientInterface;
use intmax2_zkp::{
    common::block_builder::{BlockProposal, UserSignature},
    constants::NUM_SENDERS_IN_BLOCK,
};
use itertools::Itertools;
use rand::Rng as _;
use tokio::sync::RwLock;

use crate::app::{
    block_post::BlockPostTask,
    fee::{collect_fee, FeeCollection},
    storage::nonce_manager::NonceManager,
    types::{ProposalMemo, TxRequest},
};

use super::{
    config::StorageConfig, error::StorageError,
    nonce_manager::memory_nonce_manager::InMemoryNonceManager, Storage,
};

type AR<T> = Arc<RwLock<T>>;
type ARQueue<T> = AR<VecDeque<T>>;
type ARMap<K, V> = AR<HashMap<K, V>>;

pub struct InMemoryStorage {
    pub config: StorageConfig,

    pub nonce_manager: InMemoryNonceManager,

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
    pub fn new(config: &StorageConfig, nonce_manager: InMemoryNonceManager) -> Self {
        Self {
            config: config.clone(),
            nonce_manager,
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
        let nonce = self.nonce_manager.reserve_nonce(is_registration).await?;
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
                "block_id not found for request_id: {request_id}"
            )))?;

        // get memo
        let memos = self.memos.read().await;
        let memo = memos
            .get(block_id)
            .ok_or(StorageError::AddSignatureError(format!(
                "memo not found for block_id: {block_id}"
            )))?;

        // verify signature
        signature
            .verify(&memo.block_sign_payload, memo.pubkey_hash)
            .map_err(|e| {
                StorageError::AddSignatureError(format!("signature verification failed: {e}"))
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

            // remove duplicate signatures
            let signatures = signatures
                .into_iter()
                .unique_by(|s| s.pubkey)
                .collect::<Vec<_>>();

            log::info!("num signatures: {}", signatures.len());

            // if signature are not empty, create a BlockPostTask
            if !signatures.is_empty() {
                // add to block_post_tasks_hi
                let block_post_task = BlockPostTask::from_memo(&memo, &signatures);
                let mut block_post_tasks_hi = self.block_post_tasks_hi.write().await;
                block_post_tasks_hi.push_back(block_post_task);
            }

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
        store_vault_server_client: &dyn StoreVaultClientInterface,
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
            self.config.beneficiary,
            &fee_collection,
        )
        .await?;

        // add to block_post_tasks_lo
        let mut block_post_tasks_lo = self.block_post_tasks_lo.write().await;
        block_post_tasks_lo.extend(block_post_tasks);

        Ok(())
    }

    async fn enqueue_empty_block(&self) -> Result<(), StorageError> {
        if self.config.deposit_check_interval.is_none() {
            // if deposit check is disabled, do nothing
            return Ok(());
        }
        let multiplier = rand::thread_rng().gen_range(0.5..=1.5);
        let deposit_check_interval =
            (self.config.deposit_check_interval.unwrap() as f64 * multiplier) as u64;
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

    async fn dequeue_block_post_task(&self) -> Result<Option<BlockPostTask>, StorageError> {
        // first, check if there is a high priority task
        {
            let block_post_task = self.block_post_tasks_hi.read().await.front().cloned();

            if let Some(block_post_task) = block_post_task {
                let is_registration = block_post_task.block_sign_payload.is_registration_block;
                let block_nonce = block_post_task.block_sign_payload.block_builder_nonce;
                let smallest_reserved_nonce = self
                    .nonce_manager
                    .smallest_reserved_nonce(is_registration)
                    .await?;
                if smallest_reserved_nonce == Some(block_nonce) {
                    // get front again to avoid deadlock
                    let block_post_task = self.block_post_tasks_hi.write().await.pop_front();
                    if let Some(block_post_task) = block_post_task {
                        self.nonce_manager
                            .release_nonce(
                                block_post_task.block_sign_payload.block_builder_nonce,
                                block_post_task.block_sign_payload.is_registration_block,
                            )
                            .await?;
                        return Ok(Some(block_post_task.clone()));
                    }
                } else {
                    // if the nonce is not the least reserved nonce, wait for 5 seconds and try again
                    sleep_for(self.config.nonce_waiting_time).await;
                }
            }
        }

        let block_post_task = {
            let mut block_post_tasks_hi = self.block_post_tasks_hi.write().await;
            block_post_tasks_hi.pop_front()
        };
        let result = match block_post_task {
            Some(block_post_task) => {
                // release the nonce for the block post task
                self.nonce_manager
                    .release_nonce(
                        block_post_task.block_sign_payload.block_builder_nonce,
                        block_post_task.block_sign_payload.is_registration_block,
                    )
                    .await?;
                Some(block_post_task)
            }
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
}

#[cfg(test)]
mod tests {
    use crate::app::storage::nonce_manager::config::NonceManagerConfig;

    use super::*;
    use alloy::providers::{mock::Asserter, ProviderBuilder};
    use intmax2_client_sdk::external_api::contract::{
        convert::convert_address_to_alloy, rollup_contract::RollupContract,
    };
    use intmax2_interfaces::utils::{address::IntmaxAddress, key::PublicKeyPair};
    use intmax2_zkp::ethereum_types::address::Address;

    async fn create_storage() -> InMemoryStorage {
        let config = StorageConfig {
            use_fee: false,
            use_collateral: false,
            block_builder_address: Address::default(),
            beneficiary: IntmaxAddress::default(),
            tx_timeout: 60,
            accepting_tx_interval: 10,
            proposing_block_interval: 10,
            deposit_check_interval: Some(5),
            nonce_waiting_time: 5,
            block_builder_id: "builder1".to_string(),
            redis_url: None,
            cluster_id: None,
        };

        let provider_asserter = Asserter::new();
        let provider = ProviderBuilder::default()
            .with_gas_estimation()
            .with_simple_nonce_management()
            .fetch_chain_id()
            .connect_mocked_client(provider_asserter);

        let rollup = RollupContract::new(provider, Default::default());
        let nonce_config = NonceManagerConfig {
            block_builder_address: convert_address_to_alloy(config.block_builder_address),
            redis_url: None,
            cluster_id: None,
        };
        let nonce_manager = InMemoryNonceManager::new(nonce_config, rollup);
        InMemoryStorage::new(&config, nonce_manager)
    }

    fn dummy_tx_request(request_id: &str) -> TxRequest {
        TxRequest {
            request_id: request_id.to_string(),
            sender: PublicKeyPair::default(),
            account_id: None,
            tx: Default::default(), // assuming Tx: Default
            fee_proof: None,
        }
    }

    #[tokio::test]
    async fn test_add_tx_registration() {
        let storage = create_storage().await;
        let tx = dummy_tx_request("reg-1");

        storage.add_tx(true, tx.clone()).await.unwrap();

        let queue = storage.registration_tx_requests.read().await;
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.front().unwrap().request_id, tx.request_id);
    }

    #[tokio::test]
    async fn test_add_tx_non_registration() {
        let storage = create_storage().await;
        let tx = dummy_tx_request("nonreg-1");

        storage.add_tx(false, tx.clone()).await.unwrap();

        let queue = storage.non_registration_tx_requests.read().await;
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.front().unwrap().request_id, tx.request_id);
    }
}
