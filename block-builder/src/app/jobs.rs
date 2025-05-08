use intmax2_interfaces::api::validity_prover::interface::ValidityProverClientInterface;
use std::time::Duration;
use tokio::{task::JoinHandle, time::sleep};

use super::{block_builder::BlockBuilder, block_post::post_block, error::BlockBuilderError};

pub const GENERAL_POLLING_INTERVAL: u64 = 2;
pub const RESTART_JOB_INTERVAL: u64 = 60;

impl BlockBuilder {
    async fn emit_heart_beat(&self) -> Result<(), BlockBuilderError> {
        self.registry_contract
            .emit_heart_beat(
                self.config.block_builder_private_key,
                None,
                &self.config.block_builder_url,
            )
            .await?;
        Ok(())
    }

    fn emit_heart_beat_job(self) -> JoinHandle<Result<(), BlockBuilderError>> {
        actix_web::rt::spawn(async move {
            // wait for the initial heart beat
            tokio::time::sleep(Duration::from_secs(self.config.initial_heart_beat_delay)).await;

            // emit heart beat periodically
            let mut interval =
                tokio::time::interval(Duration::from_secs(self.config.heart_beat_interval));
            loop {
                interval.tick().await;
                self.emit_heart_beat().await?;
            }
        })
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

    fn enqueue_empty_block_job(self) -> JoinHandle<Result<(), BlockBuilderError>> {
        actix_web::rt::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(GENERAL_POLLING_INTERVAL));
            loop {
                interval.tick().await;
                self.enqueue_empty_block().await?;
            }
        })
    }

    fn process_requests_job(
        self,
        is_registration: bool,
    ) -> JoinHandle<Result<(), BlockBuilderError>> {
        actix_web::rt::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(GENERAL_POLLING_INTERVAL));
            loop {
                interval.tick().await;
                self.storage.process_requests(is_registration).await?;
            }
        })
    }

    fn process_signatures_job(self) -> JoinHandle<Result<(), BlockBuilderError>> {
        actix_web::rt::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(GENERAL_POLLING_INTERVAL));
            loop {
                interval.tick().await;
                self.storage.process_signatures().await?;
            }
        })
    }

    fn process_fee_collection_job(self) -> JoinHandle<Result<(), BlockBuilderError>> {
        actix_web::rt::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(GENERAL_POLLING_INTERVAL));
            loop {
                interval.tick().await;
                self.storage
                    .process_fee_collection(self.store_vault_server_client.as_ref().as_ref())
                    .await?;
            }
        })
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
            self.config.gas_limit_for_block_post,
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

    fn post_block_job(self) -> JoinHandle<Result<(), BlockBuilderError>> {
        actix_web::rt::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(GENERAL_POLLING_INTERVAL));
            loop {
                interval.tick().await;
                self.post_block().await?;
            }
        })
    }

    fn run_with_restart(
        self,
        job: impl Fn(Self) -> JoinHandle<Result<(), BlockBuilderError>> + Send + 'static,
        job_name: String,
    ) {
        actix_web::rt::spawn(async move {
            loop {
                match job(self.clone()).await {
                    Ok(result) => {
                        if let Err(e) = result {
                            log::error!(
                                "Error in {}: {}. Restarting in {}sec ...",
                                job_name,
                                e,
                                RESTART_JOB_INTERVAL
                            );
                        }
                    }
                    Err(e) => {
                        log::error!(
                            "Panic in {}: {}. Restarting in {}sec ...",
                            job_name,
                            e,
                            RESTART_JOB_INTERVAL
                        );
                    }
                }
                sleep(Duration::from_secs(RESTART_JOB_INTERVAL)).await;
            }
        });
    }

    pub fn run(&self) {
        self.clone().run_with_restart(
            |this| this.emit_heart_beat_job(),
            "emit_heart_beat_job".to_string(),
        );
        self.clone().run_with_restart(
            |this| this.enqueue_empty_block_job(),
            "enqueue_empty_block_job".to_string(),
        );
        self.clone()
            .run_with_restart(|this| this.post_block_job(), "post_block_job".to_string());
        self.clone().run_with_restart(
            |this| this.process_requests_job(true),
            "process_registration_requests_job".to_string(),
        );
        self.clone().run_with_restart(
            |this| this.process_requests_job(false),
            "process_non_registration_requests_job".to_string(),
        );
        self.clone().run_with_restart(
            |this| this.process_signatures_job(),
            "process_signatures_job".to_string(),
        );
        self.clone().run_with_restart(
            |this| this.process_fee_collection_job(),
            "process_fee_collection_job".to_string(),
        );
    }
}
