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
use tokio::{sync::RwLock, time::sleep};
use uuid::Uuid;

use super::{
    block_builder::BlockBuilder,
    block_post::{self, post_block},
    error::BlockBuilderError,
};

pub const POST_BLOCK_POLLING_INTERVAL: u64 = 2;
pub const DEPOSIT_CHECK_POLLING_INTERVAL: u64 = 2;

impl BlockBuilder {
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

    async fn enqueue_empty_block(&self) -> Result<(), BlockBuilderError> {
        let next_deposit_index = self.validity_prover_client.get_next_deposit_index().await?;
        let latest_included_deposit_index = self
            .validity_prover_client
            .get_latest_included_deposit_index()
            .await?;

        let does_new_deposits_exist =
            if let Some(latest_included_deposit_index) = latest_included_deposit_index {
                next_deposit_index > latest_included_deposit_index + 1
            } else {
                next_deposit_index > 0
            };
        if does_new_deposits_exist {
            self.storage.enqueue_empty_block().await?;
        }
        Ok(())
    }

    fn post_empty_block_job(self) {
        actix_web::rt::spawn(async move {
            loop {
                match self.enqueue_empty_block().await {
                    Ok(_) => {}
                    Err(e) => {
                        log::error!("Error in checking new deposits: {}", e);
                    }
                }
                tokio::time::sleep(Duration::from_secs(DEPOSIT_CHECK_POLLING_INTERVAL)).await;
            }
        });
    }


    



    async fn post_block_inner(&self) -> Result<(), BlockBuilderError> {
        let block_post_task = self.storage.dequeue_block_post_task().await?;
        if block_post_task.is_none() {
            return Ok(());
        }
        let block_post_task = block_post_task.unwrap();
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
                sleep(Duration::from_secs(POST_BLOCK_POLLING_INTERVAL)).await;
            }
        });
    }

    pub fn run(&self) {
        self.clone().post_empty_block_job();
        self.clone().post_block_job();
        self.clone().emit_heart_beat_job();
    }
}
