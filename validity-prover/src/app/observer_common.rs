use std::sync::Arc;

use intmax2_client_sdk::external_api::contract::rollup_contract::FullBlockWithMeta;
use intmax2_zkp::common::witness::full_block::FullBlock;
use server_common::db::DbPool;
use tracing::instrument;

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
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(
        observer.config().observer_sync_interval,
    ));
    loop {
        interval.tick().await;
        observer.sync_events(event_type).await?;
        observer
            .rate_manager()
            .add(&sync_event_success_key(event_type))
            .await?;
    }
}

#[instrument(skip(observer))]
async fn sync_events_job<O: SyncEvent + 'static>(
    observer: Arc<O>,
    event_type: EventType,
) -> Result<(), ObserverSyncError> {
    let observer_restart_interval = observer.config().observer_sync_interval;
    let observer_error_threshold = observer.config().observer_error_threshold;
    let observer_clone = observer.clone();
    // auto restart loop
    loop {
        let error_count = observer_clone
            .rate_manager()
            .count(&sync_event_fail_key(event_type))
            .await?;
        if error_count > observer_error_threshold as usize {
            return Err(ObserverSyncError::ErrorLimitReached(format!(
                "Sync events {} job failed too many times: {}",
                event_type, error_count
            )));
        }

        let observer_clone = observer_clone.clone();
        let handler = actix_web::rt::spawn(async move {
            sync_events_inner_loop(observer_clone, event_type).await
        });
        match handler.await {
            Ok(Ok(_)) => {
                tracing::error!("Sync events job should never return Ok");
            }
            Ok(Err(e)) => {
                tracing::error!("Sync events {} job panic: {}", event_type, e);
            }
            Err(e) => {
                tracing::error!("Sync events {} job error: {}", event_type, e);
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
}

#[instrument(skip(observer))]
pub async fn start_observer_jobs<O: SyncEvent + 'static>(
    observer: Arc<O>,
) -> Result<(), ObserverSyncError> {
    let event_types = vec![
        EventType::Deposited,
        EventType::DepositLeafInserted,
        EventType::BlockPosted,
    ];
    let observer_clone = observer.clone();

    let mut handlers = Vec::new();
    for event_type in event_types {
        let observer_clone = observer_clone.clone();
        let handler = actix_web::rt::spawn(async move {
            sync_events_job(observer_clone, event_type).await?;
            Ok::<(), ObserverSyncError>(())
        });
        handlers.push(handler);
    }

    // tokio::select! {
    //     _ = handlers[0] => {
    //         log::error!("Sync events job for {:?} failed", event_types[0]);
    //     }
    // }

    log::info!("Observer started all jobs");

    Ok(())
}
