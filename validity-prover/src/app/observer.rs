use intmax2_client_sdk::external_api::{
    contract::{
        liquidity_contract::LiquidityContract,
        rollup_contract::{DepositLeafInserted, FullBlockWithMeta, RollupContract},
    },
    utils::time::sleep_for,
};
use intmax2_interfaces::api::validity_prover::interface::{DepositInfo, Deposited};
use intmax2_zkp::{
    common::witness::full_block::FullBlock,
    ethereum_types::{bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait},
    utils::leafable::Leafable,
};

use server_common::db::{DbPool, DbPoolConfig};

use super::error::ObserverError;

const BACKWARD_SYNC_BLOCK_NUMBER: u64 = 1000;
const MAX_TRIES: u32 = 3;
const SLEEP_TIME: u64 = 10;

#[derive(Clone)]
pub struct Observer {
    rollup_contract: RollupContract,
    liquidity_contract: LiquidityContract,
    pool: DbPool,
}

impl Observer {
    pub async fn new(
        rollup_contract: RollupContract,
        liquidity_contract: LiquidityContract,
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
            liquidity_contract,
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

    async fn get_l1_deposit_sync_eth_block_number(&self) -> Result<u64, ObserverError> {
        let l1_deposit_sync_eth_block_number: Option<i64> = sqlx::query_scalar!(
            "SELECT l1_deposit_sync_eth_block_num FROM observer_l1_deposit_sync_eth_block_num WHERE singleton_key = TRUE"
        )
        .fetch_optional(&self.pool)
        .await?;
        log::info!(
            "get_l1_deposit_sync_eth_block_number: {:?}",
            l1_deposit_sync_eth_block_number
        );
        Ok(l1_deposit_sync_eth_block_number
            .map(|x| x as u64)
            .unwrap_or(self.liquidity_contract.deployed_block_number))
    }

    async fn set_l1_deposit_sync_eth_block_number(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        block_number: u64,
    ) -> Result<(), ObserverError> {
        log::info!("set_l1_deposit_sync_eth_block_number: {}", block_number);
        sqlx::query!(
            r#"
            INSERT INTO observer_l1_deposit_sync_eth_block_num (singleton_key, l1_deposit_sync_eth_block_num)
            VALUES (TRUE, $1)
            ON CONFLICT (singleton_key) DO UPDATE
            SET l1_deposit_sync_eth_block_num = $1
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

    pub async fn get_next_deposit_id(&self) -> Result<u64, ObserverError> {
        let result = sqlx::query!("SELECT COUNT(*) as count FROM deposited_events")
            .fetch_one(&self.pool)
            .await?;

        Ok(result.count.map(|i| i + 1).unwrap_or(1) as u64)
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

    pub async fn get_deposited_event(
        &self,
        pubkey_salt_hash: Bytes32,
    ) -> Result<Option<Deposited>, ObserverError> {
        let record = sqlx::query!(
            "SELECT deposit_id, depositor, pubkey_salt_hash, token_index, amount, is_eligible, deposited_at, deposit_hash, tx_hash
             FROM deposited_events 
             WHERE pubkey_salt_hash = $1",
            pubkey_salt_hash.to_hex()
        )
        .fetch_optional(&self.pool)
        .await?;
        match record {
            Some(r) => {
                let tx_hash = Bytes32::from_hex(&r.tx_hash)?;
                let depositor = r.depositor.parse()?;
                let amount = U256::from_hex(&r.amount)?;
                let deposited_at = r.deposited_at as u64;
                Ok(Some(Deposited {
                    deposit_id: r.deposit_id as u64,
                    depositor,
                    pubkey_salt_hash,
                    token_index: r.token_index as u32,
                    amount,
                    is_eligible: r.is_eligible,
                    deposited_at,
                    tx_hash,
                }))
            }
            None => Ok(None),
        }
    }

    pub async fn get_deposited_event_batch(
        &self,
        pubkey_salt_hashes: Vec<Bytes32>,
    ) -> Result<Vec<Option<Deposited>>, ObserverError> {
        let mut result = Vec::new();
        for pubkey_salt_hash in pubkey_salt_hashes {
            let event = self.get_deposited_event(pubkey_salt_hash).await?;
            result.push(event);
        }
        Ok(result)
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
        pubkey_salt_hash: Bytes32,
    ) -> Result<Option<DepositInfo>, ObserverError> {
        let deposited_event = self.get_deposited_event(pubkey_salt_hash).await?;
        if deposited_event.is_none() {
            return Ok(None);
        }
        let deposited_event = deposited_event.unwrap();
        let deposit_hash = deposited_event.to_deposit().hash();
        let leaf_inserted_event = sqlx::query!(
            r#"
            SELECT deposit_index, eth_block_number, eth_tx_index 
            FROM deposit_leaf_events 
            WHERE deposit_hash = $1
            "#,
            deposit_hash.to_bytes_be()
        )
        .fetch_optional(&self.pool)
        .await?;

        let event = match leaf_inserted_event {
            Some(e) => e,
            None => {
                return Ok(Some(DepositInfo {
                    deposit_id: deposited_event.deposit_id,
                    token_index: deposited_event.token_index,
                    deposit_hash,
                    block_number: None,
                    deposit_index: None,
                    l1_deposit_tx_hash: deposited_event.tx_hash,
                }))
            }
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
                token_index: deposited_event.token_index,
                block_number: Some(b.block_number as u32),
                deposit_index: Some(event.deposit_index as u32),
                deposit_id: deposited_event.deposit_id,
                l1_deposit_tx_hash: deposited_event.tx_hash,
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

    async fn try_sync_l1_deposited_events(&self) -> Result<(Vec<Deposited>, u64), ObserverError> {
        let l1_deposit_sync_eth_block_number = self.get_l1_deposit_sync_eth_block_number().await?;
        let (deposited_events, to_block) = self
            .liquidity_contract
            .get_deposited_events(l1_deposit_sync_eth_block_number)
            .await
            .map_err(|e| ObserverError::SyncL1DepositedEventsError(e.to_string()))?;
        let next_deposit_id = self.get_next_deposit_id().await?;

        // skip already synced events
        let deposited_events = deposited_events
            .into_iter()
            .skip_while(|e| e.deposit_id < next_deposit_id)
            .collect::<Vec<_>>();
        if let Some(first) = deposited_events.first() {
            if first.deposit_id != next_deposit_id {
                return Err(ObserverError::SyncL1DepositedEventsError(format!(
                    "First deposit id mismatch: {} != {}",
                    first.deposit_id, next_deposit_id
                )));
            }
        } else {
            // no new deposits
            let onchain_last_deposit_id = self.liquidity_contract.get_last_deposit_id().await?;
            if next_deposit_id <= onchain_last_deposit_id {
                return Err(ObserverError::SyncL1DepositedEventsError(format!(
                    "next_deposit_id is less than onchain rollup_next_deposit_index: {} <= {}",
                    next_deposit_id, onchain_last_deposit_id
                )));
            }
        }
        Ok((deposited_events, to_block))
    }

    async fn sync_l1_deposited_events(&self) -> Result<(), ObserverError> {
        let mut tries = 0;
        loop {
            if tries >= MAX_TRIES {
                return Err(ObserverError::FullBlockSyncError(
                    "Max tries exceeded".to_string(),
                ));
            }

            match self.try_sync_l1_deposited_events().await {
                Ok((deposited_events, to_block)) => {
                    let mut tx = self.pool.begin().await?;
                    for event in &deposited_events {
                        let deposit_hash = event.to_deposit().hash();
                        sqlx::query!(
                            "INSERT INTO deposited_events (deposit_id, depositor, pubkey_salt_hash, token_index, amount, is_eligible, deposited_at, deposit_hash, tx_hash) 
                             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                            event.deposit_id as i64,
                            event.depositor.to_hex(),
                            event.pubkey_salt_hash.to_hex(),
                            event.token_index as i64,
                            event.amount.to_hex(),
                            event.is_eligible,
                            event.deposited_at as i64,
                            deposit_hash.to_hex(),
                            event.tx_hash.to_hex()
                        )
                        .execute(&mut *tx)
                        .await?;
                    }
                    self.set_l1_deposit_sync_eth_block_number(&mut tx, to_block + 1)
                        .await?;
                    tx.commit().await?;

                    let last_deposit_id = self.get_next_deposit_id().await?;
                    log::info!(
                        "synced to deposit_id: {}, to_eth_block_number: {}",
                        last_deposit_id,
                        to_block
                    );
                    return Ok(());
                }
                Err(e) => {
                    if matches!(e, ObserverError::FullBlockSyncError(_)) {
                        log::error!("Observer l1 deposit sync error: {:?}", e);
                        // rollback to previous block number
                        let block_number = self
                            .get_l1_deposit_sync_eth_block_number()
                            .await?
                            .saturating_sub(BACKWARD_SYNC_BLOCK_NUMBER);
                        let mut tx = self.pool.begin().await?;
                        self.set_l1_deposit_sync_eth_block_number(&mut tx, block_number)
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
        self.sync_l1_deposited_events().await?;
        self.sync_blocks().await?;
        self.sync_deposits().await?;
        log::info!("Observer synced");
        Ok(())
    }
}
