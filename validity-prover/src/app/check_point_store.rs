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
        let row = sqlx::query!(
            r#"
            SELECT eth_block_number FROM event_sync_eth_block WHERE event_type = $1
            "#,
            event_type.to_string()
        )
        .fetch_optional(&self.pool)
        .await?;
        let eth_block_number = row.map(|row| row.eth_block_number as u64);
        Ok(eth_block_number)
    }

    pub async fn set_check_point(
        &self,
        event_type: EventType,
        eth_block_number: u64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO event_sync_eth_block (event_type, eth_block_number)
            VALUES ($1, $2)
            ON CONFLICT (event_type) 
            DO UPDATE SET eth_block_number = EXCLUDED.eth_block_number;
            "#,
            event_type.to_string(),
            eth_block_number as i64
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
