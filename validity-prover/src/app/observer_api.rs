use intmax2_client_sdk::external_api::contract::{
    liquidity_contract::Deposited, rollup_contract::FullBlockWithMeta,
};
use intmax2_interfaces::api::validity_prover::interface::DepositInfo;
use intmax2_zkp::{
    common::witness::full_block::FullBlock,
    ethereum_types::{bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait},
    utils::leafable::Leafable as _,
};

use super::{error::ObserverError, observer::Observer};

impl Observer {
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
