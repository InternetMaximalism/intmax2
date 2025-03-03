use intmax2_interfaces::api::validity_prover::interface::ValidityProverClientInterface;
use std::time::Duration;
use tokio::time::sleep;

use super::{block_builder::BlockBuilder, block_post::post_block, error::BlockBuilderError};

pub const DEPOSIT_CHECK_POLLING_INTERVAL: u64 = 2;
pub const GENERAL_POLLING_INTERVAL: u64 = 2;

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
            log::info!(
                "new deposits exist because next_deposit_index={}, latest_included_deposit_index={},",
                next_deposit_index,
                latest_included_deposit_index
                    .map(|i| i as i64)
                    .unwrap_or(-1)
            );
            self.storage.enqueue_empty_block().await?;
        }
        Ok(())
    }

    fn enqueue_empty_block_job(self) {
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

    fn process_requests_job(self, is_registration: bool) {
        actix_web::rt::spawn(async move {
            loop {
                match self.storage.process_requests(is_registration).await {
                    Ok(_) => {}
                    Err(e) => {
                        log::error!("Error in processing requests: {}", e);
                    }
                }
                tokio::time::sleep(Duration::from_secs(GENERAL_POLLING_INTERVAL)).await;
            }
        });
    }

    fn process_signatures_job(self) {
        actix_web::rt::spawn(async move {
            loop {
                match self.storage.process_signatures().await {
                    Ok(_) => {}
                    Err(e) => {
                        log::error!("Error in processing signatures: {}", e);
                    }
                }
                tokio::time::sleep(Duration::from_secs(GENERAL_POLLING_INTERVAL)).await;
            }
        });
    }

    fn process_fee_collection_job(self) {
        actix_web::rt::spawn(async move {
            loop {
                match self
                    .storage
                    .process_fee_collection(&self.store_vault_server_client)
                    .await
                {
                    Ok(_) => {}
                    Err(e) => {
                        log::error!("Error in processing fee collection: {}", e);
                    }
                }
                tokio::time::sleep(Duration::from_secs(GENERAL_POLLING_INTERVAL)).await;
            }
        });
    }

    async fn post_block(&self) -> Result<(), BlockBuilderError> {
        let block_post_task = self.storage.dequeue_block_post_task().await?;
        if block_post_task.is_none() {
            return Ok(());
        }
        let block_post_task = block_post_task.unwrap();
        log::info!("Posting block: {}", block_post_task.block_id);
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
                match self.post_block().await {
                    Ok(_) => {}
                    Err(e) => {
                        log::error!("Error in post block job: {}", e);
                    }
                }
                sleep(Duration::from_secs(GENERAL_POLLING_INTERVAL)).await;
            }
        });
    }

    pub fn run(&self) {
        self.clone().enqueue_empty_block_job();
        self.clone().post_block_job();
        self.clone().emit_heart_beat_job();
        self.clone().process_requests_job(true);
        self.clone().process_requests_job(false);
        self.clone().process_signatures_job();
        self.clone().process_fee_collection_job();
    }
}
