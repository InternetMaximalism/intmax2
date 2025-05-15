use super::{
    check_point_store::{ChainType, CheckPointStore, EventType},
    error::ObserverError,
    leader_election::LeaderElection,
    observer_api::ObserverApi,
    observer_common::{ObserverConfig, SyncEvent},
    rate_manager::RateManager,
};
use crate::{app::observer_common::initialize_observer_db, EnvVar};
use alloy::providers::Provider;
use intmax2_client_sdk::external_api::contract::{
    liquidity_contract::LiquidityContract, rollup_contract::RollupContract,
};
use intmax2_zkp::{
    ethereum_types::u32limb_trait::U32LimbTrait as _, utils::leafable::Leafable as _,
};
use log::warn;
use server_common::db::{DbPool, DbPoolConfig};
use tracing::{debug, info, instrument};

#[derive(Clone)]
pub struct RPCObserver {
    pub config: ObserverConfig,
    pub rollup_contract: RollupContract,
    pub liquidity_contract: LiquidityContract,
    pub observer_api: ObserverApi,
    pub check_point_store: CheckPointStore,
    pub leader_election: LeaderElection,
    pub rate_manager: RateManager,
    pub pool: DbPool,
}

impl RPCObserver {
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
        let check_point_store = CheckPointStore::new(pool.clone());
        initialize_observer_db(pool.clone()).await?;

        Ok(RPCObserver {
            config,
            rollup_contract: observer_api.rollup_contract.clone(),
            liquidity_contract: observer_api.liquidity_contract.clone(),
            observer_api,
            check_point_store,
            leader_election,
            rate_manager,
            pool,
        })
    }

    fn default_eth_block_number(&self, event_type: EventType) -> u64 {
        match event_type.to_chain_type() {
            ChainType::L1 => self.config.liquidity_contract_deployed_block_number,
            ChainType::L2 => self.config.rollup_contract_deployed_block_number,
        }
    }

    async fn get_current_eth_block_number(
        &self,
        event_type: EventType,
    ) -> Result<u64, ObserverError> {
        let current_eth_block_number = match event_type.to_chain_type() {
            ChainType::L1 => self.liquidity_contract.provider.get_block_number().await?,
            ChainType::L2 => self.rollup_contract.provider.get_block_number().await?,
        };
        Ok(current_eth_block_number)
    }

    #[instrument(skip(self))]
    async fn fetch_and_write_deposit_leaf_inserted_events(
        &self,
        expected_next_event_id: u64,
        from_eth_block_number: u64,
        to_eth_block_number: u64,
    ) -> Result<u64, ObserverError> {
        let events = self
            .rollup_contract
            .get_deposit_leaf_inserted_events(from_eth_block_number, to_eth_block_number)
            .await
            .map_err(|e| ObserverError::EventFetchError(e.to_string()))?;
        let events = events
            .into_iter()
            .skip_while(|e| e.deposit_index < expected_next_event_id as u32)
            .collect::<Vec<_>>();
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
        from_eth_block_number: u64,
        to_eth_block_number: u64,
    ) -> Result<u64, ObserverError> {
        let events = self
            .liquidity_contract
            .get_deposited_events(from_eth_block_number, to_eth_block_number)
            .await
            .map_err(|e| ObserverError::EventFetchError(e.to_string()))?;
        let events = events
            .into_iter()
            .skip_while(|e| e.deposit_id < expected_next_event_id)
            .collect::<Vec<_>>();
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
        from_eth_block_number: u64,
        to_eth_block_number: u64,
    ) -> Result<u64, ObserverError> {
        let events = self
            .rollup_contract
            .get_blocks_posted_event(from_eth_block_number, to_eth_block_number)
            .await
            .map_err(|e| ObserverError::EventFetchError(e.to_string()))?;
        let events = events
            .into_iter()
            .skip_while(|b| b.block_number < expected_next_event_id as u32)
            .collect::<Vec<_>>();
        if events.is_empty() {
            return Ok(expected_next_event_id);
        }
        let first = events.first().unwrap();
        if first.block_number != expected_next_event_id as u32 {
            return Err(ObserverError::EventGapDetected {
                event_type: EventType::BlockPosted,
                expected_next_event_id,
                got_event_id: first.block_number as u64,
            });
        }
        // fetch full block
        let full_block_with_meta = self
            .rollup_contract
            .get_full_block_with_meta(&events)
            .await?;
        let mut tx = self.pool.begin().await?;
        for event in &full_block_with_meta {
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
        let next_event_id = events.last().unwrap().block_number + 1;
        Ok(next_event_id as u64)
    }

    #[instrument(skip(self))]
    async fn reset_check_point(
        &self,
        event_type: EventType,
        local_last_eth_block_number: Option<u64>,
        reason: &str,
    ) -> Result<(), ObserverError> {
        let reset_eth_block_number =
            local_last_eth_block_number.unwrap_or(self.default_eth_block_number(event_type));
        warn!(
            "Reset checkpoint. Event type: {}, Local last eth block number: {:?}, Reset eth block number: {}, Reason: {}",
            event_type, local_last_eth_block_number, reset_eth_block_number, reason
        );
        self.check_point_store
            .set_check_point(event_type, reset_eth_block_number)
            .await?;
        Ok(())
    }

    #[instrument(skip(self))]
    async fn sync_and_save_checkpoint(
        &self,
        event_type: EventType,
        onchain_next_event_id: u64,
        local_next_event_id: u64,
    ) -> Result<u64, ObserverError> {
        self.leader_election.wait_for_leadership().await?;
        let checkpoint_eth_block_number =
            self.check_point_store.get_check_point(event_type).await?;
        let local_last_eth_block_number = self
            .observer_api
            .get_local_last_eth_block_number(event_type)
            .await?;
        let from_eth_block_number = checkpoint_eth_block_number
            .max(local_last_eth_block_number)
            .unwrap_or(self.default_eth_block_number(event_type));
        tracing::info!(
            "checkpoint eth block number: {:?}, local last eth block number: {:?}, from eth block number: {:?}",
            checkpoint_eth_block_number,
            local_last_eth_block_number,
            from_eth_block_number
        );
        let current_eth_block_number = self.get_current_eth_block_number(event_type).await?;
        if from_eth_block_number > current_eth_block_number {
            // This should never happen unless checkpoint is corrupted, so we need to reset the checkpoint
            let reason = format!(
                "from_eth_block_number : {} > current_eth_block_number: {}",
                from_eth_block_number, current_eth_block_number
            );
            self.reset_check_point(event_type, local_last_eth_block_number, &reason)
                .await?;
            return Ok(local_next_event_id);
        }
        let to_eth_block_number = current_eth_block_number
            .min(from_eth_block_number + self.config.observer_event_block_interval - 1);
        // This is asserted because we already checked that from_eth_block_number <= current_eth_block_number
        assert!(
            to_eth_block_number >= from_eth_block_number,
            "to_eth_block_number should be greater than or equal to from_eth_block_number"
        );
        let next_event_id = match event_type {
            EventType::DepositLeafInserted => {
                self.fetch_and_write_deposit_leaf_inserted_events(
                    local_next_event_id,
                    from_eth_block_number,
                    to_eth_block_number,
                )
                .await
            }
            EventType::Deposited => {
                self.fetch_and_write_deposited_events(
                    local_next_event_id,
                    from_eth_block_number,
                    to_eth_block_number,
                )
                .await
            }
            EventType::BlockPosted => {
                self.fetch_and_write_block_posted_events(
                    local_next_event_id,
                    from_eth_block_number,
                    to_eth_block_number,
                )
                .await
            }
        };
        match next_event_id {
            Ok(next_event_id) => {
                if to_eth_block_number == current_eth_block_number
                    && next_event_id == local_next_event_id
                    && onchain_next_event_id > local_next_event_id
                {
                    // This means we have synced all events but the onchain event is not synced yet
                    let reason = format!(
                        "Sync all events but onchain event is not synced yet. Local next event id: {}, Onchain next event id: {}, From eth block number: {}, To eth block number: {}",
                        local_next_event_id,
                        onchain_next_event_id,
                        from_eth_block_number,
                        to_eth_block_number
                    );
                    self.reset_check_point(event_type, local_last_eth_block_number, &reason)
                        .await?;
                    return Ok(next_event_id);
                }
                info!(
                    "Sync success. Local next event id: {}, synced next event id: {}, From eth block number: {}, To eth block number: {}",
                    local_next_event_id, next_event_id, from_eth_block_number, to_eth_block_number
                    );
                self.check_point_store
                    .set_check_point(event_type, to_eth_block_number)
                    .await?;
                Ok(next_event_id)
            }
            Err(ObserverError::EventGapDetected {
                event_type: _event_type,
                expected_next_event_id,
                got_event_id,
            }) => {
                assert_eq!(event_type, _event_type, "Event type mismatch");
                if checkpoint_eth_block_number.is_none() {
                    // This never happens except for RPC issues
                    let reason = format!(
                        "Checkpoint eth block number is None But event gap detected. Expected next event id: {}, Got event id: {}, From eth block number: {}, To eth block number: {}",
                        expected_next_event_id,
                        got_event_id,
                        from_eth_block_number,
                        to_eth_block_number
                    );
                    self.reset_check_point(event_type, local_last_eth_block_number, &reason)
                        .await?;
                    return Ok(local_next_event_id);
                }
                // If event gap detected, we need to reset the checkpoint
                let reason = format!(
                    "Event gap detected. Expected next event id: {}, Got event id: {}, From eth block number: {}, To eth block number: {}",
                    expected_next_event_id,
                    got_event_id,
                    from_eth_block_number,
                    to_eth_block_number
                );
                self.reset_check_point(event_type, local_last_eth_block_number, &reason)
                    .await?;
                Ok(local_next_event_id)
            }
            Err(e) => {
                // Return other errors as is. Handle them in the upper function with other errors
                return Err(e);
            }
        }
    }
}

#[async_trait::async_trait(?Send)]
impl SyncEvent for RPCObserver {
    fn name(&self) -> String {
        "RPCObserver".to_string()
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
            local_next_event_id = self
                .sync_and_save_checkpoint(event_type, onchain_next_event_id, local_next_event_id)
                .await?;
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
