use ethers::types::H256;
use intmax2_client_sdk::external_api::{
    contract::{
        block_builder_registry::BlockBuilderRegistryContract, rollup_contract::RollupContract,
    },
    store_vault_server::StoreVaultServerClient,
    validity_prover::ValidityProverClient,
};
use intmax2_interfaces::api::{
    block_builder::interface::{BlockBuilderFeeInfo, FeeProof},
    validity_prover::interface::ValidityProverClientInterface,
};
use intmax2_zkp::{
    common::tx::Tx,
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    app::{fee::validate_fee_proof, types::TxRequest},
    EnvVar,
};

use super::{
    error::BlockBuilderError,
    fee::{convert_fee_vec, parse_fee_str},
    storage::{config::StorageConfig, memory_storage::InMemoryStorage, Storage},
};

pub const DEFAULT_POST_BLOCK_CHANNEL: u64 = 100;

#[derive(Debug, Clone)]
struct Config {
    block_builder_url: String,
    block_builder_private_key: H256,
    eth_allowance_for_block: U256,
    deposit_check_interval: Option<u64>,
    initial_heart_beat_delay: u64,
    heart_beat_interval: u64,

    // fees
    beneficiary_pubkey: Option<U256>,
    registration_fee: Option<HashMap<u32, U256>>,
    non_registration_fee: Option<HashMap<u32, U256>>,
    registration_collateral_fee: Option<HashMap<u32, U256>>,
    non_registration_collateral_fee: Option<HashMap<u32, U256>>,
}

pub struct BlockBuilder {
    config: Config,
    store_vault_server_client: StoreVaultServerClient,
    validity_prover_client: ValidityProverClient,
    rollup_contract: RollupContract,
    registry_contract: BlockBuilderRegistryContract,

    storage: Box<dyn Storage>,
    force_post: Arc<RwLock<bool>>,
    next_deposit_index: Arc<RwLock<u32>>,
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

        let storage_config = StorageConfig {
            use_fee: registration_fee.is_some() || non_registration_fee.is_some(),
            use_collateral: registration_collateral_fee.is_some() || non_registration_fee.is_some(),
            fee_beneficiary: beneficiary_pubkey.unwrap_or_default(),
            tx_timeout: env.tx_timeout,
            accepting_tx_interval: env.accepting_tx_interval,
            proposing_block_interval: env.proposing_block_interval,
            redis_url: env.redis_url.clone(),
            block_builder_id: Uuid::new_v4().to_string(),
        };
        let storage = Box::new(InMemoryStorage::new(&storage_config));

        let config = Config {
            block_builder_url: env.block_builder_url.clone(),
            block_builder_private_key: env.block_builder_private_key,
            eth_allowance_for_block,
            deposit_check_interval: env.deposit_check_interval,
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
            storage,
            force_post: Arc::new(RwLock::new(false)),
            next_deposit_index: Arc::new(RwLock::new(0)),
        })
    }

    pub fn get_fee_info(&self) -> BlockBuilderFeeInfo {
        BlockBuilderFeeInfo {
            beneficiary: self.config.beneficiary_pubkey,
            registration_fee: convert_fee_vec(&self.config.registration_fee),
            non_registration_fee: convert_fee_vec(&self.config.non_registration_fee),
            registration_collateral_fee: convert_fee_vec(&self.config.registration_collateral_fee),
            non_registration_collateral_fee: convert_fee_vec(
                &self.config.non_registration_collateral_fee,
            ),
        }
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

        let tx_request = TxRequest {
            pubkey,
            account_id,
            tx,
            fee_proof: fee_proof.clone(),
            request_id: Uuid::new_v4().to_string(),
        };
        self.storage
            .add_tx(is_registration_block, tx_request)
            .await?;

        Ok(())
    }

    //     // Query the constructed proposal by the user.
    //     pub async fn query_proposal(
    //         &self,
    //         is_registration_block: bool,
    //         pubkey: U256,
    //         tx: Tx,
    //     ) -> Result<Option<BlockProposal>, BlockBuilderError> {
    //         log::info!(
    //             "query_proposal is_registration_block: {}",
    //             is_registration_block
    //         );
    //         let state = self.state_read(is_registration_block).await;
    //         if state.is_pausing() {
    //             return Err(BlockBuilderError::BlockBuilderIsPausing);
    //         }
    //         if state.is_accepting_txs() && !state.is_request_contained(pubkey, tx) {
    //             return Err(BlockBuilderError::TxRequestNotFound);
    //         }
    //         Ok(state.query_proposal(pubkey, tx))
    //     }
}

//     // Post the signature by the user.
//     pub async fn post_signature(
//         &self,
//         is_registration_block: bool,
//         tx: Tx,
//         signature: UserSignature,
//     ) -> Result<(), BlockBuilderError> {
//         log::info!(
//             "post_signature is_registration_block: {}",
//             is_registration_block
//         );
//         let mut state = self.state_write(is_registration_block).await;
//         if !state.is_proposing_block() {
//             return Err(BlockBuilderError::NotProposing);
//         }
//         if state.is_request_contained(signature.pubkey, tx) {
//             return Err(BlockBuilderError::TxRequestNotFound);
//         }
//         let memo = state.get_proposal_memo().unwrap();
//         signature
//             .verify(memo.tx_tree_root, memo.expiry, memo.pubkey_hash)
//             .map_err(|e| BlockBuilderError::InvalidSignature(e.to_string()))?;
//         // update state
//         state.append_signature(signature);
//         Ok(())
//     }

//     async fn check_new_deposits(&self) -> Result<bool, BlockBuilderError> {
//         log::info!("check_new_deposits");
//         let next_deposit_index = self.validity_prover_client.get_next_deposit_index().await?;
//         let current_next_deposit_index = *self.next_deposit_index.read().await; // release the lock immediately

//         // sanity check
//         if next_deposit_index < current_next_deposit_index {
//             return Err(BlockBuilderError::UnexpectedError(format!(
//                 "next_deposit_index is smaller than the current one: {} < {}",
//                 next_deposit_index, current_next_deposit_index
//             )));
//         }
//         if next_deposit_index == current_next_deposit_index {
//             return Ok(false);
//         }

//         // update the next deposit index
//         *self.next_deposit_index.write().await = next_deposit_index;

//         log::info!("new deposit found: {}", next_deposit_index);
//         Ok(true)
//     }

//     /// Reset the block builder.
//     async fn reset(&self, is_registration_block: bool) {
//         log::info!("reset");
//         let mut state = self.state_write(is_registration_block).await;
//         *state = BuilderState::default();
//     }

//     // Cycle of the block builder.
//     async fn cycle(&self, is_registration_block: bool) -> Result<(), BlockBuilderError> {
//         log::info!("cycle is_registration_block: {}", is_registration_block);
//         self.start_accepting_txs(is_registration_block).await?;

//         tokio::time::sleep(Duration::from_secs(self.config.accepting_tx_interval)).await;

//         let num_tx_requests = self.num_tx_requests(is_registration_block).await?;
//         let force_post = *self.force_post.read().await;
//         if num_tx_requests == 0 && (is_registration_block || !force_post) {
//             log::info!("No tx requests, not constructing block");
//             self.reset(is_registration_block).await;
//             return Ok(());
//         }

//         self.construct_block(is_registration_block).await?;

//         tokio::time::sleep(Duration::from_secs(self.config.proposing_block_interval)).await;

//         let force_post = *self.force_post.read().await;
//         self.post_block(is_registration_block, force_post).await?;

//         let force_post = *self.force_post.read().await;
//         if force_post {
//             *self.force_post.write().await = false;
//         }

//         Ok(())
//     }

//     // job
//     async fn emit_heart_beat(&self) -> Result<(), BlockBuilderError> {
//         self.registry_contract
//             .emit_heart_beat(
//                 self.config.block_builder_private_key,
//                 &self.config.block_builder_url,
//             )
//             .await?;
//         Ok(())
//     }

//     fn emit_heart_beat_job(self) {
//         let start_time = chrono::Utc::now().timestamp() as u64;
//         actix_web::rt::spawn(async move {
//             let now = chrono::Utc::now().timestamp() as u64;
//             let initial_heartbeat_time = start_time + self.config.initial_heart_beat_delay;
//             let delay_secs = if initial_heartbeat_time > now {
//                 initial_heartbeat_time - now
//             } else {
//                 0
//             };

//             // wait for the initial heart beat
//             tokio::time::sleep(Duration::from_secs(delay_secs)).await;

//             // emit initial heart beat
//             match self.emit_heart_beat().await {
//                 Ok(_) => log::info!("Initial heart beat emitted"),
//                 Err(e) => log::error!("Error in emitting initial heart beat: {}", e),
//             }

//             // emit heart beat periodically
//             loop {
//                 tokio::time::sleep(Duration::from_secs(self.config.heart_beat_interval)).await;
//                 match self.emit_heart_beat().await {
//                     Ok(_) => log::info!("Heart beat emitted"),
//                     Err(e) => log::error!("Error in emitting heart beat: {}", e),
//                 }
//             }
//         });
//     }

//     async fn post_block_inner(&self) -> Result<(), BlockBuilderError> {
//         let mut rx_high = self.rx_high.lock().await;
//         let mut rx_low = self.rx_low.lock().await;
//         let block_post_task = tokio::select! {
//             Some(t) =  rx_high.recv() => {
//                 t
//             }
//             Some(t) = rx_low.recv()  => {
//                 t
//             }
//             else => {
//                 return Err(BlockBuilderError::QueueError("No block post task".to_string()));
//             }
//         };

//         match post_block(
//             self.config.block_builder_private_key,
//             self.config.eth_allowance_for_block,
//             &self.rollup_contract,
//             &self.validity_prover_client,
//             block_post_task,
//         )
//         .await
//         {
//             Ok(_) => {}
//             Err(e) => {
//                 log::error!("Error in posting block: {}", e);
//             }
//         }
//         Ok(())
//     }

//     fn post_block_job(self) {
//         actix_web::rt::spawn(async move {
//             loop {
//                 match self.post_block_inner().await {
//                     Ok(_) => {}
//                     Err(e) => {
//                         log::error!("Error in post block job: {}", e);
//                     }
//                 }
//                 sleep(Duration::from_secs(10)).await;
//             }
//         });
//     }

//     fn post_empty_block_job(self, deposit_check_interval: u64) {
//         actix_web::rt::spawn(async move {
//             loop {
//                 tokio::time::sleep(Duration::from_secs(deposit_check_interval)).await;
//                 match self.check_new_deposits().await {
//                     Ok(new_deposits_exist) => {
//                         if new_deposits_exist {
//                             *self.force_post.write().await = true;
//                         }
//                     }
//                     Err(e) => {
//                         log::error!("Error in checking new deposits: {}", e);
//                     }
//                 }
//             }
//         });
//     }

//     fn cycle_job(self, is_registration_block: bool) {
//         actix_web::rt::spawn(async move {
//             loop {
//                 match self.cycle(is_registration_block).await {
//                     Ok(_) => {
//                         log::info!(
//                             "Cycle successful for registration block: {}",
//                             is_registration_block
//                         );
//                     }
//                     Err(e) => {
//                         log::error!("Error in block builder: {}", e);
//                         self.reset(is_registration_block).await;
//                         *self.force_post.write().await = false;
//                         sleep(Duration::from_secs(10)).await;
//                     }
//                 }
//             }
//         });
//     }

//     pub fn run(&self) {
//         if let Some(deposit_check_interval) = self.config.deposit_check_interval {
//             self.clone().post_empty_block_job(deposit_check_interval);
//         }
//         self.clone().post_block_job();
//         self.clone().cycle_job(true);
//         self.clone().cycle_job(false);
//         self.clone().emit_heart_beat_job();
//     }
// }
