use intmax2_client_sdk::external_api::contract::{
    liquidity_contract::LiquidityContract, rollup_contract::RollupContract,
};
use intmax2_zkp::{
    ethereum_types::u32limb_trait::U32LimbTrait as _, utils::leafable::Leafable as _,
};
use server_common::db::{DbPool, DbPoolConfig};
use tracing::{debug, info, instrument};

use crate::{app::observer_common::initialize_observer_db, EnvVar};

use super::{
    check_point_store::EventType,
    error::ObserverError,
    leader_election::LeaderElection,
    observer_api::ObserverApi,
    observer_common::{ObserverConfig, SyncEvent},
    rate_manager::RateManager,
    the_graph::client::TheGraphClient,
};

const EVENT_LIMIT: usize = 100;

#[derive(Clone)]
pub struct TheGraphObserver {
    pub config: ObserverConfig,
    pub rollup_contract: RollupContract,
    pub liquidity_contract: LiquidityContract,
    pub graph_client: TheGraphClient,
    pub observer_api: ObserverApi,
    pub leader_election: LeaderElection,
    pub rate_manager: RateManager,
    pub pool: DbPool,
}

impl TheGraphObserver {
    pub async fn new(
        env: &EnvVar,
        observer_api: ObserverApi,
        leader_election: LeaderElection,
        rate_manager: RateManager,
    ) -> Result<Self, ObserverError> {
        let config = ObserverConfig::from_env(env);
        tracing::info!("Observer config: {:?}", config);
        let pool = DbPool::from_config(&DbPoolConfig {
            max_connections: env.database_max_connections,
            idle_timeout: env.database_timeout,
            url: env.database_url.to_string(),
        })
        .await?;
        if env.the_graph_l1_url.is_none() || env.the_graph_l2_url.is_none() {
            return Err(ObserverError::EnvError(
                "L1 and L2 The Graph URLs must be provided".to_string(),
            ));
        }
        let rollup_contract = observer_api.rollup_contract.clone();
        let liquidity_contract = observer_api.liquidity_contract.clone();
        let graph_client = TheGraphClient::new(
            env.the_graph_l1_url.clone().unwrap(),
            env.the_graph_l2_url.clone().unwrap(),
            env.the_graph_l1_bearer.clone(),
            env.the_graph_l2_bearer.clone(),
            liquidity_contract.provider.clone(),
            rollup_contract.provider.clone(),
        );

        initialize_observer_db(pool.clone()).await?;

        Ok(Self {
            config,
            rollup_contract,
            liquidity_contract,
            observer_api,
            graph_client,
            leader_election,
            rate_manager,
            pool,
        })
    }

    #[instrument(skip(self))]
    async fn fetch_and_write_deposit_leaf_inserted_events(
        &self,
        expected_next_event_id: u64,
    ) -> Result<u64, ObserverError> {
        let events = self
            .graph_client
            .get_deposit_leaf_inserted_events(expected_next_event_id as u32, EVENT_LIMIT)
            .await?;
        if events.is_empty() {
            return Ok(expected_next_event_id);
        }
        let first = events.first().unwrap();
        if first.deposit_index != expected_next_event_id as u32 {
            return Err(ObserverError::EventGapDetected {
                event_type: EventType::DepositLeafInserted,
                expected_next_event_id,
                got_event_id: first.deposit_index as u64,
            });
        }
        let mut tx = self.pool.begin().await?;
        for event in &events {
            sqlx::query!(
            "INSERT INTO deposit_leaf_events (deposit_index, deposit_hash, eth_block_number, eth_tx_index) 
            VALUES ($1, $2, $3, $4)",
            event.deposit_index as i32,
            event.deposit_hash.to_bytes_be(),
            event.eth_block_number as i64,
            event.eth_tx_index as i64
            )
            .execute(&mut *tx).await?;
        }
        tx.commit().await?;
        let next_event_id = events.last().unwrap().deposit_index as u64 + 1;
        Ok(next_event_id)
    }

    #[instrument(skip(self))]
    async fn fetch_and_write_deposited_events(
        &self,
        expected_next_event_id: u64,
    ) -> Result<u64, ObserverError> {
        let events = self
            .graph_client
            .get_deposited_events(expected_next_event_id, EVENT_LIMIT)
            .await?;
        if events.is_empty() {
            return Ok(expected_next_event_id);
        }
        let first = events.first().unwrap();
        if first.deposit_id != expected_next_event_id {
            return Err(ObserverError::EventGapDetected {
                event_type: EventType::Deposited,
                expected_next_event_id,
                got_event_id: first.deposit_id,
            });
        }
        let mut tx = self.pool.begin().await?;
        for event in &events {
            let deposit_hash = event.to_deposit().hash();
            sqlx::query!(
                "INSERT INTO deposited_events (deposit_id, depositor, pubkey_salt_hash, token_index, amount, is_eligible, deposited_at, deposit_hash, tx_hash, eth_block_number, eth_tx_index) 
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
                event.deposit_id as i64,
                event.depositor.to_hex(),
                event.pubkey_salt_hash.to_hex(),
                event.token_index as i64,
                event.amount.to_hex(),
                event.is_eligible,
                event.deposited_at as i64,
                deposit_hash.to_hex(),
                event.tx_hash.to_hex(),
                event.eth_block_number as i64,
                event.eth_tx_index as i64
            )
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        let next_event_id = events.last().unwrap().deposit_id + 1;
        Ok(next_event_id)
    }

    #[instrument(skip(self))]
    async fn fetch_and_write_block_posted_events(
        &self,
        expected_next_event_id: u64,
    ) -> Result<u64, ObserverError> {
        let events = self
            .graph_client
            .get_full_block_with_meta(expected_next_event_id as u32, EVENT_LIMIT)
            .await?;
        if events.is_empty() {
            return Ok(expected_next_event_id);
        }
        let first = events.first().unwrap();
        if first.full_block.block.block_number != expected_next_event_id as u32 {
            return Err(ObserverError::EventGapDetected {
                event_type: EventType::BlockPosted,
                expected_next_event_id,
                got_event_id: first.full_block.block.block_number as u64,
            });
        }
        let mut tx = self.pool.begin().await?;
        for event in &events {
            sqlx::query!(
                "INSERT INTO full_blocks (block_number, eth_block_number, eth_tx_index, full_block) 
                 VALUES ($1, $2, $3, $4)",
                event.full_block.block.block_number as i32,
                event.eth_block_number as i64,
                event.eth_tx_index as i64,
                bincode::serialize(&event.full_block).unwrap()
            )
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        let next_event_id = events.last().unwrap().full_block.block.block_number + 1;
        Ok(next_event_id as u64)
    }
}

#[async_trait::async_trait(?Send)]
impl SyncEvent for TheGraphObserver {
    fn name(&self) -> String {
        "TheGraphObserver".to_string()
    }

    fn config(&self) -> ObserverConfig {
        self.config.clone()
    }

    fn rate_manager(&self) -> &RateManager {
        &self.rate_manager
    }

    #[instrument(skip(self))]
    async fn sync_events(&self, event_type: EventType) -> Result<(), ObserverError> {
        // determine whether to sync or not
        let mut local_next_event_id = self
            .observer_api
            .get_local_next_event_id(event_type)
            .await?;
        let onchain_next_event_id = self
            .observer_api
            .get_onchain_next_event_id(event_type)
            .await?;
        if local_next_event_id >= onchain_next_event_id {
            debug!(
                "No new events to sync. Local: {}, Onchain: {}",
                local_next_event_id, onchain_next_event_id
            );
            return Ok(());
        }
        info!(
            "Syncing events. Local next event id: {}, Onchain next event id: {}",
            local_next_event_id, onchain_next_event_id
        );
        // continue to sync until local_next_event_id >= onchain_next_event_id with max_query_times
        for _ in 0..self.config.observer_max_query_times {
            self.leader_election.wait_for_leadership().await?;
            local_next_event_id = match event_type {
                EventType::DepositLeafInserted => {
                    self.fetch_and_write_deposit_leaf_inserted_events(local_next_event_id)
                        .await?
                }
                EventType::Deposited => {
                    self.fetch_and_write_deposited_events(local_next_event_id)
                        .await?
                }
                EventType::BlockPosted => {
                    self.fetch_and_write_block_posted_events(local_next_event_id)
                        .await?
                }
            };
            if local_next_event_id >= onchain_next_event_id {
                break;
            }
        }
        info!(
            "Synced events. Local next event id: {}, Onchain next event id: {}",
            local_next_event_id, onchain_next_event_id
        );
        Ok(())
    }
}
