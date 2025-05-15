use std::sync::Arc;

use intmax2_client_sdk::external_api::contract::rollup_contract::FullBlockWithMeta;
use intmax2_zkp::common::witness::full_block::FullBlock;
use server_common::db::DbPool;
use tracing::{error, info, instrument, warn};

use crate::EnvVar;

use super::{
    check_point_store::EventType,
    error::{ObserverError, ObserverSyncError},
    rate_manager::RateManager,
};

pub fn sync_event_success_key(event_type: EventType) -> String {
    format!("sync_events_success_{}", event_type)
}

pub fn sync_event_fail_key(event_type: EventType) -> String {
    format!("sync_events_fail_{}", event_type)
}

#[derive(Debug, Clone)]
pub struct ObserverConfig {
    pub observer_event_block_interval: u64,
    pub observer_max_query_times: usize,
    pub observer_sync_interval: u64,
    pub observer_restart_interval: u64,
    pub observer_error_threshold: u64,

    pub rollup_contract_deployed_block_number: u64,
    pub liquidity_contract_deployed_block_number: u64,
}

impl ObserverConfig {
    pub fn from_env(env: &EnvVar) -> Self {
        Self {
            observer_event_block_interval: env.observer_event_block_interval,
            observer_max_query_times: env.observer_max_query_times,
            observer_sync_interval: env.observer_sync_interval,
            observer_restart_interval: env.observer_restart_interval,
            observer_error_threshold: env.observer_error_threshold,
            rollup_contract_deployed_block_number: env.rollup_contract_deployed_block_number,
            liquidity_contract_deployed_block_number: env.liquidity_contract_deployed_block_number,
        }
    }
}

#[async_trait::async_trait(?Send)]
pub trait SyncEvent {
    fn name(&self) -> String;

    fn config(&self) -> ObserverConfig;

    fn rate_manager(&self) -> &RateManager;

    async fn sync_events(&self, event_type: EventType) -> Result<(), ObserverError>;
}

pub async fn initialize_observer_db(pool: DbPool) -> Result<(), ObserverError> {
    // Initialize with genesis block if table is empty
    let count = sqlx::query!("SELECT COUNT(*) as count FROM full_blocks")
        .fetch_one(&pool)
        .await?
        .count
        .unwrap_or(0);
    if count == 0 {
        let genesis = FullBlockWithMeta {
            full_block: FullBlock::genesis(),
            eth_block_number: 0,
            eth_tx_index: 0,
        };
        // Insert genesis block
        sqlx::query!(
            "INSERT INTO full_blocks (block_number, eth_block_number, eth_tx_index, full_block) 
                 VALUES ($1, $2, $3, $4)",
            0i32,
            genesis.eth_block_number as i64,
            genesis.eth_tx_index as i64,
            bincode::serialize(&genesis.full_block).unwrap()
        )
        .execute(&pool)
        .await?;
    }
    Ok(())
}

#[instrument(skip(observer))]
async fn sync_events_inner_loop<O: SyncEvent>(
    observer: Arc<O>,
    event_type: EventType,
) -> Result<(), ObserverError> {
    let config = observer.config();
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(
        config.observer_sync_interval,
    ));
    loop {
        interval.tick().await;

        let rate_manager = observer.rate_manager();
        let stop = rate_manager.get_stop_flag(&observer.name()).await?;
        if stop {
            info!("Stopping sync events because of stop flag, {}", event_type);
            return Ok(());
        }
        observer.sync_events(event_type).await?;

        rate_manager
            .add(&sync_event_success_key(event_type))
            .await?;
        rate_manager.cleanup().await?;
    }
}

#[instrument(skip(observer))]
async fn sync_events_job<O: SyncEvent + 'static>(
    observer: Arc<O>,
    event_type: EventType,
) -> Result<(), ObserverSyncError> {
    let observer_restart_interval = observer.config().observer_sync_interval;
    let observer_error_threshold = observer.config().observer_error_threshold as usize;
    let observer_clone = observer.clone();
    // auto restart loop
    loop {
        let fail_count = observer
            .rate_manager()
            .count(&sync_event_fail_key(event_type))
            .await?;
        if fail_count >= observer_error_threshold {
            // stop the job
            observer
                .rate_manager()
                .set_stop_flag(&observer.name(), true)
                .await?;
            warn!(
                "Stopping sync events job for {} because error limit reached",
                event_type
            );
            return Ok(());
        }

        let observer_clone = observer_clone.clone();
        let handler = actix_web::rt::spawn(async move {
            sync_events_inner_loop(observer_clone, event_type).await
        });
        match handler.await {
            Ok(Ok(_)) => {
                info!("Sync events {} job finished", event_type);
                break;
            }
            Ok(Err(e)) => {
                error!("Sync events {} job panic: {}", event_type, e);
            }
            Err(e) => {
                error!("Sync events {} job error: {}", event_type, e);
            }
        }
        observer
            .rate_manager()
            .add(&sync_event_fail_key(event_type))
            .await?;

        // wait for a while before restarting
        tokio::time::sleep(tokio::time::Duration::from_secs(observer_restart_interval)).await;
        log::info!("Restarting sync events job for {}", event_type);
    }
    Ok(())
}

#[instrument(skip(observer))]
pub async fn start_observer_jobs<O: SyncEvent + 'static>(
    observer: Arc<O>,
) -> Result<(), ObserverSyncError> {
    let observer_clone = observer.clone();
    let deposited_handler = actix_web::rt::spawn(async move {
        sync_events_job(observer_clone, EventType::Deposited).await?;
        Ok::<(), ObserverSyncError>(())
    });
    let observer_clone = observer.clone();
    let deposit_leaf_inserted_handler = actix_web::rt::spawn(async move {
        sync_events_job(observer_clone, EventType::DepositLeafInserted).await?;
        Ok::<(), ObserverSyncError>(())
    });
    let observer_clone = observer.clone();
    let block_posted_handler = actix_web::rt::spawn(async move {
        sync_events_job(observer_clone, EventType::BlockPosted).await?;
        Ok::<(), ObserverSyncError>(())
    });
    tokio::select! {
        result = deposited_handler => {
            match result {
                Ok(Ok(_)) => {
                    log::info!("Sync events job for {:?} finished", EventType::Deposited);
                }
                Ok(Err(e)) => {
                    log::error!("Sync events job for {:?} failed: {}", EventType::Deposited, e);
                }
                Err(e) => {
                    log::error!("Sync events job for {:?} error: {}", EventType::Deposited, e);
                }
            }
        }
        result = deposit_leaf_inserted_handler => {
            match result {
                Ok(Ok(_)) => {
                    log::info!("Sync events job for {:?} finished", EventType::DepositLeafInserted);
                }
                Ok(Err(e)) => {
                    log::error!("Sync events job for {:?} failed: {}", EventType::DepositLeafInserted, e);
                }
                Err(e) => {
                    log::error!("Sync events job for {:?} error: {}", EventType::DepositLeafInserted, e);
                }
            }
        }
        result = block_posted_handler => {
            match result {
                Ok(Ok(_)) => {
                    log::info!("Sync events job for {:?} finished", EventType::BlockPosted);
                }
                Ok(Err(e)) => {
                    log::error!("Sync events job for {:?} failed: {}", EventType::BlockPosted, e);
                }
                Err(e) => {
                    log::error!("Sync events job for {:?} error: {}", EventType::BlockPosted, e);
                }
            }
        }
    }
    Ok(())
}

#[instrument(skip(primary_observer, secondary_observer))]
pub async fn run_and_switch_observers<P: SyncEvent + 'static, S: SyncEvent + 'static>(
    primary_observer: Arc<P>,
    secondary_observer: Option<Arc<S>>,
) {
    let primary_observer_clone = primary_observer.clone();
    actix_web::rt::spawn(async move {
        start_observer_jobs(primary_observer_clone).await?;

        error!(
            "Primary observer job finished, sleeping {} seconds",
            primary_observer.config().observer_restart_interval
        );
        tokio::time::sleep(tokio::time::Duration::from_secs(
            primary_observer.config().observer_restart_interval,
        ))
        .await;

        warn!("Clearing rate manager");
        primary_observer.rate_manager().reset().await?;

        if let Some(secondary_observer) = secondary_observer {
            warn!("Switching to secondary observer");
            start_observer_jobs(secondary_observer).await?;
        } else {
            warn!("No secondary observer to switch to");
        }
        error!("Observer job finished, exiting");
        Ok::<(), ObserverSyncError>(())
    });
}
