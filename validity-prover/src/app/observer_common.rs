use std::sync::Arc;

use intmax2_client_sdk::external_api::contract::rollup_contract::FullBlockWithMeta;
use intmax2_zkp::common::witness::full_block::FullBlock;
use server_common::db::DbPool;
use tracing::instrument;

use super::{check_point_store::EventType, error::ObserverError};

#[derive(Debug, Clone)]
pub struct ObserverConfig {
    pub observer_event_block_interval: u64,
    pub observer_max_query_times: usize,
    pub observer_sync_interval: u64,
    pub observer_restart_interval: u64,

    pub rollup_contract_deployed_block_number: u64,
    pub liquidity_contract_deployed_block_number: u64,
}

#[async_trait::async_trait(?Send)]
pub trait SyncEvent {
    fn config(&self) -> ObserverConfig;

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
    }
}

#[instrument(skip(observer))]
async fn sync_events_job<O: SyncEvent + 'static>(observer: Arc<O>, event_type: EventType) {
    let observer_restart_interval = observer.config().observer_sync_interval;
    let observer_clone = observer.clone();
    // auto restart loop
    loop {
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
        // wait for a while before restarting
        tokio::time::sleep(tokio::time::Duration::from_secs(observer_restart_interval)).await;
        log::info!("Restarting sync events job for {}", event_type);
    }
}

#[instrument(skip(observer))]
pub fn start_observer_jobs<O: SyncEvent + 'static>(observer: Arc<O>) {
    let event_types = vec![
        EventType::Deposited,
        EventType::DepositLeafInserted,
        EventType::BlockPosted,
    ];
    let observer_clone = observer.clone();
    for event_type in event_types {
        let observer_clone = observer_clone.clone();
        actix_web::rt::spawn(async move {
            sync_events_job(observer_clone, event_type).await;
        });
    }
    log::info!("Observer started all jobs");
}
