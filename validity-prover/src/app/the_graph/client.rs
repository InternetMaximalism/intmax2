use super::{error::GraphClientError, query_client::TheGraphQueryClient};
use alloy::consensus::Transaction as _;
use intmax2_client_sdk::external_api::contract::{
    convert::convert_bytes32_to_tx_hash,
    data_decoder::decode_post_block_calldata,
    error::BlockchainError,
    liquidity_contract::Deposited,
    rollup_contract::{DepositLeafInserted, FullBlockWithMeta},
    utils::{get_batch_transaction, NormalProvider},
};

// A wrapper around TheGraphClient that provides additional functionality for interacting with the L1 and L2 providers.
#[derive(Clone, Debug)]
pub struct TheGraphClient {
    pub client: TheGraphQueryClient,
    pub l1_provider: NormalProvider,
    pub l2_provider: NormalProvider,
}

impl TheGraphClient {
    pub fn new(
        l1_url: String,
        l2_url: String,
        l1_bearer_token: Option<String>,
        l2_bearer_token: Option<String>,
        l1_provider: NormalProvider,
        l2_provider: NormalProvider,
    ) -> Self {
        Self {
            client: TheGraphQueryClient::new(l1_url, l2_url, l1_bearer_token, l2_bearer_token),
            l1_provider,
            l2_provider,
        }
    }

    pub async fn get_full_block_with_meta(
        &self,
        next_block_number: u32,
        limit: usize,
    ) -> Result<Vec<FullBlockWithMeta>, GraphClientError> {
        let block_posteds = self
            .client
            .fetch_block_posteds(next_block_number, limit)
            .await?;

        // fetch transactions for calldata and metadata
        let tx_hashes = block_posteds
            .iter()
            .map(|entry| convert_bytes32_to_tx_hash(entry.transaction_hash))
            .collect::<Vec<_>>();
        let txs = get_batch_transaction(&self.l2_provider, &tx_hashes).await?;
        let mut full_blocks = Vec::new();
        for (tx, event) in txs.iter().zip(block_posteds) {
            let input = tx.input();
            let full_block = decode_post_block_calldata(
                event.prev_block_hash,
                event.deposit_tree_root,
                event.block_timestamp,
                event.rollup_block_number,
                event.block_builder,
                input,
            )
            .map_err(|e| {
                BlockchainError::DecodeCallDataError(format!(
                    "failed to decode post block calldata: {e}"
                ))
            })?;
            full_blocks.push(FullBlockWithMeta {
                full_block,
                eth_block_number: tx.block_number.unwrap(),
                eth_tx_index: tx.transaction_index.unwrap(),
            });
        }

        Ok(full_blocks)
    }

    pub async fn get_deposit_leaf_inserted_events(
        &self,
        next_deposit_index: u32,
        limit: usize,
    ) -> Result<Vec<DepositLeafInserted>, GraphClientError> {
        let deposit_leaf_inserteds = self
            .client
            .fetch_deposit_leaves(next_deposit_index, limit)
            .await?;

        // fetch transactions for tx metadata
        let tx_hashes = deposit_leaf_inserteds
            .iter()
            .map(|entry| convert_bytes32_to_tx_hash(entry.transaction_hash))
            .collect::<Vec<_>>();
        let txs = get_batch_transaction(&self.l2_provider, &tx_hashes).await?;

        let mut deposit_leaf_events = Vec::new();
        for (tx, event) in txs.iter().zip(deposit_leaf_inserteds) {
            deposit_leaf_events.push(DepositLeafInserted {
                deposit_index: event.deposit_index,
                deposit_hash: event.deposit_hash,
                eth_block_number: tx.block_number.unwrap(),
                eth_tx_index: tx.transaction_index.unwrap(),
            });
        }

        Ok(deposit_leaf_events)
    }

    pub async fn get_deposited_events(
        &self,
        next_deposit_id: u64,
        limit: usize,
    ) -> Result<Vec<Deposited>, GraphClientError> {
        let depositeds = self.client.fetch_deposited(next_deposit_id, limit).await?;

        // fetch transactions for tx metadata
        let tx_hashes = depositeds
            .iter()
            .map(|entry| convert_bytes32_to_tx_hash(entry.transaction_hash))
            .collect::<Vec<_>>();
        let txs = get_batch_transaction(&self.l1_provider, &tx_hashes).await?;

        let mut deposits_events = Vec::new();
        for (tx, event) in txs.iter().zip(depositeds) {
            deposits_events.push(Deposited {
                deposit_id: event.deposit_id,
                depositor: event.sender,
                pubkey_salt_hash: event.recipient_salt_hash,
                token_index: event.token_index,
                amount: event.amount,
                is_eligible: event.is_eligible,
                deposited_at: event.deposited_at,
                tx_hash: event.transaction_hash,
                eth_block_number: tx.block_number.unwrap(),
                eth_tx_index: tx.transaction_index.unwrap(),
            });
        }
        Ok(deposits_events)
    }
}
