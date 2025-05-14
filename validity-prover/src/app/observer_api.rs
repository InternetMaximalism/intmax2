use intmax2_client_sdk::external_api::contract::{
    liquidity_contract::{Deposited, LiquidityContract},
    rollup_contract::{DepositLeafInserted, FullBlockWithMeta, RollupContract},
};
use intmax2_interfaces::api::validity_prover::interface::DepositInfo;
use intmax2_zkp::{
    common::witness::full_block::FullBlock,
    ethereum_types::{bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait},
    utils::leafable::Leafable as _,
};
use server_common::db::{DbPool, DbPoolConfig};
use tracing::instrument;

use crate::EnvVar;

use super::{check_point_store::EventType, error::ObserverError};

#[derive(Clone)]
pub struct ObserverApi {
    pub(crate) rollup_contract: RollupContract,
    pub(crate) liquidity_contract: LiquidityContract,
    pub(crate) pool: DbPool,
}

impl ObserverApi {
    pub async fn new(
        env: &EnvVar,
        rollup_contract: RollupContract,
        liquidity_contract: LiquidityContract,
    ) -> Result<Self, ObserverError> {
        let pool = DbPool::from_config(&DbPoolConfig {
            max_connections: env.database_max_connections,
            idle_timeout: env.database_timeout,
            url: env.database_url.to_string(),
        })
        .await?;
        Ok(Self {
            rollup_contract,
            liquidity_contract,
            pool,
        })
    }

    pub async fn get_local_last_deposit_id(&self) -> Result<u64, ObserverError> {
        let result = sqlx::query!("SELECT MAX(deposit_id) FROM deposited_events")
            .fetch_optional(&self.pool)
            .await?;
        let last_deposit_id = result.and_then(|r| r.max).map(|i| i as u64);
        Ok(last_deposit_id.unwrap_or(0))
    }

    pub async fn get_local_last_deposit_index(&self) -> Result<Option<u32>, ObserverError> {
        let result = sqlx::query!("SELECT MAX(deposit_index) FROM deposit_leaf_events")
            .fetch_optional(&self.pool)
            .await?;
        let last_deposit_index = result.and_then(|r| r.max).map(|i| i as u32);
        Ok(last_deposit_index)
    }

    pub async fn get_local_last_block_number(&self) -> Result<u32, ObserverError> {
        let result = sqlx::query!("SELECT MAX(block_number) FROM full_blocks")
            .fetch_optional(&self.pool)
            .await?;
        let last_block_number = result.and_then(|r| r.max).map(|i| i as u32);
        Ok(last_block_number.unwrap_or(0))
    }

    #[instrument(skip(self))]
    pub async fn get_local_next_event_id(
        &self,
        event_type: EventType,
    ) -> Result<u64, ObserverError> {
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
    pub async fn get_local_last_eth_block_number(
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

    #[instrument(skip(self))]
    pub async fn is_synced(&self, event_type: EventType) -> Result<bool, ObserverError> {
        let local_next_event_id = self.get_local_next_event_id(event_type).await?;
        let onchain_next_event_id = self.get_onchain_next_event_id(event_type).await?;
        Ok(local_next_event_id >= onchain_next_event_id)
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
            if is_synced {
                // This means no deposit leaf inserted events though we have synced all events
                return Ok(Some(Vec::new()));
            } else {
                //  We have not synced all events yet
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

    pub async fn get_next_deposit_index(&self) -> Result<u32, ObserverError> {
        let last_deposit_index = self.get_local_last_deposit_index().await?;
        Ok(last_deposit_index.map(|i| i + 1).unwrap_or(0))
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
                if full_block.block.block_number != block_number {
                    return Err(ObserverError::BlockNumberMismatch(
                        full_block.block.block_number,
                        block_number,
                    ));
                }
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
            "SELECT deposit_id, depositor, pubkey_salt_hash, token_index, amount, is_eligible, deposited_at, deposit_hash, tx_hash, eth_block_number, eth_tx_index
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
                    eth_block_number: r.eth_block_number as u64,
                    eth_tx_index: r.eth_tx_index as u64,
                }))
            }
            None => Ok(None),
        }
    }

    /// get the latest value of the deposit index included in the block
    pub async fn get_latest_included_deposit_index(&self) -> Result<Option<u32>, ObserverError> {
        let block_number = self.get_local_last_block_number().await?;
        if block_number == 0 {
            // genesis block does not have any deposits
            return Ok(None);
        }

        let latest_block = self
            .get_full_block_with_meta(block_number)
            .await?
            .ok_or(ObserverError::BlockNotFound(block_number))?;
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
}
