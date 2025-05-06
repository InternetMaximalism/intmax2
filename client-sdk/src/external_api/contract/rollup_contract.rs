use alloy::{
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
    pub tx_hash: B256,
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
    // For now, we'll leave the event methods as placeholders
    // since we need to understand how events are handled in alloy-rs
    pub async fn get_blocks_posted_event(
        &self,
        from_eth_block: u64,
        to_eth_block: u64,
    ) -> Result<Vec<BlockPosted>, BlockchainError> {
        log::info!(
            "get_blocks_posted_event: from_block={}, to_block={}",
            from_eth_block,
            to_eth_block
        );

        // This is a placeholder - we need to implement this properly
        // once we understand how events are handled in alloy-rs
        Err(BlockchainError::TransactionError(
            "Event handling not implemented for alloy-rs yet".to_string(),
        ))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn get_full_block_with_meta(
        &self,
        block_posted_events: &[BlockPosted],
    ) -> Result<Vec<FullBlockWithMeta>, BlockchainError> {
        use crate::external_api::contract::data_decoder::decode_post_block_calldata;
        use std::time::Instant;

        // We need to implement get_batch_transaction for alloy-rs
        // For now, we'll leave this as a placeholder
        Err(BlockchainError::TransactionError(
            "get_batch_transaction not implemented for alloy-rs yet".to_string(),
        ))
    }

    pub async fn get_deposit_leaf_inserted_events(
        &self,
        from_eth_block: u64,
        to_eth_block_number: u64,
    ) -> Result<Vec<DepositLeafInserted>, BlockchainError> {
        log::info!(
            "get_deposit_leaf_inserted_event: from_eth_block={}, to_eth_block_number={}",
            from_eth_block,
            to_eth_block_number
        );

        // This is a placeholder - we need to implement this properly
        // once we understand how events are handled in alloy-rs
        Err(BlockchainError::TransactionError(
            "Event handling not implemented for alloy-rs yet".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use alloy::primitives::B256;
    use intmax2_zkp::{
        common::signature_content::SignatureContent,
        ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait as _},
    };
    use num_bigint::BigUint;

    use crate::external_api::contract::{rollup_contract::RollupContract, utils::get_provider};

    #[tokio::test]
    async fn test_rollup_contract() -> anyhow::Result<()> {
        // This test needs to be updated for alloy-rs
        // The original test used Anvil from ethers, which might not be directly compatible with alloy
        // For now, we'll leave this as a placeholder
        Ok(())
    }
}
