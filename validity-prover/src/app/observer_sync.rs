use ethers::types::Address;
use intmax2_client_sdk::external_api::contract::{
    liquidity_contract::LiquidityContract,
    rollup_contract::{FullBlockWithMeta, RollupContract},
    utils::get_latest_block_number,
};
use intmax2_zkp::{
    common::witness::full_block::FullBlock, ethereum_types::u32limb_trait::U32LimbTrait as _,
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
    pub max_tries: u32,
    pub sleep_time: u64,

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

    pub async fn get_local_next_event_id(
        &self,
        event_type: EventType,
    ) -> Result<u64, ObserverError> {
        let latest_event_id = match event_type {
            EventType::Deposited => self.get_local_last_deposit_id().await?.map(|i| i as u64),
            EventType::DepositLeafInserted => {
                self.get_local_last_deposit_index().await?.map(|i| i as u64)
            }
            EventType::BlockPosted => self.get_local_last_block_number().await?.map(|i| i as u64),
        };
        Ok(latest_event_id.map_or(0, |i| i + 1))
    }

    pub async fn get_local_last_eth_block_number(
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
            _ => {
                todo!()
            }
        };
        Ok(last_eth_block_number.map(|i| i as u64))
    }

    pub async fn get_onchain_next_event_id(
        &self,
        event_type: EventType,
    ) -> Result<u64, ObserverError> {
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

    pub fn default_eth_block_number(&self, event_type: EventType) -> u64 {
        match event_type.to_chain_type() {
            ChainType::L1 => self.config.l1_chain_id,
            ChainType::L2 => self.config.l2_chain_id,
        }
    }

    pub async fn get_current_eth_block_number(
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
    pub async fn fetch_and_write_deposit_leaf_inserted_event(
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
    pub async fn single_sync_deposit_leaf_inserted(&self) -> Result<(), ObserverError> {
        let event_type = EventType::DepositLeafInserted;

        // syncするべきかどうかの判定をする
        let local_next_event_id = self.get_local_next_event_id(event_type).await?;
        let onchain_next_event_id = self.get_onchain_next_event_id(event_type).await?;
        if local_next_event_id >= onchain_next_event_id {
            info!(
                "No new events to sync. Local: {}, Onchain: {}",
                local_next_event_id, onchain_next_event_id
            );
            return Ok(());
        }

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
                self.fetch_and_write_deposit_leaf_inserted_event(
                    local_next_event_id,
                    from_eth_block_number,
                    to_eth_block_number,
                )
                .await
            }
            _ => {
                todo!()
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
            }
            Err(e) => {
                // それ以外のエラーはそのまま返す. 他のエラーと一緒に上位の関数で処理する
                return Err(e);
            }
        }

        todo!();
    }

    // async fn try_sync_deposits(&self) -> Result<(Vec<DepositLeafInserted>, u64), ObserverError> {
    //     let deposit_sync_eth_block_number = self.get_deposit_sync_eth_block_number().await?;
    //     let (deposit_leaf_events, to_block) = self
    //         .rollup_contract
    //         .get_deposit_leaf_inserted_events(deposit_sync_eth_block_number)
    //         .await
    //         .map_err(|e| ObserverError::FullBlockSyncError(e.to_string()))?;
    //     let next_deposit_index = self.get_next_deposit_index().await?;

    //     // skip already synced events
    //     let deposit_leaf_events = deposit_leaf_events
    //         .into_iter()
    //         .skip_while(|e| e.deposit_index < next_deposit_index)
    //         .collect::<Vec<_>>();
    //     if let Some(first) = deposit_leaf_events.first() {
    //         if first.deposit_index != next_deposit_index {
    //             return Err(ObserverError::FullBlockSyncError(format!(
    //                 "First deposit index mismatch: {} != {}",
    //                 first.deposit_index, next_deposit_index
    //             )));
    //         }
    //     } else {
    //         // no new deposits
    //         let rollup_next_deposit_index = self.rollup_contract.get_next_deposit_index().await?;
    //         if next_deposit_index < rollup_next_deposit_index {
    //             return Err(ObserverError::FullBlockSyncError(format!(
    //                 "next_deposit_index is less than rollup_next_deposit_index: {} < {}",
    //                 next_deposit_index, rollup_next_deposit_index
    //             )));
    //         }
    //     }
    //     Ok((deposit_leaf_events, to_block))
    // }

    // async fn sync_deposits(&self) -> Result<(), ObserverError> {
    //     let mut tries = 0;
    //     loop {
    //         if tries >= MAX_TRIES {
    //             return Err(ObserverError::FullBlockSyncError(
    //                 "Max tries exceeded".to_string(),
    //             ));
    //         }

    //         match self.try_sync_deposits().await {
    //             Ok((deposit_leaf_events, to_block)) => {
    //                 let mut tx = self.pool.begin().await?;
    //                 for event in &deposit_leaf_events {
    //                     sqlx::query!(
    //                         "INSERT INTO deposit_leaf_events (deposit_index, deposit_hash, eth_block_number, eth_tx_index)
    //                          VALUES ($1, $2, $3, $4)",
    //                         event.deposit_index as i32,
    //                         event.deposit_hash.to_bytes_be(),
    //                         event.eth_block_number as i64,
    //                         event.eth_tx_index as i64
    //                     )
    //                     .execute(&mut *tx)
    //                     .await?;
    //                 }
    //                 self.set_deposit_sync_eth_block_number(&mut tx, to_block + 1)
    //                     .await?;
    //                 tx.commit().await?;

    //                 let next_deposit_index = self.get_next_deposit_index().await?;
    //                 log::info!(
    //                     "synced to deposit_index: {}, to_eth_block_number: {}",
    //                     next_deposit_index.saturating_sub(1),
    //                     to_block
    //                 );
    //                 return Ok(());
    //             }
    //             Err(e) => {
    //                 if matches!(e, ObserverError::FullBlockSyncError(_)) {
    //                     log::error!("Observer sync error: {:?}", e);
    //                     // rollback to previous block number
    //                     let block_number = self
    //                         .get_deposit_sync_eth_block_number()
    //                         .await?
    //                         .saturating_sub(BACKWARD_SYNC_BLOCK_NUMBER);
    //                     let mut tx = self.pool.begin().await?;
    //                     self.set_deposit_sync_eth_block_number(&mut tx, block_number)
    //                         .await?;
    //                     tx.commit().await?;
    //                     sleep_for(SLEEP_TIME).await;
    //                     tries += 1;
    //                 } else {
    //                     return Err(e);
    //                 }
    //             }
    //         }
    //     }
    // }

    // async fn try_sync_block(&self) -> Result<(Vec<FullBlockWithMeta>, u64), ObserverError> {
    //     let block_sync_eth_block_number = self.get_block_sync_eth_block_number().await?;
    //     let (full_blocks, to_block) = self
    //         .rollup_contract
    //         .get_full_block_with_meta(block_sync_eth_block_number)
    //         .await
    //         .map_err(|e| ObserverError::FullBlockSyncError(e.to_string()))?;
    //     let next_block_number = self.get_next_block_number().await?;
    //     // skip already synced events
    //     let full_blocks = full_blocks
    //         .into_iter()
    //         .skip_while(|b| b.full_block.block.block_number < next_block_number)
    //         .collect::<Vec<_>>();
    //     if let Some(first) = full_blocks.first() {
    //         if first.full_block.block.block_number != next_block_number {
    //             return Err(ObserverError::FullBlockSyncError(format!(
    //                 "First block mismatch: {} != {}",
    //                 first.full_block.block.block_number, next_block_number
    //             )));
    //         }
    //     } else {
    //         // no new blocks
    //         let rollup_block_number = self.rollup_contract.get_latest_block_number().await?;
    //         if next_block_number <= rollup_block_number {
    //             return Err(ObserverError::FullBlockSyncError(format!(
    //                 "next_block_number is less than rollup_block_number: {} <= {}",
    //                 next_block_number, rollup_block_number
    //             )));
    //         }
    //     }
    //     Ok((full_blocks, to_block))
    // }

    // async fn sync_blocks(&self) -> Result<(), ObserverError> {
    //     let mut tries = 0;
    //     loop {
    //         if tries >= MAX_TRIES {
    //             return Err(ObserverError::FullBlockSyncError(
    //                 "Max tries exceeded".to_string(),
    //             ));
    //         }
    //         match self.try_sync_block().await {
    //             Ok((full_blocks, to_block)) => {
    //                 let mut tx = self.pool.begin().await?;
    //                 for block in &full_blocks {
    //                     sqlx::query!(
    //                         "INSERT INTO full_blocks (block_number, eth_block_number, eth_tx_index, full_block)
    //                          VALUES ($1, $2, $3, $4)",
    //                         block.full_block.block.block_number as i32,
    //                         block.eth_block_number as i64,
    //                         block.eth_tx_index as i64,
    //                         bincode::serialize(&block.full_block).unwrap()
    //                     )
    //                     .execute(&mut *tx)
    //                     .await?;
    //                 }
    //                 self.set_block_sync_eth_block_number(&mut tx, to_block + 1)
    //                     .await?;
    //                 tx.commit().await?;

    //                 let next_block_number = self.get_next_block_number().await?;
    //                 log::info!(
    //                     "synced to block_number: {}, to_eth_block_number: {}",
    //                     next_block_number.saturating_sub(1),
    //                     to_block
    //                 );
    //                 return Ok(());
    //             }
    //             Err(e) => {
    //                 if matches!(e, ObserverError::FullBlockSyncError(_)) {
    //                     log::error!("Observer sync error: {:?}", e);

    //                     // rollback to previous block number
    //                     let block_number = self
    //                         .get_block_sync_eth_block_number()
    //                         .await?
    //                         .saturating_sub(BACKWARD_SYNC_BLOCK_NUMBER);
    //                     let mut tx = self.pool.begin().await?;
    //                     self.set_block_sync_eth_block_number(&mut tx, block_number)
    //                         .await?;
    //                     tx.commit().await?;
    //                     sleep_for(SLEEP_TIME).await;
    //                     tries += 1;
    //                 } else {
    //                     return Err(e);
    //                 }
    //             }
    //         }
    //     }
    // }

    // async fn try_sync_l1_deposited_events(&self) -> Result<(Vec<Deposited>, u64), ObserverError> {
    //     let l1_deposit_sync_eth_block_number = self.get_l1_deposit_sync_eth_block_number().await?;
    //     let (deposited_events, to_block) = self
    //         .liquidity_contract
    //         .get_deposited_events(l1_deposit_sync_eth_block_number)
    //         .await
    //         .map_err(|e| ObserverError::SyncL1DepositedEventsError(e.to_string()))?;
    //     let next_deposit_id = self.get_next_deposit_id().await?;

    //     // skip already synced events
    //     let deposited_events = deposited_events
    //         .into_iter()
    //         .skip_while(|e| e.deposit_id < next_deposit_id)
    //         .collect::<Vec<_>>();
    //     if let Some(first) = deposited_events.first() {
    //         if first.deposit_id != next_deposit_id {
    //             return Err(ObserverError::SyncL1DepositedEventsError(format!(
    //                 "First deposit id mismatch: {} != {}",
    //                 first.deposit_id, next_deposit_id
    //             )));
    //         }
    //     } else {
    //         // no new deposits
    //         let onchain_last_deposit_id = self.liquidity_contract.get_last_deposit_id().await?;
    //         if next_deposit_id <= onchain_last_deposit_id {
    //             return Err(ObserverError::SyncL1DepositedEventsError(format!(
    //                 "next_deposit_id is less than onchain rollup_next_deposit_index: {} <= {}",
    //                 next_deposit_id, onchain_last_deposit_id
    //             )));
    //         }
    //     }
    //     Ok((deposited_events, to_block))
    // }

    // async fn sync_l1_deposited_events(&self) -> Result<(), ObserverError> {
    //     let mut tries = 0;
    //     loop {
    //         if tries >= MAX_TRIES {
    //             return Err(ObserverError::FullBlockSyncError(
    //                 "Max tries exceeded".to_string(),
    //             ));
    //         }

    //         match self.try_sync_l1_deposited_events().await {
    //             Ok((deposited_events, to_block)) => {
    //                 let mut tx = self.pool.begin().await?;
    //                 for event in &deposited_events {
    //                     let deposit_hash = event.to_deposit().hash();
    //                     sqlx::query!(
    //                         "INSERT INTO deposited_events (deposit_id, depositor, pubkey_salt_hash, token_index, amount, is_eligible, deposited_at, deposit_hash, tx_hash)
    //                          VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    //                         event.deposit_id as i64,
    //                         event.depositor.to_hex(),
    //                         event.pubkey_salt_hash.to_hex(),
    //                         event.token_index as i64,
    //                         event.amount.to_hex(),
    //                         event.is_eligible,
    //                         event.deposited_at as i64,
    //                         deposit_hash.to_hex(),
    //                         event.tx_hash.to_hex()
    //                     )
    //                     .execute(&mut *tx)
    //                     .await?;
    //                 }
    //                 self.set_l1_deposit_sync_eth_block_number(&mut tx, to_block + 1)
    //                     .await?;
    //                 tx.commit().await?;

    //                 let last_deposit_id = self.get_next_deposit_id().await?;
    //                 log::info!(
    //                     "synced to deposit_id: {}, to_eth_block_number: {}",
    //                     last_deposit_id,
    //                     to_block
    //                 );
    //                 return Ok(());
    //             }
    //             Err(e) => {
    //                 if matches!(e, ObserverError::FullBlockSyncError(_)) {
    //                     log::error!("Observer l1 deposit sync error: {:?}", e);
    //                     // rollback to previous block number
    //                     let block_number = self
    //                         .get_l1_deposit_sync_eth_block_number()
    //                         .await?
    //                         .saturating_sub(BACKWARD_SYNC_BLOCK_NUMBER);
    //                     let mut tx = self.pool.begin().await?;
    //                     self.set_l1_deposit_sync_eth_block_number(&mut tx, block_number)
    //                         .await?;
    //                     tx.commit().await?;
    //                     sleep_for(SLEEP_TIME).await;
    //                     tries += 1;
    //                 } else {
    //                     return Err(e);
    //                 }
    //             }
    //         }
    //     }
    // }

    pub async fn sync(&self) -> Result<(), ObserverError> {
        // self.sync_l1_deposited_events().await?;
        // self.sync_blocks().await?;
        // self.sync_deposits().await?;
        log::info!("Observer synced");
        Ok(())
    }
}
