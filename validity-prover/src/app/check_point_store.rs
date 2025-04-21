use server_common::db::DbPool;

#[derive(Debug, Clone, Copy)]
pub enum EventType {
    Deposited,
    DepositLeafInserted,
    BlockPosted,
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

    pub async fn get_check_point(
        &self,
        event_type: EventType,
    ) -> Result<Option<u64>, sqlx::Error> {
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
                sqlx::query!("UPDATE observer_l1_deposit_sync_eth_block_num SET l1_deposit_sync_eth_block_num = $1 WHERE singleton_key = TRUE", eth_block_number as i64)
                    .execute(&self.pool)
                    .await?;
            }
            EventType::DepositLeafInserted => {
                sqlx::query!("UPDATE observer_deposit_sync_eth_block_num SET deposit_sync_eth_block_num = $1 WHERE singleton_key = TRUE", eth_block_number as i64)
                    .execute(&self.pool)
                    .await?;
            }
            EventType::BlockPosted => {
                sqlx::query!("UPDATE observer_block_sync_eth_block_num SET block_sync_eth_block_num = $1 WHERE singleton_key = TRUE", eth_block_number as i64)
                    .execute(&self.pool)
                    .await?;
            }
        }
        Ok(())
    }
}
