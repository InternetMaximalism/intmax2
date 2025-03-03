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
    common::{
        block_builder::{BlockProposal, UserSignature},
        tx::Tx,
    },
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
};
use std::{collections::HashMap, sync::Arc, time::Duration};
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
            deposit_check_interval: env.deposit_check_interval,
            redis_url: env.redis_url.clone(),
            block_builder_id: Uuid::new_v4().to_string(),
        };
        let storage = Box::new(InMemoryStorage::new(&storage_config));

        let config = Config {
            block_builder_url: env.block_builder_url.clone(),
            block_builder_private_key: env.block_builder_private_key,
            eth_allowance_for_block,
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
    ) -> Result<String, BlockBuilderError> {
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

        let request_id = Uuid::new_v4().to_string();
        let tx_request = TxRequest {
            pubkey,
            account_id,
            tx,
            fee_proof: fee_proof.clone(),
            request_id: request_id.clone(),
        };
        self.storage
            .add_tx(is_registration_block, tx_request)
            .await?;

        Ok(request_id)
    }

    // Query the constructed proposal by the user.
    pub async fn query_proposal(
        &self,
        request_id: &str,
    ) -> Result<Option<BlockProposal>, BlockBuilderError> {
        log::info!("query_proposal request_id: {}", request_id);
        let proposal = self.storage.query_proposal(request_id).await?;
        Ok(proposal)
    }

    // Post the signature by the user.
    pub async fn post_signature(
        &self,
        request_id: &str,
        signature: UserSignature,
    ) -> Result<(), BlockBuilderError> {
        log::info!("post_signature request_id: {}", request_id);
        self.storage.add_signature(request_id, signature).await?;
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

    async fn post_empty_block(&self) -> Result<(), BlockBuilderError> {
        // Enqueue an empty block for deposit checking
        self.storage.enqueue_empty_block().await?;
        Ok(())
            .get_latest_included_deposit_index()
            .await?;

        let does_new_deposits_exist =
            if let Some(latest_included_deposit_index) = latest_included_deposit_index {
                next_deposit_index > latest_included_deposit_index + 1
            } else {
                next_deposit_index > 0
            };

        // if does_new_deposits_exist && self.storage.acquire_empty_block_lock().await? {
        //     self.storage.
        // }


        todo!()
    }

    fn post_empty_block_job(self, deposit_check_interval: u64) {
        actix_web::rt::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(deposit_check_interval)).await;
                match self.does_new_deposits_exist().await {
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
}

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
