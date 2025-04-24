use std::fmt;

use server_common::db::DbPool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    Deposited,
    DepositLeafInserted,
    BlockPosted,
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventType::Deposited => write!(f, "Deposited"),
            EventType::DepositLeafInserted => write!(f, "DepositLeafInserted"),
            EventType::BlockPosted => write!(f, "BlockPosted"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ChainType {
    L1,
    L2,
}

impl EventType {
    pub fn to_chain_type(&self) -> ChainType {
        match self {
            EventType::Deposited => ChainType::L1,
            EventType::DepositLeafInserted => ChainType::L2,
            EventType::BlockPosted => ChainType::L2,
        }
    }
}

#[derive(Clone)]
pub struct CheckPointStore {
    pool: DbPool,
}

impl CheckPointStore {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn get_check_point(&self, event_type: EventType) -> Result<Option<u64>, sqlx::Error> {
        let eth_block_number = match event_type {
            EventType::Deposited => {
                sqlx::query!("SELECT l1_deposit_sync_eth_block_num FROM observer_l1_deposit_sync_eth_block_num WHERE singleton_key = TRUE")
                    .fetch_optional(&self.pool)
                    .await?
                    .map(|row| row.l1_deposit_sync_eth_block_num)
            }
            EventType::DepositLeafInserted => {
                sqlx::query!("SELECT deposit_sync_eth_block_num FROM observer_deposit_sync_eth_block_num WHERE singleton_key = TRUE")
                    .fetch_optional(&self.pool)
                    .await?
                    .map(|row| row.deposit_sync_eth_block_num)
            }
            EventType::BlockPosted => {
                sqlx::query!("SELECT block_sync_eth_block_num FROM observer_block_sync_eth_block_num WHERE singleton_key = TRUE")
                    .fetch_optional(&self.pool)
                    .await?
                    .map(|row| row.block_sync_eth_block_num)
            }
        };
        Ok(eth_block_number.map(|num| num as u64))
    }

    pub async fn set_check_point(
        &self,
        event_type: EventType,
        eth_block_number: u64,
    ) -> Result<(), sqlx::Error> {
        match event_type {
            EventType::Deposited => {
                sqlx::query!(
                    r#"
                    INSERT INTO observer_l1_deposit_sync_eth_block_num (singleton_key, l1_deposit_sync_eth_block_num)
                    VALUES (TRUE, $1)
                    ON CONFLICT (singleton_key) 
                    DO UPDATE SET l1_deposit_sync_eth_block_num = $1
                    "#,
                    eth_block_number as i64
                )
                .execute(&self.pool)
                .await?;
            }
            EventType::DepositLeafInserted => {
                sqlx::query!(
                    r#"
                    INSERT INTO observer_deposit_sync_eth_block_num (singleton_key, deposit_sync_eth_block_num)
                    VALUES (TRUE, $1)
                    ON CONFLICT (singleton_key) 
                    DO UPDATE SET deposit_sync_eth_block_num = $1
                    "#,
                    eth_block_number as i64
                )
                .execute(&self.pool)
                .await?;
            }
            EventType::BlockPosted => {
                sqlx::query!(
                    r#"
                    INSERT INTO observer_block_sync_eth_block_num (singleton_key, block_sync_eth_block_num)
                    VALUES (TRUE, $1)
                    ON CONFLICT (singleton_key) 
                    DO UPDATE SET block_sync_eth_block_num = $1
                    "#,
                    eth_block_number as i64
                )
                .execute(&self.pool)
                .await?;
            }
        }
        Ok(())
    }
}
