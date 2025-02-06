use std::{sync::Arc, time::Duration};

use ark_bn254::{Bn254, Fr, G1Affine, G2Affine};
use ark_ec::{pairing::Pairing as _, AffineRepr as _};
use ethers::types::H256;
use intmax2_client_sdk::external_api::{
    contract::{
        block_builder_registry::BlockBuilderRegistryContract, rollup_contract::RollupContract,
    },
    validity_prover::ValidityProverClient,
};
use intmax2_interfaces::api::{
    block_builder::interface::BlockBuilderStatus,
    validity_prover::interface::ValidityProverClientInterface,
};
use intmax2_zkp::{
    common::{
        block_builder::{BlockProposal, UserSignature},
        signature::{
            flatten::FlatG2,
            sign::{hash_to_weight, tx_tree_root_and_expiry_to_message_point},
            SignatureContent,
        },
        tx::Tx,
    },
    constants::NUM_SENDERS_IN_BLOCK,
    ethereum_types::{
        account_id_packed::AccountIdPacked, bytes16::Bytes16, bytes32::Bytes32, u256::U256,
        u32limb_trait::U32LimbTrait,
    },
};
use num::BigUint;
use plonky2_bn254::fields::recover::RecoverFromX as _;
use tokio::{sync::RwLock, time::sleep};

use crate::EnvVar;

use super::{builder_state::BuilderState, error::BlockBuilderError};

#[derive(Debug, Clone)]
struct Config {
    block_builder_url: String,
    block_builder_private_key: H256,
    eth_allowance_for_block: ethers::types::U256,
    deposit_check_interval: Option<u64>,
    accepting_tx_interval: u64,
    proposing_block_interval: u64,
    initial_heart_beat_delay: u64,
    heart_beat_interval: u64,
}

#[derive(Debug, Clone)]
pub struct BlockBuilder {
    config: Config,
    validity_prover_client: ValidityProverClient,
    rollup_contract: RollupContract,
    registry_contract: BlockBuilderRegistryContract,

    force_post: Arc<RwLock<bool>>,
    next_deposit_index: Arc<RwLock<u32>>,
    registration_state: Arc<RwLock<BuilderState>>,
    non_registration_state: Arc<RwLock<BuilderState>>,
}

impl BlockBuilder {
    pub fn new(env: &EnvVar) -> Self {
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
        let eth_allowance_for_block =
            ethers::utils::parse_ether(env.eth_allowance_for_block.clone()).unwrap();
        let config = Config {
            block_builder_url: env.block_builder_url.clone(),
            block_builder_private_key: env.block_builder_private_key,
            eth_allowance_for_block,
            deposit_check_interval: env.deposit_check_interval,
            accepting_tx_interval: env.accepting_tx_interval,
            proposing_block_interval: env.proposing_block_interval,
            initial_heart_beat_delay: env.initial_heart_beat_delay,
            heart_beat_interval: env.heart_beat_interval,
        };
        Self {
            config,
            validity_prover_client,
            rollup_contract,
            registry_contract,

            force_post: Arc::new(RwLock::new(false)),
            next_deposit_index: Arc::new(RwLock::new(0)),
            registration_state: Arc::new(RwLock::new(BuilderState::default())),
            non_registration_state: Arc::new(RwLock::new(BuilderState::default())),
        }
    }

    async fn emit_heart_beat(&self) -> Result<(), BlockBuilderError> {
        self.registry_contract
            .emit_heart_beat(
                self.config.block_builder_private_key,
                &self.config.block_builder_url,
            )
            .await?;
        Ok(())
    }

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

    // Send a tx request by the user.
    pub async fn send_tx_request(
        &self,
        is_registration_block: bool,
        pubkey: U256,
        tx: Tx,
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
        let block_number = self.rollup_contract.get_latest_block_number().await?;
        let account_info = self.validity_prover_client.get_account_info(pubkey).await?;
        if block_number != account_info.block_number {
            // todo: better error handling, maybe wait for the validity prover to sync
            return Err(BlockBuilderError::ValidityProverIsNotSynced(
                block_number,
                account_info.block_number,
            ));
        }

        if is_registration_block {
            if let Some(account_id) = account_info.account_id {
                return Err(BlockBuilderError::AccountAlreadyRegistered(
                    pubkey, account_id,
                ));
            }
        } else if account_info.account_id.is_none() {
            return Err(BlockBuilderError::AccountNotFound(pubkey));
        }

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
        state.append_tx_request(pubkey, tx);

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
        state.propose_block();
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

    pub async fn num_tx_requests(
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

    // Post the block with the given signatures.
    pub async fn post_block(&self, is_registration_block: bool) -> Result<(), BlockBuilderError> {
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

        let mut account_id_packed = None;
        if is_registration_block {
            for pubkey in memo.pubkeys.iter() {
                if pubkey.is_dummy_pubkey() {
                    // ignore dummy pubkey
                    continue;
                }
                let account_info = self
                    .validity_prover_client
                    .get_account_info(*pubkey)
                    .await?;
                if account_info.account_id.is_some() {
                    // This is unrecoverable so abandon the block
                    self.reset(is_registration_block).await;
                    return Err(BlockBuilderError::AccountAlreadyRegistered(
                        *pubkey,
                        account_info.account_id.unwrap(),
                    ));
                }
            }
        } else {
            let mut account_ids = Vec::new();
            for pubkey in memo.pubkeys.iter() {
                if pubkey.is_dummy_pubkey() {
                    account_ids.push(1); // dummy account id
                    continue;
                }
                let account_info = self
                    .validity_prover_client
                    .get_account_info(*pubkey)
                    .await?;
                if account_info.account_id.is_none() {
                    // This is unrecoverable so abandon the block
                    self.reset(is_registration_block).await;
                    return Err(BlockBuilderError::AccountNotFound(*pubkey));
                }
                account_ids.push(account_info.account_id.unwrap());
            }
            account_id_packed = Some(AccountIdPacked::pack(&account_ids));
        }
        let account_id_hash = account_id_packed.map_or(Bytes32::default(), |ids| ids.hash());
        let mut sender_with_signatures = memo
            .pubkeys
            .iter()
            .map(|pubkey| SenderWithSignature {
                sender: *pubkey,
                signature: None,
            })
            .collect::<Vec<_>>();
        for signature in signatures.iter() {
            let tx_index = memo
                .pubkeys
                .iter()
                .position(|pubkey| pubkey == &signature.pubkey)
                .unwrap(); // safe
            sender_with_signatures[tx_index].signature = Some(signature.signature.clone());
        }
        let signature = construct_signature(
            memo.tx_tree_root,
            memo.expiry,
            memo.pubkey_hash,
            account_id_hash,
            is_registration_block,
            &sender_with_signatures,
        );

        // call contract
        if is_registration_block {
            let trimmed_pubkeys = memo
                .pubkeys
                .into_iter()
                .filter(|pubkey| !pubkey.is_dummy_pubkey())
                .collect::<Vec<_>>();
            self.rollup_contract
                .post_registration_block(
                    self.config.block_builder_private_key,
                    self.config.eth_allowance_for_block,
                    memo.tx_tree_root,
                    memo.expiry,
                    signature.sender_flag,
                    signature.agg_pubkey,
                    signature.agg_signature,
                    signature.message_point,
                    trimmed_pubkeys,
                )
                .await?;
        } else {
            self.rollup_contract
                .post_non_registration_block(
                    self.config.block_builder_private_key,
                    self.config.eth_allowance_for_block,
                    memo.tx_tree_root,
                    memo.expiry,
                    signature.sender_flag,
                    signature.agg_pubkey,
                    signature.agg_signature,
                    signature.message_point,
                    memo.pubkey_hash,
                    account_id_packed.unwrap().to_trimmed_bytes(),
                )
                .await?;
        };
        // update state
        self.state_write(is_registration_block)
            .await
            .finalize_block();
        Ok(())
    }

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

        self.post_block(is_registration_block).await?;

        let force_post = *self.force_post.read().await;
        if force_post {
            *self.force_post.write().await = false;
        }

        Ok(())
    }

    pub async fn evoke_force_post(&self) -> Result<(), BlockBuilderError> {
        *self.force_post.write().await = true;
        Ok(())
    }

    // job
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

    fn post_empty_block_job(self, deposit_check_interval: u64) {
        actix_web::rt::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(deposit_check_interval)).await;
                match self.check_new_deposits().await {
                    Ok(new_deposits_exist) => {
                        if new_deposits_exist {
                            self.evoke_force_post().await.unwrap();
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
        self.clone().cycle_job(true);
        self.clone().cycle_job(false);
        self.clone().emit_heart_beat_job();
    }
}

struct SenderWithSignature {
    sender: U256,
    signature: Option<FlatG2>,
}

fn construct_signature(
    tx_tree_root: Bytes32,
    expiry: u64,
    pubkey_hash: Bytes32,
    account_id_hash: Bytes32,
    is_registration_block: bool,
    sender_with_signatures: &[SenderWithSignature],
) -> SignatureContent {
    assert_eq!(sender_with_signatures.len(), NUM_SENDERS_IN_BLOCK);
    let sender_flag_bits = sender_with_signatures
        .iter()
        .map(|s| s.signature.is_some())
        .collect::<Vec<_>>();
    let sender_flag = Bytes16::from_bits_be(&sender_flag_bits);
    let agg_pubkey = sender_with_signatures
        .iter()
        .map(|s| {
            let weight = hash_to_weight(s.sender, pubkey_hash);
            if s.signature.is_some() {
                let pubkey_g1: G1Affine = G1Affine::recover_from_x(s.sender.into());
                (pubkey_g1 * Fr::from(BigUint::from(weight))).into()
            } else {
                G1Affine::zero()
            }
        })
        .fold(G1Affine::zero(), |acc: G1Affine, x: G1Affine| {
            (acc + x).into()
        });
    let agg_signature = sender_with_signatures
        .iter()
        .map(|s| {
            if let Some(signature) = s.signature.clone() {
                signature.into()
            } else {
                G2Affine::zero()
            }
        })
        .fold(G2Affine::zero(), |acc: G2Affine, x: G2Affine| {
            (acc + x).into()
        });
    // message point
    let message_point = tx_tree_root_and_expiry_to_message_point(tx_tree_root, expiry.into());
    assert!(
        Bn254::pairing(agg_pubkey, message_point)
            == Bn254::pairing(G1Affine::generator(), agg_signature)
    );
    SignatureContent {
        tx_tree_root,
        expiry: expiry.into(),
        is_registration_block,
        sender_flag,
        pubkey_hash,
        account_id_hash,
        agg_pubkey: agg_pubkey.into(),
        agg_signature: agg_signature.into(),
        message_point: message_point.into(),
    }
}
