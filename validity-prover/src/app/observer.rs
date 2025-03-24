use intmax2_client_sdk::external_api::{
    contract::rollup_contract::{DepositLeafInserted, FullBlockWithMeta, RollupContract},
    utils::time::sleep_for,
};
use intmax2_interfaces::api::validity_prover::interface::DepositInfo;
use intmax2_zkp::{
    common::witness::full_block::FullBlock,
    ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait},
};

use server_common::db::{DbPool, DbPoolConfig};

use super::error::ObserverError;

const BACKWARD_SYNC_BLOCK_NUMBER: u64 = 1000;
const MAX_TRIES: u32 = 3;
const SLEEP_TIME: u64 = 10;

#[derive(Clone)]
pub struct Observer {
    rollup_contract: RollupContract,
    pool: DbPool,
}

impl Observer {
    pub async fn new(
        rollup_contract: RollupContract,
        database_url: &str,
        database_max_connections: u32,
        database_timeout: u64,
    ) -> Result<Self, ObserverError> {
        let pool = DbPool::from_config(&DbPoolConfig {
            max_connections: database_max_connections,
            idle_timeout: database_timeout,
            url: database_url.to_string(),
        })
        .await?;

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

            sqlx::query!(
                r#"
                INSERT INTO observer_block_sync_eth_block_num (singleton_key, block_sync_eth_block_num)
                VALUES (TRUE, $1)
                ON CONFLICT (singleton_key) DO UPDATE
                SET block_sync_eth_block_num = $1
                "#,
                rollup_contract.deployed_block_number as i64
            )
            .execute(&pool)
            .await?;
        }

        Ok(Observer {
            rollup_contract,
            pool,
        })
    }

    async fn get_block_sync_eth_block_number(&self) -> Result<u64, ObserverError> {
        let block_sync_eth_block_number: Option<i64> = sqlx::query_scalar!(
            "SELECT block_sync_eth_block_num FROM observer_block_sync_eth_block_num WHERE singleton_key = TRUE"
        )
        .fetch_optional(&self.pool)
        .await?;

        log::info!(
            "get_block_sync_eth_block_number: {:?}",
            block_sync_eth_block_number
        );

        Ok(block_sync_eth_block_number
            .map(|x| x as u64)
            .unwrap_or(self.rollup_contract.deployed_block_number))
    }

    async fn set_block_sync_eth_block_number(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        block_number: u64,
    ) -> Result<(), ObserverError> {
        log::info!("set_block_sync_eth_block_number: {}", block_number);
        sqlx::query!(
            r#"
            INSERT INTO observer_block_sync_eth_block_num (singleton_key, block_sync_eth_block_num)
            VALUES (TRUE, $1)
            ON CONFLICT (singleton_key) DO UPDATE
            SET block_sync_eth_block_num = $1
            "#,
            block_number as i64
        )
        .execute(tx.as_mut())
        .await?;

        Ok(())
    }

    async fn get_deposit_sync_eth_block_number(&self) -> Result<u64, ObserverError> {
        let deposit_sync_eth_block_number: Option<i64> = sqlx::query_scalar!(
            "SELECT deposit_sync_eth_block_num FROM observer_deposit_sync_eth_block_num WHERE singleton_key = TRUE"
        )
        .fetch_optional(&self.pool)
        .await?;
        log::info!(
            "get_deposit_sync_eth_block_number: {:?}",
            deposit_sync_eth_block_number
        );
        Ok(deposit_sync_eth_block_number
            .map(|x| x as u64)
            .unwrap_or(self.rollup_contract.deployed_block_number))
    }

    async fn set_deposit_sync_eth_block_number(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        block_number: u64,
    ) -> Result<(), ObserverError> {
        log::info!("set_deposit_sync_eth_block_number: {}", block_number);
        sqlx::query!(
            r#"
            INSERT INTO observer_deposit_sync_eth_block_num (singleton_key, deposit_sync_eth_block_num)
            VALUES (TRUE, $1)
            ON CONFLICT (singleton_key) DO UPDATE
            SET deposit_sync_eth_block_num = $1
            "#,
            block_number as i64
        )
        .execute(tx.as_mut())
        .await?;
        Ok(())
    }

    pub async fn get_next_block_number(&self) -> Result<u32, ObserverError> {
        let result = sqlx::query!("SELECT COUNT(*) as count FROM full_blocks")
            .fetch_one(&self.pool)
            .await?;

        Ok(result.count.unwrap_or(0) as u32)
    }

    pub async fn get_next_deposit_index(&self) -> Result<u32, ObserverError> {
        let result = sqlx::query!("SELECT COUNT(*) as count FROM deposit_leaf_events")
            .fetch_one(&self.pool)
            .await?;

        Ok(result.count.unwrap_or(0) as u32)
    }

    pub async fn get_full_block(&self, block_number: u32) -> Result<FullBlock, ObserverError> {
        let record = sqlx::query!(
            "SELECT full_block FROM full_blocks WHERE block_number = $1",
            block_number as i32
        )
        .fetch_optional(&self.pool)
        .await?;

        let full_block: FullBlock = match record {
            Some(r) => bincode::deserialize(&r.full_block)?,
            None => return Err(ObserverError::BlockNotFound(block_number)),
        };

        if full_block.block.block_number != block_number {
            return Err(ObserverError::BlockNumberMismatch(
                full_block.block.block_number,
                block_number,
            ));
        }

        Ok(full_block)
    }

    pub async fn get_full_block_with_meta(
        &self,
        block_number: u32,
    ) -> Result<Option<FullBlockWithMeta>, ObserverError> {
        let record = sqlx::query!(
            "SELECT eth_block_number, eth_tx_index, full_block 
             FROM full_blocks 
             WHERE block_number = $1",
            block_number as i32
        )
        .fetch_optional(&self.pool)
        .await?;
        match record {
            Some(r) => {
                let full_block: FullBlock = bincode::deserialize(&r.full_block)?;
                Ok(Some(FullBlockWithMeta {
                    full_block,
                    eth_block_number: r.eth_block_number as u64,
                    eth_tx_index: r.eth_tx_index as u64,
                }))
            }
            None => Ok(None),
        }
    }

    /// get the latest value of the deposit index included in the block
    pub async fn get_latest_included_deposit_index(&self) -> Result<Option<u32>, ObserverError> {
        let next_block_number = self.get_next_block_number().await?;
        if next_block_number < 2 {
            // no blocks or genesis block does not have any deposits
            return Ok(None);
        }

        let latest_block = self
            .get_full_block_with_meta(next_block_number - 1)
            .await?
            .ok_or(ObserverError::BlockNotFound(next_block_number - 1))?;
        let deposit = sqlx::query!(
            r#"
            SELECT deposit_index, deposit_hash, eth_block_number, eth_tx_index
            FROM deposit_leaf_events
            WHERE (eth_block_number, eth_tx_index) <= ($1, $2)
            ORDER BY deposit_index DESC
            LIMIT 1
            "#,
            latest_block.eth_block_number as i64,
            latest_block.eth_tx_index as i64,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(deposit.map(|d| d.deposit_index as u32))
    }

    pub async fn get_deposit_info(
        &self,
        deposit_hash: Bytes32,
    ) -> Result<Option<DepositInfo>, ObserverError> {
        let event = sqlx::query!(
            r#"
            SELECT deposit_index, eth_block_number, eth_tx_index 
            FROM deposit_leaf_events 
            WHERE deposit_hash = $1
            "#,
            deposit_hash.to_bytes_be()
        )
        .fetch_optional(&self.pool)
        .await?;

        let event = match event {
            Some(e) => e,
            None => return Ok(None),
        };

        let block = sqlx::query!(
            r#"
            SELECT block_number
            FROM full_blocks 
            WHERE (eth_block_number, eth_tx_index) > ($1, $2)
            ORDER BY eth_block_number, eth_tx_index
            LIMIT 1
            "#,
            event.eth_block_number,
            event.eth_tx_index
        )
        .fetch_optional(&self.pool)
        .await?;

        match block {
            Some(b) => Ok(Some(DepositInfo {
                deposit_hash,
                block_number: b.block_number as u32,
                deposit_index: event.deposit_index as u32,
            })),
            None => Ok(None),
        }
    }

    pub async fn get_deposits_between_blocks(
        &self,
        block_number: u32,
    ) -> Result<Vec<DepositLeafInserted>, ObserverError> {
        let current_block = self.get_full_block_with_meta(block_number).await?;
        let prev_block = self
            .get_full_block_with_meta(block_number.saturating_sub(1))
            .await?;

        let (prev_block, current_block) = match (prev_block, current_block) {
            (Some(p), Some(c)) => (p, c),
            _ => return Ok(Vec::new()),
        };

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

        Ok(deposits
            .into_iter()
            .map(|d| DepositLeafInserted {
                deposit_index: d.deposit_index as u32,
                deposit_hash: Bytes32::from_bytes_be(&d.deposit_hash).unwrap(),
                eth_block_number: d.eth_block_number as u64,
                eth_tx_index: d.eth_tx_index as u64,
            })
            .collect())
    }

    async fn try_sync_deposits(&self) -> Result<(Vec<DepositLeafInserted>, u64), ObserverError> {
        let deposit_sync_eth_block_number = self.get_deposit_sync_eth_block_number().await?;
        let (deposit_leaf_events, to_block) = self
            .rollup_contract
            .get_deposit_leaf_inserted_events(deposit_sync_eth_block_number)
            .await
            .map_err(|e| ObserverError::FullBlockSyncError(e.to_string()))?;
        let next_deposit_index = self.get_next_deposit_index().await?;

        // skip already synced events
        let deposit_leaf_events = deposit_leaf_events
            .into_iter()
            .skip_while(|e| e.deposit_index < next_deposit_index)
            .collect::<Vec<_>>();
        if let Some(first) = deposit_leaf_events.first() {
            if first.deposit_index != next_deposit_index {
                return Err(ObserverError::FullBlockSyncError(format!(
                    "First deposit index mismatch: {} != {}",
                    first.deposit_index, next_deposit_index
                )));
            }
        } else {
            // no new deposits
            let rollup_next_deposit_index = self.rollup_contract.get_next_deposit_index().await?;
            if next_deposit_index < rollup_next_deposit_index {
                return Err(ObserverError::FullBlockSyncError(format!(
                    "next_deposit_index is less than rollup_next_deposit_index: {} < {}",
                    next_deposit_index, rollup_next_deposit_index
                )));
            }
        }
        Ok((deposit_leaf_events, to_block))
    }

    async fn sync_deposits(&self) -> Result<(), ObserverError> {
        let mut tries = 0;
        loop {
            if tries >= MAX_TRIES {
                return Err(ObserverError::FullBlockSyncError(
                    "Max tries exceeded".to_string(),
                ));
            }

            match self.try_sync_deposits().await {
                Ok((deposit_leaf_events, to_block)) => {
                    let mut tx = self.pool.begin().await?;
                    for event in &deposit_leaf_events {
                        sqlx::query!(
                            "INSERT INTO deposit_leaf_events (deposit_index, deposit_hash, eth_block_number, eth_tx_index) 
                             VALUES ($1, $2, $3, $4)",
                            event.deposit_index as i32,
                            event.deposit_hash.to_bytes_be(),
                            event.eth_block_number as i64,
                            event.eth_tx_index as i64
                        )
                        .execute(&mut *tx)
                        .await?;
                    }
                    self.set_deposit_sync_eth_block_number(&mut tx, to_block + 1)
                        .await?;
                    tx.commit().await?;

                    let next_deposit_index = self.get_next_deposit_index().await?;
                    log::info!(
                        "synced to deposit_index: {}, to_eth_block_number: {}",
                        next_deposit_index.saturating_sub(1),
                        to_block
                    );
                    return Ok(());
                }
                Err(e) => {
                    if matches!(e, ObserverError::FullBlockSyncError(_)) {
                        log::error!("Observer sync error: {:?}", e);
                        // rollback to previous block number
                        let block_number = self
                            .get_deposit_sync_eth_block_number()
                            .await?
                            .saturating_sub(BACKWARD_SYNC_BLOCK_NUMBER);
                        let mut tx = self.pool.begin().await?;
                        self.set_deposit_sync_eth_block_number(&mut tx, block_number)
                            .await?;
                        tx.commit().await?;
                        sleep_for(SLEEP_TIME).await;
                        tries += 1;
                    } else {
                        return Err(e);
                    }
                }
            }
        }
    }

    async fn try_sync_block(&self) -> Result<(Vec<FullBlockWithMeta>, u64), ObserverError> {
        let block_sync_eth_block_number = self.get_block_sync_eth_block_number().await?;
        let (full_blocks, to_block) = self
            .rollup_contract
            .get_full_block_with_meta(block_sync_eth_block_number)
            .await
            .map_err(|e| ObserverError::FullBlockSyncError(e.to_string()))?;
        let next_block_number = self.get_next_block_number().await?;
        // skip already synced events
        let full_blocks = full_blocks
            .into_iter()
            .skip_while(|b| b.full_block.block.block_number < next_block_number)
            .collect::<Vec<_>>();
        if let Some(first) = full_blocks.first() {
            if first.full_block.block.block_number != next_block_number {
                return Err(ObserverError::FullBlockSyncError(format!(
                    "First block mismatch: {} != {}",
                    first.full_block.block.block_number, next_block_number
                )));
            }
        } else {
            // no new blocks
            let rollup_block_number = self.rollup_contract.get_latest_block_number().await?;
            if next_block_number <= rollup_block_number {
                return Err(ObserverError::FullBlockSyncError(format!(
                    "next_block_number is less than rollup_block_number: {} <= {}",
                    next_block_number, rollup_block_number
                )));
            }
        }
        Ok((full_blocks, to_block))
    }

    async fn sync_blocks(&self) -> Result<(), ObserverError> {
        let mut tries = 0;
        loop {
            if tries >= MAX_TRIES {
                return Err(ObserverError::FullBlockSyncError(
                    "Max tries exceeded".to_string(),
                ));
            }
            match self.try_sync_block().await {
                Ok((full_blocks, to_block)) => {
                    let mut tx = self.pool.begin().await?;
                    for block in &full_blocks {
                        sqlx::query!(
                            "INSERT INTO full_blocks (block_number, eth_block_number, eth_tx_index, full_block) 
                             VALUES ($1, $2, $3, $4)",
                            block.full_block.block.block_number as i32,
                            block.eth_block_number as i64,
                            block.eth_tx_index as i64,
                            bincode::serialize(&block.full_block).unwrap()
                        )
                        .execute(&mut *tx)
                        .await?;
                    }
                    self.set_block_sync_eth_block_number(&mut tx, to_block + 1)
                        .await?;
                    tx.commit().await?;

                    let next_block_number = self.get_next_block_number().await?;
                    log::info!(
                        "synced to block_number: {}, to_eth_block_number: {}",
                        next_block_number.saturating_sub(1),
                        to_block
                    );
                    return Ok(());
                }
                Err(e) => {
                    if matches!(e, ObserverError::FullBlockSyncError(_)) {
                        log::error!("Observer sync error: {:?}", e);

                        // rollback to previous block number
                        let block_number = self
                            .get_block_sync_eth_block_number()
                            .await?
                            .saturating_sub(BACKWARD_SYNC_BLOCK_NUMBER);
                        let mut tx = self.pool.begin().await?;
                        self.set_block_sync_eth_block_number(&mut tx, block_number)
                            .await?;
                        tx.commit().await?;
                        sleep_for(SLEEP_TIME).await;
                        tries += 1;
                    } else {
                        return Err(e);
                    }
                }
            }
        }
    }

    pub async fn sync(&self) -> Result<(), ObserverError> {
        self.sync_blocks().await?;
        self.sync_deposits().await?;
        log::info!("Observer synced");
        Ok(())
    }
}
