use ethers::types::H256;
use intmax2_client_sdk::external_api::{
    contract::{
        block_builder_registry::BlockBuilderRegistryContract, rollup_contract::RollupContract,
    },
    store_vault_server::StoreVaultServerClient,
    validity_prover::ValidityProverClient,
};
use intmax2_interfaces::api::{
    block_builder::interface::{BlockBuilderStatus, FeeInfo, FeeProof},
    validity_prover::interface::ValidityProverClientInterface,
};
use intmax2_zkp::{
    common::{
        block_builder::{BlockProposal, UserSignature},
        tx::Tx,
    },
    constants::NUM_SENDERS_IN_BLOCK,
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{
    sync::{mpsc, Mutex, RwLock},
    time::sleep,
};

use crate::{
    app::{
        block_post::BlockPostTask,
        fee::{collect_fee, validate_fee_proof, FeeCollection},
    },
    EnvVar,
};

use super::{
    block_post::post_block,
    builder_state::BuilderState,
    error::BlockBuilderError,
    fee::{convert_fee_vec, parse_fee_str},
};

pub const DEFAULT_POST_BLOCK_CHANNEL: u64 = 100;

#[derive(Debug, Clone)]
struct Config {
    block_builder_url: String,
    block_builder_private_key: H256,
    eth_allowance_for_block: U256,
    deposit_check_interval: Option<u64>,
    accepting_tx_interval: u64,
    proposing_block_interval: u64,
    initial_heart_beat_delay: u64,
    heart_beat_interval: u64,

    // fees
    beneficiary_pubkey: Option<U256>,
    registration_fee: Option<HashMap<u32, U256>>,
    non_registration_fee: Option<HashMap<u32, U256>>,
    registration_collateral_fee: Option<HashMap<u32, U256>>,
    non_registration_collateral_fee: Option<HashMap<u32, U256>>,
}

#[derive(Debug, Clone)]
pub struct BlockBuilder {
    config: Config,
    store_vault_server_client: StoreVaultServerClient,
    validity_prover_client: ValidityProverClient,
    rollup_contract: RollupContract,
    registry_contract: BlockBuilderRegistryContract,
    tx_high: mpsc::Sender<BlockPostTask>,
    rx_high: Arc<Mutex<mpsc::Receiver<BlockPostTask>>>,
    tx_low: mpsc::Sender<BlockPostTask>,
    rx_low: Arc<Mutex<mpsc::Receiver<BlockPostTask>>>,

    force_post: Arc<RwLock<bool>>,
    next_deposit_index: Arc<RwLock<u32>>,
    registration_state: Arc<RwLock<BuilderState>>,
    non_registration_state: Arc<RwLock<BuilderState>>,
}

impl BlockBuilder {
    pub fn new(env: &EnvVar) -> Result<Self, BlockBuilderError> {
        let store_vault_server_client =
            StoreVaultServerClient::new(&env.store_vault_server_base_url);
        let validity_prover_client = ValidityProverClient::new(&env.validity_prover_base_url);
        let rollup_contract = RollupContract::new(
            &env.l2_rpc_url,
            env.l2_chain_id,
            env.rollup_contract_address,
            env.rollup_contract_deployed_block_number,
        );
        let registry_contract = BlockBuilderRegistryContract::new(
            &env.l2_rpc_url,
            env.l2_chain_id,
            env.block_builder_registry_contract_address,
        );
        let eth_allowance_for_block = {
            let u = ethers::utils::parse_ether(env.eth_allowance_for_block.clone()).unwrap();
            let mut buf = [0u8; 32];
            u.to_big_endian(&mut buf);
            U256::from_bytes_be(&buf)
        };

        let buf = env
            .num_block_post_channel
            .unwrap_or(DEFAULT_POST_BLOCK_CHANNEL) as usize;
        let (tx_high, rx_high) = mpsc::channel(buf);
        let (tx_low, rx_low) = mpsc::channel(buf);

        let registration_fee = env
            .registration_fee
            .as_ref()
            .map(|fee| parse_fee_str(fee))
            .transpose()?;
        let non_registration_fee = env
            .non_registration_fee
            .as_ref()
            .map(|fee| parse_fee_str(fee))
            .transpose()?;
        let registration_collateral_fee = env
            .registration_collateral_fee
            .as_ref()
            .map(|fee| parse_fee_str(fee))
            .transpose()?;
        let non_registration_collateral_fee = env
            .non_registration_collateral_fee
            .as_ref()
            .map(|fee| parse_fee_str(fee))
            .transpose()?;

        let beneficiary_pubkey = env
            .beneficiary_pubkey
            .map(|pubkey| U256::from_bytes_be(pubkey.as_bytes()));

        let config = Config {
            block_builder_url: env.block_builder_url.clone(),
            block_builder_private_key: env.block_builder_private_key,
            eth_allowance_for_block,
            deposit_check_interval: env.deposit_check_interval,
            accepting_tx_interval: env.accepting_tx_interval,
            proposing_block_interval: env.proposing_block_interval,
            initial_heart_beat_delay: env.initial_heart_beat_delay,
            heart_beat_interval: env.heart_beat_interval,
            beneficiary_pubkey,
            registration_fee,
            non_registration_fee,
            registration_collateral_fee,
            non_registration_collateral_fee,
        };
        Ok(Self {
            config,
            store_vault_server_client,
            validity_prover_client,
            rollup_contract,
            registry_contract,
            tx_high,
            rx_high: Arc::new(Mutex::new(rx_high)),
            tx_low,
            rx_low: Arc::new(Mutex::new(rx_low)),
            force_post: Arc::new(RwLock::new(false)),
            next_deposit_index: Arc::new(RwLock::new(0)),
            registration_state: Arc::new(RwLock::new(BuilderState::default())),
            non_registration_state: Arc::new(RwLock::new(BuilderState::default())),
        })
    }

    pub fn get_fee_info(&self) -> FeeInfo {
        FeeInfo {
            beneficiary: self.config.beneficiary_pubkey,
            registration_fee: convert_fee_vec(&self.config.registration_fee),
            non_registration_fee: convert_fee_vec(&self.config.non_registration_fee),
            registration_collateral_fee: convert_fee_vec(&self.config.registration_collateral_fee),
            non_registration_collateral_fee: convert_fee_vec(
                &self.config.non_registration_collateral_fee,
            ),
        }
    }

    // utility functions
    async fn state_read(
        &self,
        is_registration_block: bool,
    ) -> tokio::sync::RwLockReadGuard<'_, BuilderState> {
        if is_registration_block {
            self.registration_state.read().await
        } else {
            self.non_registration_state.read().await
        }
    }

    async fn state_write(
        &self,
        is_registration_block: bool,
    ) -> tokio::sync::RwLockWriteGuard<'_, BuilderState> {
        if is_registration_block {
            self.registration_state.write().await
        } else {
            self.non_registration_state.write().await
        }
    }

    pub async fn get_status(&self, is_registration_block: bool) -> BlockBuilderStatus {
        if is_registration_block {
            self.registration_state.read().await.get_status()
        } else {
            self.non_registration_state.read().await.get_status()
        }
    }

    async fn num_tx_requests(
        &self,
        is_registration_block: bool,
    ) -> Result<usize, BlockBuilderError> {
        log::info!(
            "num_tx_requests is_registration_block: {}",
            is_registration_block
        );
        let state = self.state_read(is_registration_block).await;
        Ok(state.count_tx_requests())
    }

    // Send a tx request by the user.
    pub async fn send_tx_request(
        &self,
        is_registration_block: bool,
        pubkey: U256,
        tx: Tx,
        fee_proof: &Option<FeeProof>,
    ) -> Result<(), BlockBuilderError> {
        log::info!(
            "send_tx_request is_registration_block: {}",
            is_registration_block
        );

        {
            // check if the block builder is accepting txs
            let state = self.state_read(is_registration_block).await;
            if !state.is_accepting_txs() {
                return Err(BlockBuilderError::NotAcceptingTx);
            }
            if state.count_tx_requests() >= NUM_SENDERS_IN_BLOCK {
                return Err(BlockBuilderError::BlockIsFull);
            }
            if state.is_pubkey_contained(pubkey) {
                return Err(BlockBuilderError::OnlyOneSenderAllowed);
            }
            // drop the lock
        }

        // registration check
        let account_info = self.validity_prover_client.get_account_info(pubkey).await?;
        let account_id = account_info.account_id;
        if is_registration_block {
            if let Some(account_id) = account_id {
                return Err(BlockBuilderError::AccountAlreadyRegistered(
                    pubkey, account_id,
                ));
            }
        } else if account_id.is_none() {
            return Err(BlockBuilderError::AccountNotFound(pubkey));
        }

        // fee check
        let required_fee = if is_registration_block {
            self.config.registration_fee.as_ref()
        } else {
            self.config.non_registration_fee.as_ref()
        };
        let required_collateral_fee = if is_registration_block {
            self.config.registration_collateral_fee.as_ref()
        } else {
            self.config.non_registration_collateral_fee.as_ref()
        };
        validate_fee_proof(
            &self.store_vault_server_client,
            self.config.beneficiary_pubkey,
            required_fee,
            required_collateral_fee,
            pubkey,
            fee_proof,
        )
        .await?;

        let mut state = self.state_write(is_registration_block).await;
        // check again after the async call
        if !state.is_accepting_txs() {
            return Err(BlockBuilderError::NotAcceptingTx);
        }
        if state.count_tx_requests() >= NUM_SENDERS_IN_BLOCK {
            return Err(BlockBuilderError::BlockIsFull);
        }
        if state.is_pubkey_contained(pubkey) {
            return Err(BlockBuilderError::OnlyOneSenderAllowed);
        }
        // update state
        state.append_tx_request(pubkey, account_id, tx, fee_proof.clone());

        Ok(())
    }

    // Construct a block with the given tx requests by the block builder.
    pub async fn construct_block(
        &self,
        is_registration_block: bool,
    ) -> Result<(), BlockBuilderError> {
        log::info!(
            "construct_block is_registration_block: {}",
            is_registration_block
        );
        let mut state = self.state_write(is_registration_block).await;
        if !state.is_accepting_txs() {
            return Err(BlockBuilderError::NotAcceptingTx);
        }
        state.propose_block(is_registration_block);
        Ok(())
    }

    // Query the constructed proposal by the user.
    pub async fn query_proposal(
        &self,
        is_registration_block: bool,
        pubkey: U256,
        tx: Tx,
    ) -> Result<Option<BlockProposal>, BlockBuilderError> {
        log::info!(
            "query_proposal is_registration_block: {}",
            is_registration_block
        );
        let state = self.state_read(is_registration_block).await;
        if state.is_pausing() {
            return Err(BlockBuilderError::BlockBuilderIsPausing);
        }
        if state.is_accepting_txs() && !state.is_request_contained(pubkey, tx) {
            return Err(BlockBuilderError::TxRequestNotFound);
        }
        Ok(state.query_proposal(pubkey, tx))
    }

    // Post the signature by the user.
    pub async fn post_signature(
        &self,
        is_registration_block: bool,
        tx: Tx,
        signature: UserSignature,
    ) -> Result<(), BlockBuilderError> {
        log::info!(
            "post_signature is_registration_block: {}",
            is_registration_block
        );
        let mut state = self.state_write(is_registration_block).await;
        if !state.is_proposing_block() {
            return Err(BlockBuilderError::NotProposing);
        }
        if state.is_request_contained(signature.pubkey, tx) {
            return Err(BlockBuilderError::TxRequestNotFound);
        }
        let memo = state.get_proposal_memo().unwrap();
        signature
            .verify(memo.tx_tree_root, memo.expiry, memo.pubkey_hash)
            .map_err(|e| BlockBuilderError::InvalidSignature(e.to_string()))?;
        // update state
        state.append_signature(signature);
        Ok(())
    }

    // Post the block with the given signatures.
    pub async fn post_block(
        &self,
        is_registration_block: bool,
        force_post: bool,
    ) -> Result<(), BlockBuilderError> {
        log::info!(
            "post_block is_registration_block: {}",
            is_registration_block
        );
        let state = self.state_read(is_registration_block).await;
        if !state.is_proposing_block() {
            return Err(BlockBuilderError::NotProposing);
        }
        let memo = state.get_proposal_memo().unwrap();
        let signatures = state.get_signatures().unwrap();
        drop(state); // release the lock

        // queue the block post
        let block_post = BlockPostTask {
            force_post,
            is_registration_block,
            tx_tree_root: memo.tx_tree_root,
            expiry: memo.expiry,
            pubkeys: memo.pubkeys.clone(),
            account_ids: memo.get_account_ids(),
            pubkey_hash: memo.pubkey_hash,
            signatures: signatures.clone(),
        };

        self.tx_high.send(block_post).await.map_err(|e| {
            BlockBuilderError::QueueError(format!("Error in sending block post: {}", e))
        })?;

        // queue fee transfer
        let use_fee = if is_registration_block {
            self.config.registration_fee.is_some()
        } else {
            self.config.non_registration_fee.is_some()
        };
        if use_fee {
            let beneficiary_pubkey =
                self.config
                    .beneficiary_pubkey
                    .ok_or(BlockBuilderError::UnexpectedError(
                        "Beneficiary pubkey is not set".to_string(),
                    ))?;
            let use_collateral = if is_registration_block {
                self.config.registration_collateral_fee.is_some()
            } else {
                self.config.non_registration_collateral_fee.is_some()
            };
            let fee_collection = FeeCollection {
                use_collateral,
                memo,
                signatures,
            };
            collect_fee(
                &self.tx_low,
                &self.store_vault_server_client,
                beneficiary_pubkey,
                &fee_collection,
            )
            .await?;
        }

        // update state
        self.state_write(is_registration_block)
            .await
            .finalize_block();
        Ok(())
    }

    // cycle functions
    async fn start_accepting_txs(
        &self,
        is_registration_block: bool,
    ) -> Result<(), BlockBuilderError> {
        log::info!(
            "start_accepting_txs is_registration_block: {}",
            is_registration_block
        );
        let mut state = self.state_write(is_registration_block).await;
        if !state.is_pausing() {
            return Err(BlockBuilderError::ShouldBePausing);
        }
        state.start_accepting_txs();
        Ok(())
    }

    async fn check_new_deposits(&self) -> Result<bool, BlockBuilderError> {
        log::info!("check_new_deposits");
        let next_deposit_index = self.validity_prover_client.get_next_deposit_index().await?;
        let current_next_deposit_index = *self.next_deposit_index.read().await; // release the lock immediately

        // sanity check
        if next_deposit_index < current_next_deposit_index {
            return Err(BlockBuilderError::UnexpectedError(format!(
                "next_deposit_index is smaller than the current one: {} < {}",
                next_deposit_index, current_next_deposit_index
            )));
        }
        if next_deposit_index == current_next_deposit_index {
            return Ok(false);
        }

        // update the next deposit index
        *self.next_deposit_index.write().await = next_deposit_index;

        log::info!("new deposit found: {}", next_deposit_index);
        Ok(true)
    }

    /// Reset the block builder.
    async fn reset(&self, is_registration_block: bool) {
        log::info!("reset");
        let mut state = self.state_write(is_registration_block).await;
        *state = BuilderState::default();
    }

    // Cycle of the block builder.
    async fn cycle(&self, is_registration_block: bool) -> Result<(), BlockBuilderError> {
        log::info!("cycle is_registration_block: {}", is_registration_block);
        self.start_accepting_txs(is_registration_block).await?;

        tokio::time::sleep(Duration::from_secs(self.config.accepting_tx_interval)).await;

        let num_tx_requests = self.num_tx_requests(is_registration_block).await?;
        let force_post = *self.force_post.read().await;
        if num_tx_requests == 0 && (is_registration_block || !force_post) {
            log::info!("No tx requests, not constructing block");
            self.reset(is_registration_block).await;
            return Ok(());
        }

        self.construct_block(is_registration_block).await?;

        tokio::time::sleep(Duration::from_secs(self.config.proposing_block_interval)).await;

        let force_post = *self.force_post.read().await;
        self.post_block(is_registration_block, force_post).await?;

        let force_post = *self.force_post.read().await;
        if force_post {
            *self.force_post.write().await = false;
        }

        Ok(())
    }

    // job
    async fn emit_heart_beat(&self) -> Result<(), BlockBuilderError> {
        self.registry_contract
            .emit_heart_beat(
                self.config.block_builder_private_key,
                &self.config.block_builder_url,
            )
            .await?;
        Ok(())
    }

    fn emit_heart_beat_job(self) {
        let start_time = chrono::Utc::now().timestamp() as u64;
        actix_web::rt::spawn(async move {
            let now = chrono::Utc::now().timestamp() as u64;
            let initial_heartbeat_time = start_time + self.config.initial_heart_beat_delay;
            let delay_secs = if initial_heartbeat_time > now {
                initial_heartbeat_time - now
            } else {
                0
            };

            // wait for the initial heart beat
            tokio::time::sleep(Duration::from_secs(delay_secs)).await;

            // emit initial heart beat
            match self.emit_heart_beat().await {
                Ok(_) => log::info!("Initial heart beat emitted"),
                Err(e) => log::error!("Error in emitting initial heart beat: {}", e),
            }

            // emit heart beat periodically
            loop {
                tokio::time::sleep(Duration::from_secs(self.config.heart_beat_interval)).await;
                match self.emit_heart_beat().await {
                    Ok(_) => log::info!("Heart beat emitted"),
                    Err(e) => log::error!("Error in emitting heart beat: {}", e),
                }
            }
        });
    }

    async fn post_block_inner(&self) -> Result<(), BlockBuilderError> {
        let mut rx_high = self.rx_high.lock().await;
        let mut rx_low = self.rx_low.lock().await;
        let block_post_task = tokio::select! {
            Some(t) =  rx_high.recv() => {
                t
            }
            Some(t) = rx_low.recv()  => {
                t
            }
            else => {
                return Err(BlockBuilderError::QueueError("No block post task".to_string()));
            }
        };

        match post_block(
            self.config.block_builder_private_key,
            self.config.eth_allowance_for_block,
            &self.rollup_contract,
            &self.validity_prover_client,
            block_post_task,
        )
        .await
        {
            Ok(_) => {}
            Err(e) => {
                log::error!("Error in posting block: {}", e);
            }
        }
        Ok(())
    }

    fn post_block_job(self) {
        actix_web::rt::spawn(async move {
            loop {
                match self.post_block_inner().await {
                    Ok(_) => {}
                    Err(e) => {
                        log::error!("Error in post block job: {}", e);
                    }
                }
                sleep(Duration::from_secs(10)).await;
            }
        });
    }

    fn post_empty_block_job(self, deposit_check_interval: u64) {
        actix_web::rt::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(deposit_check_interval)).await;
                match self.check_new_deposits().await {
                    Ok(new_deposits_exist) => {
                        if new_deposits_exist {
                            *self.force_post.write().await = true;
                        }
                    }
                    Err(e) => {
                        log::error!("Error in checking new deposits: {}", e);
                    }
                }
            }
        });
    }

    fn cycle_job(self, is_registration_block: bool) {
        actix_web::rt::spawn(async move {
            loop {
                match self.cycle(is_registration_block).await {
                    Ok(_) => {
                        log::info!(
                            "Cycle successful for registration block: {}",
                            is_registration_block
                        );
                    }
                    Err(e) => {
                        log::error!("Error in block builder: {}", e);
                        self.reset(is_registration_block).await;
                        *self.force_post.write().await = false;
                        sleep(Duration::from_secs(10)).await;
                    }
                }
            }
        });
    }

    pub fn run(&self) {
        if let Some(deposit_check_interval) = self.config.deposit_check_interval {
            self.clone().post_empty_block_job(deposit_check_interval);
        }
        self.clone().post_block_job();
        self.clone().cycle_job(true);
        self.clone().cycle_job(false);
        self.clone().emit_heart_beat_job();
    }
}
