use super::{
    check_point_store::{ChainType, CheckPointStore, EventType},
    error::ObserverError,
    leader_election::LeaderElection,
};
use crate::EnvVar;
use alloy::providers::Provider;
use intmax2_client_sdk::external_api::contract::{
    liquidity_contract::LiquidityContract,
    rollup_contract::{DepositLeafInserted, FullBlockWithMeta, RollupContract},
    utils::NormalProvider,
};
use intmax2_zkp::{
    common::witness::full_block::FullBlock,
    ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait as _},
    utils::leafable::Leafable as _,
};
use log::warn;
use server_common::db::{DbPool, DbPoolConfig};
use std::sync::Arc;
use tracing::{debug, info, instrument};

#[derive(Debug, Clone)]
pub struct ObserverConfig {
    pub observer_event_block_interval: u64,
    pub observer_max_query_times: usize,
    pub observer_sync_interval: u64,
    pub observer_restart_interval: u64,

    pub rollup_contract_deployed_block_number: u64,
    pub liquidity_contract_deployed_block_number: u64,
}

#[derive(Clone)]
pub struct Observer {
    pub(crate) config: ObserverConfig,
    pub(crate) rollup_contract: RollupContract,
    pub(crate) liquidity_contract: LiquidityContract,
    pub(crate) check_point_store: CheckPointStore,
    pub(crate) leader_election: LeaderElection,
    pub(crate) pool: DbPool,
}

impl Observer {
    pub async fn new(
        env: &EnvVar,
        l1_provider: NormalProvider,
        l2_provider: NormalProvider,
    ) -> Result<Self, ObserverError> {
        let config = ObserverConfig {
            observer_event_block_interval: env.observer_event_block_interval,
            observer_max_query_times: env.observer_max_query_times,
            observer_sync_interval: env.observer_sync_interval,
            observer_restart_interval: env.observer_restart_interval,
            rollup_contract_deployed_block_number: env.rollup_contract_deployed_block_number,
            liquidity_contract_deployed_block_number: env.liquidity_contract_deployed_block_number,
        };
        tracing::info!("Observer config: {:?}", config);
        let pool = DbPool::from_config(&DbPoolConfig {
            max_connections: env.database_max_connections,
            idle_timeout: env.database_timeout,
            url: env.database_url.to_string(),
        })
        .await?;
        let check_point_store = CheckPointStore::new(pool.clone());
        let leader_election = LeaderElection::new(
            &env.redis_url,
            "validity_prover:sync_leader",
            std::time::Duration::from_secs(env.leader_lock_ttl),
        )?;
        let rollup_contract = RollupContract::new(l2_provider, env.rollup_contract_address);
        let liquidity_contract =
            LiquidityContract::new(l1_provider, env.liquidity_contract_address);
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
        Ok(Observer {
            config,
            rollup_contract,
            liquidity_contract,
            check_point_store,
            leader_election,
            pool,
        })
    }

    #[instrument(skip(self))]
    async fn get_local_next_event_id(&self, event_type: EventType) -> Result<u64, ObserverError> {
        let next_event_id = match event_type {
            EventType::Deposited => self.get_local_last_deposit_id().await? + 1,
            EventType::DepositLeafInserted => self
                .get_local_last_deposit_index()
                .await?
                .map(|i| i as u64 + 1)
                .unwrap_or(0),
            EventType::BlockPosted => self.get_local_last_block_number().await? as u64 + 1,
        };
        Ok(next_event_id)
    }

    #[instrument(skip(self))]
    async fn get_local_last_eth_block_number(
        &self,
        event_type: EventType,
    ) -> Result<Option<u64>, ObserverError> {
        let last_eth_block_number = match event_type {
            EventType::Deposited => {
                sqlx::query_scalar!(
                    r#"
                    SELECT eth_block_number
                    FROM deposited_events
                    WHERE deposit_id = (SELECT MAX(deposit_id) FROM deposited_events)
                    "#
                )
                .fetch_optional(&self.pool)
                .await?
            }
            EventType::DepositLeafInserted => {
                sqlx::query_scalar!(
                    r#"
                    SELECT eth_block_number
                    FROM deposit_leaf_events
                    WHERE deposit_index = (SELECT MAX(deposit_index) FROM deposit_leaf_events)
                    "#
                )
                .fetch_optional(&self.pool)
                .await?
            }
            EventType::BlockPosted => {
                sqlx::query_scalar!(
                    r#"
                    SELECT eth_block_number
                    FROM full_blocks
                    WHERE block_number = (SELECT MAX(block_number) FROM full_blocks)
                    "#
                )
                .fetch_optional(&self.pool)
                .await?
            }
        };
        // This is a special case for genesis block
        if last_eth_block_number == Some(0) {
            return Ok(None);
        }
        Ok(last_eth_block_number.map(|i| i as u64))
    }

    #[instrument(skip(self))]
    async fn get_onchain_next_event_id(&self, event_type: EventType) -> Result<u64, ObserverError> {
        let next_event_id = match event_type {
            EventType::Deposited => self.liquidity_contract.get_last_deposit_id().await? + 1,
            EventType::DepositLeafInserted => {
                self.rollup_contract.get_next_deposit_index().await? as u64
            }
            EventType::BlockPosted => {
                self.rollup_contract.get_latest_block_number().await? as u64 + 1
            }
        };
        Ok(next_event_id)
    }

    #[instrument(skip(self))]
    async fn is_synced(&self, event_type: EventType) -> Result<bool, ObserverError> {
        let local_next_event_id = self.get_local_next_event_id(event_type).await?;
        let onchain_next_event_id = self.get_onchain_next_event_id(event_type).await?;
        Ok(local_next_event_id >= onchain_next_event_id)
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

    // Util function to get deposit_leaf_inserted events between the specified block and the previous block
    // This is used to generate validity witness for the block
    #[instrument(skip(self))]
    pub async fn get_deposits_between_blocks(
        &self,
        block_number: u32,
    ) -> Result<Option<Vec<DepositLeafInserted>>, ObserverError> {
        if block_number == 0 {
            return Ok(Some(Vec::new()));
        }
        let prev_block_number = block_number - 1;
        let local_last_block_number = self.get_local_last_block_number().await?;
        if block_number > local_last_block_number {
            // blocks are not ready
            return Ok(None);
        }
        let current_block = self
            .get_full_block_with_meta(block_number)
            .await?
            .ok_or(ObserverError::BlockNotFound(block_number))?;
        let prev_block = self
            .get_full_block_with_meta(prev_block_number)
            .await?
            .ok_or(ObserverError::BlockNotFound(prev_block_number))?;
        let local_last_eth_block_number = self
            .get_local_last_eth_block_number(EventType::DepositLeafInserted)
            .await?;
        if local_last_eth_block_number.is_none() {
            let is_synced = self.is_synced(EventType::DepositLeafInserted).await?;
            if !is_synced {
                return Ok(None);
            }
        }
        let local_last_eth_block_number = local_last_eth_block_number.unwrap();
        if local_last_eth_block_number < current_block.eth_block_number {
            let is_synced = self.is_synced(EventType::DepositLeafInserted).await?;
            if !is_synced {
                return Ok(None);
            }
        }
        let deposits = sqlx::query!(
            r#"
            SELECT deposit_index, deposit_hash, eth_block_number, eth_tx_index
            FROM deposit_leaf_events
            WHERE (eth_block_number, eth_tx_index) > ($1, $2)
            AND (eth_block_number, eth_tx_index) <= ($3, $4)
            ORDER BY deposit_index
            "#,
            prev_block.eth_block_number as i64,
            prev_block.eth_tx_index as i64,
            current_block.eth_block_number as i64,
            current_block.eth_tx_index as i64,
        )
        .fetch_all(&self.pool)
        .await?;
        let events = deposits
            .into_iter()
            .map(|d| DepositLeafInserted {
                deposit_index: d.deposit_index as u32,
                deposit_hash: Bytes32::from_bytes_be(&d.deposit_hash).unwrap(),
                eth_block_number: d.eth_block_number as u64,
                eth_tx_index: d.eth_tx_index as u64,
            })
            .collect();
        Ok(Some(events))
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
        let local_last_eth_block_number = self.get_local_last_eth_block_number(event_type).await?;
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

    #[instrument(skip(self))]
    async fn sync_events(&self, event_type: EventType) -> Result<(), ObserverError> {
        // determine whether to sync or not
        let mut local_next_event_id = self.get_local_next_event_id(event_type).await?;
        let onchain_next_event_id = self.get_onchain_next_event_id(event_type).await?;
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

    #[instrument(skip(self))]
    async fn sync_events_inner_loop(&self, event_type: EventType) -> Result<(), ObserverError> {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(
            self.config.observer_sync_interval,
        ));
        loop {
            interval.tick().await;
            self.sync_events(event_type).await?;
        }
    }

    #[instrument(skip(self))]
    async fn sync_events_job(&self, event_type: EventType) {
        let this = Arc::new(self.clone());
        // auto restart loop
        loop {
            let this = this.clone();
            let handler =
                actix_web::rt::spawn(async move { this.sync_events_inner_loop(event_type).await });

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
            tokio::time::sleep(tokio::time::Duration::from_secs(
                self.config.observer_restart_interval,
            ))
            .await;
            log::info!("Restarting sync events job for {}", event_type);
        }
    }

    #[instrument(skip(self))]
    pub fn start_all_jobs(&self) {
        self.leader_election.start_job();

        let event_types = vec![
            EventType::Deposited,
            EventType::DepositLeafInserted,
            EventType::BlockPosted,
        ];
        let this = Arc::new(self.clone());
        for event_type in event_types {
            let this = this.clone();
            actix_web::rt::spawn(async move {
                this.sync_events_job(event_type).await;
            });
        }
        log::info!("Observer started all jobs");
    }
}
