use std::sync::Arc;

use ethers::types::Address;
use intmax2_client_sdk::external_api::contract::{
    liquidity_contract::LiquidityContract,
    rollup_contract::{FullBlockWithMeta, RollupContract},
    utils::get_latest_block_number,
};
use intmax2_zkp::{
    common::witness::full_block::FullBlock, ethereum_types::u32limb_trait::U32LimbTrait as _,
    utils::leafable::Leafable as _,
};

use log::warn;
use server_common::db::DbPool;
use tracing::{info, instrument};

use super::{
    check_point_store::{ChainType, CheckPointStore, EventType},
    error::ObserverError,
};

#[derive(Debug, Clone)]
pub struct ObserverConfig {
    pub event_block_interval: u64,
    pub backward_sync_block_number: u64,
    pub sync_interval: u64,

    // chain config
    pub l1_rpc_url: String,
    pub l1_chain_id: u64,
    pub l2_rpc_url: String,
    pub l2_chain_id: u64,
    pub rollup_contract_address: Address,
    pub rollup_contract_deployed_block_number: u64,
    pub liquidity_contract_address: Address,
    pub liquidity_contract_deployed_block_number: u64,
}

#[derive(Clone)]
pub struct Observer {
    config: ObserverConfig,
    rollup_contract: RollupContract,
    liquidity_contract: LiquidityContract,
    check_point_store: CheckPointStore,
    pub(crate) pool: DbPool,
}

impl Observer {
    pub async fn new(config: ObserverConfig, pool: DbPool) -> Result<Self, ObserverError> {
        let check_point_store = CheckPointStore::new(pool.clone());
        let rollup_contract = RollupContract::new(
            &config.l2_rpc_url,
            config.l2_chain_id,
            config.rollup_contract_address,
        );
        let liquidity_contract = LiquidityContract::new(
            &config.l1_rpc_url,
            config.l1_chain_id,
            config.liquidity_contract_address,
        );
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
            pool,
        })
    }

    async fn get_local_next_event_id(&self, event_type: EventType) -> Result<u64, ObserverError> {
        let latest_event_id = match event_type {
            EventType::Deposited => self.get_local_last_deposit_id().await?,
            EventType::DepositLeafInserted => {
                self.get_local_last_deposit_index().await?.map(|i| i as u64)
            }
            EventType::BlockPosted => self.get_local_last_block_number().await?.map(|i| i as u64),
        };
        Ok(latest_event_id.map_or(0, |i| i + 1))
    }

    async fn get_local_last_eth_block_number(
        &self,
        event_type: EventType,
    ) -> Result<Option<u64>, ObserverError> {
        let last_eth_block_number = match event_type {
            EventType::Deposited => {
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
        Ok(last_eth_block_number.map(|i| i as u64))
    }

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

    fn default_eth_block_number(&self, event_type: EventType) -> u64 {
        match event_type.to_chain_type() {
            ChainType::L1 => self.config.l1_chain_id,
            ChainType::L2 => self.config.l2_chain_id,
        }
    }

    async fn get_current_eth_block_number(
        &self,
        event_type: EventType,
    ) -> Result<u64, ObserverError> {
        let current_eth_block_number = match event_type.to_chain_type() {
            ChainType::L1 => get_latest_block_number(&self.config.l1_rpc_url).await?,
            ChainType::L2 => get_latest_block_number(&self.config.l2_rpc_url).await?,
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
                event_type: EventType::DepositLeafInserted,
                expected_next_event_id,
                got_event_id: first.deposit_id as u64,
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
    async fn sync_and_save_checkpoint(
        &self,
        event_type: EventType,
        local_next_event_id: u64,
    ) -> Result<u64, ObserverError> {
        // どこからblockを同期したらいいのか？-> まずはcheckpointとlocalの最新eth_block_numberを取得
        let checkpoint_eth_block_number =
            self.check_point_store.get_check_point(event_type).await?;
        let local_last_eth_block_number = self.get_local_last_eth_block_number(event_type).await?;
        // 大きい方を取る。両方がNoneの場合はdefault_eth_block_numberを使う
        let from_eth_block_number = checkpoint_eth_block_number
            .max(local_last_eth_block_number)
            .unwrap_or(self.default_eth_block_number(event_type));

        // どこまでblockを同期したらいいのか？-> onchainの最新eth_block_numberを取得
        let to_eth_block_number = self
            .get_current_eth_block_number(event_type)
            .await?
            .min(from_eth_block_number + self.config.event_block_interval - 1);

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
                // checkpointを更新する
                self.check_point_store
                    .set_check_point(event_type, to_eth_block_number)
                    .await?;
                info!(
         "Sync success. Event type: {}, Local next event id: {}, Onchain next event id: {}, From eth block number: {}, To eth block number: {}",
         event_type, local_next_event_id, next_event_id, from_eth_block_number, to_eth_block_number
     );
                Ok(next_event_id)
            }
            Err(ObserverError::EventGapDetected {
                event_type,
                expected_next_event_id,
                got_event_id,
            }) => {
                let backward_eth_block_number = from_eth_block_number
                    .saturating_sub(self.config.backward_sync_block_number)
                    .max(self.default_eth_block_number(event_type));
                self.check_point_store
                    .set_check_point(event_type, backward_eth_block_number)
                    .await?;
                warn!(
         "Event gap detected. Event type: {}, Expected next event id: {}, Got event id: {}. Backward to {}",
         event_type, expected_next_event_id, got_event_id, backward_eth_block_number
     );
                //next_event_idは更新しない
                Ok(local_next_event_id)
            }
            Err(e) => {
                // それ以外のエラーはそのまま返す. 他のエラーと一緒に上位の関数で処理する
                return Err(e);
            }
        }
    }

    #[instrument(skip(self))]
    async fn sync_events(&self, event_type: EventType) -> Result<(), ObserverError> {
        // syncするべきかどうかの判定をする
        let mut local_next_event_id = self.get_local_next_event_id(event_type).await?;
        let onchain_next_event_id = self.get_onchain_next_event_id(event_type).await?;
        if local_next_event_id >= onchain_next_event_id {
            info!(
                "No new events to sync. Local: {}, Onchain: {}",
                local_next_event_id, onchain_next_event_id
            );
            return Ok(());
        }
        // localがonchainに追いつくまでsyncをつづける
        loop {
            local_next_event_id = self
                .sync_and_save_checkpoint(event_type, local_next_event_id)
                .await?;
            // もしlocalがonchainに追いついたらbreakする
            if local_next_event_id >= onchain_next_event_id {
                break;
            }
        }
        info!(
            "Synced: Local next event id: {}, Onchain next event id: {}",
            local_next_event_id, onchain_next_event_id
        );
        Ok(())
    }

    #[instrument(skip(self))]
    async fn sync_events_inner_loop(&self, event_type: EventType) -> Result<(), ObserverError> {
        let mut interval =
            tokio::time::interval(tokio::time::Duration::from_secs(self.config.sync_interval));
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
                tokio::spawn(async move { this.sync_events_inner_loop(event_type).await });

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
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
            log::info!("Restarting sync events job for {}", event_type);
        }
    }

    #[instrument(skip(self))]
    pub fn start_all_jobs(&self) {
        let event_types = vec![
            EventType::Deposited,
            EventType::DepositLeafInserted,
            EventType::BlockPosted,
        ];
        let this = Arc::new(self.clone());
        for event_type in event_types {
            let this = this.clone();
            tokio::spawn(async move {
                this.sync_events_job(event_type).await;
            });
        }
        log::info!("Observer started all jobs");
    }
}
