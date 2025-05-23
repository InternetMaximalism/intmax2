use crate::external_api::contract::{
    convert::{convert_address_to_intmax, convert_bytes32_to_tx_hash, convert_tx_hash_to_bytes32},
    data_decoder::decode_post_block_calldata,
    utils::get_batch_transaction,
};
use alloy::{
    consensus::Transaction,
    network::TransactionBuilder,
    primitives::{Address, Bytes, B256, U256},
    sol,
};
use intmax2_zkp::{
    common::{
        signature_content::flatten::{FlatG1, FlatG2},
        witness::full_block::FullBlock,
    },
    ethereum_types::{
        address::Address as ZkpAddress, bytes16::Bytes16, bytes32::Bytes32, u256::U256 as ZkpU256,
    },
};
use std::time::Instant;

use super::{
    convert::{
        convert_b256_to_bytes32, convert_bytes16_to_b128, convert_bytes32_to_b256,
        convert_u256_to_alloy, convert_u256_to_intmax,
    },
    error::BlockchainError,
    handlers::send_transaction_with_gas_bump,
    proxy_contract::ProxyContract,
    utils::{get_provider_with_signer, NormalProvider},
};

sol!(
    #[allow(clippy::too_many_arguments)]
    #[sol(rpc)]
    Rollup,
    "abi/Rollup.json",
);

#[derive(Clone, Debug)]
pub struct DepositLeafInserted {
    pub deposit_index: u32,
    pub deposit_hash: Bytes32,

    // meta data
    pub eth_block_number: u64,
    pub eth_tx_index: u64,
}

#[derive(Clone, Debug)]
pub struct BlockPosted {
    pub prev_block_hash: Bytes32,
    pub block_builder: ZkpAddress,
    pub timestamp: u64,
    pub block_number: u32,
    pub deposit_tree_root: Bytes32,
    pub signature_hash: Bytes32,

    // meta data
    pub tx_hash: Bytes32,
    pub eth_block_number: u64,
    pub eth_tx_index: u64,
}

#[derive(Clone, Debug)]
pub struct FullBlockWithMeta {
    pub full_block: FullBlock,
    pub eth_block_number: u64,
    pub eth_tx_index: u64,
}

#[derive(Debug, Clone)]
pub struct RollupContract {
    pub provider: NormalProvider,
    pub address: Address,
}

impl RollupContract {
    pub fn new(provider: NormalProvider, address: Address) -> Self {
        Self { provider, address }
    }

    pub async fn deploy(provider: NormalProvider, private_key: B256) -> anyhow::Result<Self> {
        let signer = get_provider_with_signer(&provider, private_key);
        let impl_contract = Rollup::deploy(signer).await?;
        let impl_address = *impl_contract.address();
        let proxy = ProxyContract::deploy(provider.clone(), private_key, impl_address, &[]).await?;
        Ok(Self {
            provider,
            address: proxy.address,
        })
    }

    pub async fn initialize(
        &self,
        signer_private_key: B256,
        admin: Address,
        scroll_messenger_address: Address,
        liquidity_address: Address,
        contribution_address: Address,
    ) -> Result<B256, BlockchainError> {
        let signer = get_provider_with_signer(&self.provider, signer_private_key);
        let contract = Rollup::new(self.address, signer.clone());
        let tx_request = contract
            .initialize(
                admin,
                scroll_messenger_address,
                liquidity_address,
                contribution_address,
            )
            .into_transaction_request();
        send_transaction_with_gas_bump(signer, tx_request, "initialize").await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn post_registration_block(
        &self,
        signer_private_key: B256,
        gas_limit: Option<u64>,
        msg_value: ZkpU256,
        tx_tree_root: Bytes32,
        expiry: u64,
        block_builder_nonce: u32,
        sender_flag: Bytes16,
        agg_pubkey: FlatG1,
        agg_signature: FlatG2,
        message_point: FlatG2,
        sender_public_keys: Vec<ZkpU256>,
    ) -> Result<B256, BlockchainError> {
        let signer = get_provider_with_signer(&self.provider, signer_private_key);
        let contract = Rollup::new(self.address, signer.clone());

        // Convert types to alloy types
        let tx_tree_root_bytes = convert_bytes32_to_b256(tx_tree_root);
        let sender_flag_bytes = convert_bytes16_to_b128(sender_flag);
        let agg_pubkey_bytes: [B256; 2] = [
            convert_u256_to_alloy(agg_pubkey.0[0]).into(),
            convert_u256_to_alloy(agg_pubkey.0[1]).into(),
        ];
        let agg_signature_bytes: [B256; 4] = [
            convert_u256_to_alloy(agg_signature.0[0]).into(),
            convert_u256_to_alloy(agg_signature.0[1]).into(),
            convert_u256_to_alloy(agg_signature.0[2]).into(),
            convert_u256_to_alloy(agg_signature.0[3]).into(),
        ];
        let message_point_bytes: [B256; 4] = [
            convert_u256_to_alloy(message_point.0[0]).into(),
            convert_u256_to_alloy(message_point.0[1]).into(),
            convert_u256_to_alloy(message_point.0[2]).into(),
            convert_u256_to_alloy(message_point.0[3]).into(),
        ];
        let sender_pubkeys: Vec<U256> = sender_public_keys
            .iter()
            .map(|pubkey| convert_u256_to_alloy(*pubkey))
            .collect();
        let msg_value = convert_u256_to_alloy(msg_value);
        let mut tx_request = contract
            .postRegistrationBlock(
                tx_tree_root_bytes,
                expiry,
                block_builder_nonce,
                sender_flag_bytes,
                agg_pubkey_bytes,
                agg_signature_bytes,
                message_point_bytes,
                sender_pubkeys,
            )
            .into_transaction_request();
        tx_request.set_value(msg_value);
        if let Some(gas_limit) = gas_limit {
            tx_request.set_gas_limit(gas_limit);
        }
        send_transaction_with_gas_bump(signer, tx_request, "post_registration_block").await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn post_non_registration_block(
        &self,
        signer_private_key: B256,
        gas_limit: Option<u64>,
        msg_value: ZkpU256,
        tx_tree_root: Bytes32,
        expiry: u64,
        block_builder_nonce: u32,
        sender_flag: Bytes16,
        agg_pubkey: FlatG1,
        agg_signature: FlatG2,
        message_point: FlatG2,
        public_keys_hash: Bytes32,
        account_ids: Vec<u8>,
    ) -> Result<B256, BlockchainError> {
        let signer = get_provider_with_signer(&self.provider, signer_private_key);
        let contract = Rollup::new(self.address, signer.clone());

        // Convert types to alloy types
        let tx_tree_root_bytes = convert_bytes32_to_b256(tx_tree_root);
        let sender_flag_bytes = convert_bytes16_to_b128(sender_flag);
        let agg_pubkey_bytes: [B256; 2] = [
            convert_u256_to_alloy(agg_pubkey.0[0]).into(),
            convert_u256_to_alloy(agg_pubkey.0[1]).into(),
        ];
        let agg_signature_bytes: [B256; 4] = [
            convert_u256_to_alloy(agg_signature.0[0]).into(),
            convert_u256_to_alloy(agg_signature.0[1]).into(),
            convert_u256_to_alloy(agg_signature.0[2]).into(),
            convert_u256_to_alloy(agg_signature.0[3]).into(),
        ];
        let message_point_bytes: [B256; 4] = [
            convert_u256_to_alloy(message_point.0[0]).into(),
            convert_u256_to_alloy(message_point.0[1]).into(),
            convert_u256_to_alloy(message_point.0[2]).into(),
            convert_u256_to_alloy(message_point.0[3]).into(),
        ];
        let public_keys_hash_bytes = convert_bytes32_to_b256(public_keys_hash);
        let account_ids_bytes = Bytes::from(account_ids);
        let msg_value = convert_u256_to_alloy(msg_value);

        let mut tx_request = contract
            .postNonRegistrationBlock(
                tx_tree_root_bytes,
                expiry,
                block_builder_nonce,
                sender_flag_bytes,
                agg_pubkey_bytes,
                agg_signature_bytes,
                message_point_bytes,
                public_keys_hash_bytes,
                account_ids_bytes,
            )
            .into_transaction_request();
        tx_request.set_value(msg_value);
        if let Some(gas_limit) = gas_limit {
            tx_request.set_gas_limit(gas_limit);
        }
        send_transaction_with_gas_bump(signer, tx_request, "post_non_registration_block").await
    }

    /// This is a backdoor method to simplify relaying deposits for testing purposes.
    /// It will be reverted in other environments.
    pub async fn process_deposits(
        &self,
        signer_private_key: B256,
        gas_limit: Option<u64>,
        last_processed_deposit_id: u32,
        deposit_hashes: &[Bytes32],
    ) -> Result<B256, BlockchainError> {
        let signer = get_provider_with_signer(&self.provider, signer_private_key);
        let contract = Rollup::new(self.address, signer.clone());
        let deposit_hashes_bytes: Vec<B256> = deposit_hashes
            .iter()
            .map(|e| convert_bytes32_to_b256(*e))
            .collect();
        let mut tx_request = contract
            .processDeposits(U256::from(last_processed_deposit_id), deposit_hashes_bytes)
            .into_transaction_request();
        if let Some(gas_limit) = gas_limit {
            tx_request.set_gas_limit(gas_limit);
        }
        send_transaction_with_gas_bump(signer, tx_request, "process_deposits").await
    }

    pub async fn get_latest_block_number(&self) -> Result<u32, BlockchainError> {
        let contract = Rollup::new(self.address, self.provider.clone());
        let latest_block_number = contract.getLatestBlockNumber().call().await?;
        Ok(latest_block_number)
    }

    pub async fn get_next_deposit_index(&self) -> Result<u32, BlockchainError> {
        let contract = Rollup::new(self.address, self.provider.clone());
        let next_deposit_index = contract.depositIndex().call().await?;
        Ok(next_deposit_index)
    }

    pub async fn get_block_hash(&self, block_number: u32) -> Result<Bytes32, BlockchainError> {
        let contract = Rollup::new(self.address, self.provider.clone());
        let block_hash = contract.getBlockHash(block_number).call().await?;
        Ok(convert_b256_to_bytes32(block_hash))
    }

    pub async fn get_penalty(&self) -> Result<ZkpU256, BlockchainError> {
        let contract = Rollup::new(self.address, self.provider.clone());
        let penalty = contract.getPenalty().call().await?;
        Ok(convert_u256_to_intmax(penalty))
    }
}

// Event related methods
impl RollupContract {
    pub async fn get_blocks_posted_event(
        &self,
        from_eth_block: u64,
        to_eth_block: u64,
    ) -> Result<Vec<BlockPosted>, BlockchainError> {
        log::info!("get_blocks_posted_event: from_block={from_eth_block}, to_block={to_eth_block}");
        let contract = Rollup::new(self.address, self.provider.clone());
        let events = contract
            .event_filter::<Rollup::BlockPosted>()
            .address(self.address)
            .from_block(from_eth_block)
            .to_block(to_eth_block)
            .query()
            .await?;
        let mut block_posited_events = Vec::new();
        for (event, meta) in events {
            block_posited_events.push(BlockPosted {
                prev_block_hash: convert_b256_to_bytes32(event.prevBlockHash),
                block_builder: convert_address_to_intmax(event.blockBuilder),
                timestamp: event.timestamp,
                block_number: event.blockNumber.to(),
                deposit_tree_root: convert_b256_to_bytes32(event.depositTreeRoot),
                signature_hash: convert_b256_to_bytes32(event.signatureHash),
                tx_hash: convert_tx_hash_to_bytes32(meta.transaction_hash.unwrap()),
                eth_block_number: meta.block_number.unwrap(),
                eth_tx_index: meta.transaction_index.unwrap(),
            });
        }
        block_posited_events.sort_by_key(|event| event.block_number);
        Ok(block_posited_events)
    }

    pub async fn get_full_block_with_meta(
        &self,
        block_posted_events: &[BlockPosted],
    ) -> Result<Vec<FullBlockWithMeta>, BlockchainError> {
        let tx_hashes = block_posted_events
            .iter()
            .map(|e| convert_bytes32_to_tx_hash(e.tx_hash))
            .collect::<Vec<_>>();
        let instant = Instant::now();
        let txs = get_batch_transaction(&self.provider, &tx_hashes).await?;
        log::info!(
            "get_batch_transaction: {:?} for {} txs",
            instant.elapsed(),
            tx_hashes.len()
        );
        let mut full_blocks = Vec::new();
        for (tx, event) in txs.iter().zip(block_posted_events) {
            let input = tx.input();
            let full_block = decode_post_block_calldata(
                event.prev_block_hash,
                event.deposit_tree_root,
                event.timestamp,
                event.block_number,
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
                eth_block_number: event.eth_block_number,
                eth_tx_index: event.eth_tx_index,
            });
        }

        // Sort by block number
        full_blocks.sort_by_key(|block| block.full_block.block.block_number);

        Ok(full_blocks)
    }

    pub async fn get_deposit_leaf_inserted_events(
        &self,
        from_eth_block: u64,
        to_eth_block_number: u64,
    ) -> Result<Vec<DepositLeafInserted>, BlockchainError> {
        log::info!(
            "get_deposit_leaf_inserted_event: from_eth_block={from_eth_block}, to_eth_block_number={to_eth_block_number}"
        );
        let contract = Rollup::new(self.address, self.provider.clone());
        let events = contract
            .event_filter::<Rollup::DepositLeafInserted>()
            .address(self.address)
            .from_block(from_eth_block)
            .to_block(to_eth_block_number)
            .query()
            .await?;
        let mut deposit_leaf_inserted_events = Vec::new();
        for (event, meta) in events {
            deposit_leaf_inserted_events.push(DepositLeafInserted {
                deposit_index: event.depositIndex,
                deposit_hash: convert_b256_to_bytes32(event.depositHash),
                eth_block_number: meta.block_number.unwrap(),
                eth_tx_index: meta.transaction_index.unwrap(),
            });
        }
        deposit_leaf_inserted_events.sort_by_key(|event| event.deposit_index);
        Ok(deposit_leaf_inserted_events)
    }
}
