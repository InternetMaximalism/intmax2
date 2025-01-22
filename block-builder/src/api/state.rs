use std::{sync::Arc, time::Duration};

use tokio::{sync::RwLock, time::sleep};

use crate::Env;

use super::{block_builder::BlockBuilder, error::BlockBuilderError};

#[derive(Debug, Clone)]
pub struct State {
    pub is_shutting_down: Arc<RwLock<bool>>,
    pub force_post: Arc<RwLock<bool>>,
    pub block_builder: Arc<RwLock<BlockBuilder>>,
}

impl State {
    pub fn new(block_builder: BlockBuilder) -> Self {
        State {
            is_shutting_down: Arc::new(RwLock::new(false)),
            force_post: Arc::new(RwLock::new(false)),
            block_builder: Arc::new(RwLock::new(block_builder)),
        }
    }

    pub async fn post_empty_block_job(self, deposit_check_interval: u64) {
        actix_web::rt::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(deposit_check_interval)).await;
                match self.block_builder.write().await.check_new_deposits().await {
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

    pub async fn main_job(self, is_registration_block: bool) {
        actix_web::rt::spawn(async move {
            loop {
                if *self.is_shutting_down.read().await {
                    log::info!("Shutting down block builder");
                    break;
                }
                match self.cycle(is_registration_block).await {
                    Ok(_) => {
                        log::info!(
                            "Cycle successful for registration block: {}",
                            is_registration_block
                        );
                    }
                    Err(e) => {
                        log::error!("Error in block builder: {}", e);
                        self.block_builder
                            .write()
                            .await
                            .reset(is_registration_block);
                        *self.force_post.write().await = false;
                        sleep(Duration::from_secs(10)).await;
                    }
                }
            }
        });
    }

    pub async fn evoke_force_post(&self) -> Result<(), BlockBuilderError> {
        *self.force_post.write().await = true;
        Ok(())
    }

    async fn cycle(&self, is_registration_block: bool) -> Result<(), BlockBuilderError> {
        let env = envy::from_env::<Env>().unwrap();

        self.block_builder
            .write()
            .await
            .start_accepting_txs(is_registration_block)?;

        tokio::time::sleep(Duration::from_secs(env.accepting_tx_interval)).await;

        let num_tx_requests = self
            .block_builder
            .read()
            .await
            .num_tx_requests(is_registration_block)
            .await?;
        let force_post = *self.force_post.read().await;
        if num_tx_requests == 0 && (is_registration_block || !force_post) {
            log::info!("No tx requests, not constructing block");
            self.block_builder
                .write()
                .await
                .reset(is_registration_block);
            return Ok(());
        }

        self.block_builder
            .write()
            .await
            .construct_block(is_registration_block)?;

        tokio::time::sleep(Duration::from_secs(env.proposing_block_interval)).await;

        self.block_builder
            .write()
            .await
            .post_block(is_registration_block)
            .await?;

        let force_post = *self.force_post.read().await;
        if force_post {
            *self.force_post.write().await = false;
        }

        Ok(())
    }

    pub async fn run(&self) {
        self.clone().main_job(true).await;
        self.clone().main_job(false).await;

        let env = envy::from_env::<Env>().unwrap();
        if let Some(deposit_check_interval) = env.deposit_check_interval {
            self.clone()
                .post_empty_block_job(deposit_check_interval)
                .await;
        }
    }
}
