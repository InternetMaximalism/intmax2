use std::sync::Arc;

use ethers::{
    contract::abigen,
    core::k256::ecdsa::SigningKey,
    middleware::SignerMiddleware,
    providers::{Http, Provider},
    signers::Wallet,
    types::{self, Bytes, H256, U256 as EthU256},
};
use intmax2_zkp::{
    common::{
        signature_content::flatten::{FlatG1, FlatG2},
        witness::full_block::FullBlock,
    },
    ethereum_types::{
        address::Address, bytes16::Bytes16, bytes32::Bytes32, u256::U256,
        u32limb_trait::U32LimbTrait as _,
    },
};

use crate::external_api::utils::retry::with_retry;

use super::{
    error::BlockchainError,
    handlers::handle_contract_call,
    proxy_contract::ProxyContract,
    utils::{get_client, get_client_with_signer},
};

abigen!(Rollup, "abi/Rollup.json",);

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
    pub block_builder: Address,
    pub timestamp: u64,
    pub block_number: u32,
    pub deposit_tree_root: Bytes32,
    pub signature_hash: Bytes32,

    // meta data
    pub tx_hash: H256,
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
    pub rpc_url: String,
    pub chain_id: u64,
    pub address: ethers::types::Address,
}

impl RollupContract {
    pub fn new(rpc_url: &str, chain_id: u64, address: ethers::types::Address) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            chain_id,
            address,
        }
    }

    pub async fn deploy(rpc_url: &str, chain_id: u64, private_key: H256) -> anyhow::Result<Self> {
        let client = get_client_with_signer(rpc_url, chain_id, private_key).await?;
        let impl_contract = Rollup::deploy::<()>(Arc::new(client), ())?.send().await?;
        let impl_address = impl_contract.address();
        let proxy =
            ProxyContract::deploy(rpc_url, chain_id, private_key, impl_address, &[]).await?;
        let address = proxy.address();
        Ok(Self::new(rpc_url, chain_id, address))
    }

    pub fn address(&self) -> ethers::types::Address {
        self.address
    }

    pub async fn get_contract(&self) -> Result<rollup::Rollup<Provider<Http>>, BlockchainError> {
        let client = get_client(&self.rpc_url).await?;
        let contract = Rollup::new(self.address, client);
        Ok(contract)
    }

    pub async fn get_contract_with_signer(
        &self,
        private_key: H256,
    ) -> Result<rollup::Rollup<SignerMiddleware<Provider<Http>, Wallet<SigningKey>>>, BlockchainError>
    {
        let client = get_client_with_signer(&self.rpc_url, self.chain_id, private_key).await?;
        let contract = Rollup::new(self.address, Arc::new(client));
        Ok(contract)
    }

    pub async fn initialize(
        &self,
        signer_private_key: H256,
        admin: types::Address,
        scroll_messenger_address: types::Address,
        liquidity_address: types::Address,
        contribution_address: types::Address,
    ) -> Result<H256, BlockchainError> {
        let contract = self.get_contract_with_signer(signer_private_key).await?;
        let mut tx = contract.initialize(
            admin,
            scroll_messenger_address,
            liquidity_address,
            contribution_address,
        );
        let client =
            get_client_with_signer(&self.rpc_url, self.chain_id, signer_private_key).await?;
        let tx_hash = handle_contract_call(&client, &mut tx, "initialize", None).await?;
        Ok(tx_hash)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn post_registration_block(
        &self,
        signer_private_key: H256,
        gas_limit: Option<u64>,
        msg_value: U256,
        tx_tree_root: Bytes32,
        expiry: u64,
        block_builder_nonce: u32,
        sender_flag: Bytes16,
        agg_pubkey: FlatG1,
        agg_signature: FlatG2,
        message_point: FlatG2,
        sender_public_keys: Vec<U256>,
    ) -> Result<H256, BlockchainError> {
        let contract = self.get_contract_with_signer(signer_private_key).await?;
        let tx_tree_root: [u8; 32] = tx_tree_root.to_bytes_be().try_into().unwrap();
        let sender_flag: [u8; 16] = sender_flag.to_bytes_be().try_into().unwrap();
        let agg_pubkey = encode_flat_g1(&agg_pubkey);
        let agg_signature = encode_flat_g2(&agg_signature);
        let message_point = encode_flat_g2(&message_point);
        let sender_pubkeys: Vec<ethers::types::U256> = sender_public_keys
            .iter()
            .map(|e| ethers::types::U256::from_big_endian(&e.to_bytes_be()))
            .collect();
        let msg_value = ethers::types::U256::from_big_endian(&msg_value.to_bytes_be());
        let mut tx = contract
            .post_registration_block(
                tx_tree_root,
                expiry,
                block_builder_nonce,
                sender_flag,
                agg_pubkey,
                agg_signature,
                message_point,
                sender_pubkeys,
            )
            .value(msg_value);
        let client =
            get_client_with_signer(&self.rpc_url, self.chain_id, signer_private_key).await?;
        let tx_hash =
            handle_contract_call(&client, &mut tx, "post_registration_block", gas_limit).await?;
        Ok(tx_hash)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn post_non_registration_block(
        &self,
        signer_private_key: H256,
        gas_limit: Option<u64>,
        msg_value: U256,
        tx_tree_root: Bytes32,
        expiry: u64,
        block_builder_nonce: u32,
        sender_flag: Bytes16,
        agg_pubkey: FlatG1,
        agg_signature: FlatG2,
        message_point: FlatG2,
        public_keys_hash: Bytes32,
        account_ids: Vec<u8>,
    ) -> Result<H256, BlockchainError> {
        let contract = self.get_contract_with_signer(signer_private_key).await?;
        let tx_tree_root: [u8; 32] = tx_tree_root.to_bytes_be().try_into().unwrap();
        let sender_flag: [u8; 16] = sender_flag.to_bytes_be().try_into().unwrap();
        let agg_pubkey = encode_flat_g1(&agg_pubkey);
        let agg_signature = encode_flat_g2(&agg_signature);
        let message_point = encode_flat_g2(&message_point);
        let public_keys_hash: [u8; 32] = public_keys_hash.to_bytes_be().try_into().unwrap();
        let account_ids: Bytes = Bytes::from(account_ids);
        let msg_value = ethers::types::U256::from_big_endian(&msg_value.to_bytes_be());
        let mut tx = contract
            .post_non_registration_block(
                tx_tree_root,
                expiry,
                block_builder_nonce,
                sender_flag,
                agg_pubkey,
                agg_signature,
                message_point,
                public_keys_hash,
                account_ids,
            )
            .value(msg_value);
        let client =
            get_client_with_signer(&self.rpc_url, self.chain_id, signer_private_key).await?;
        let tx_hash =
            handle_contract_call(&client, &mut tx, "post_non_registration_block", gas_limit)
                .await?;
        Ok(tx_hash)
    }

    pub async fn process_deposits(
        &self,
        signer_private_key: H256,
        gas_limit: Option<u64>,
        last_processed_deposit_id: u32,
        deposit_hashes: &[Bytes32],
    ) -> Result<H256, BlockchainError> {
        let contract = self.get_contract_with_signer(signer_private_key).await?;
        let deposit_hashes: Vec<[u8; 32]> = deposit_hashes
            .iter()
            .map(|e| e.to_bytes_be())
            .map(|e| e.try_into().unwrap())
            .collect();
        let mut tx = contract.process_deposits(last_processed_deposit_id.into(), deposit_hashes);
        let client =
            get_client_with_signer(&self.rpc_url, self.chain_id, signer_private_key).await?;
        let tx_hash = handle_contract_call(&client, &mut tx, "process_deposits", gas_limit).await?;
        Ok(tx_hash)
    }

    pub async fn get_latest_block_number(&self) -> Result<u32, BlockchainError> {
        let contract = self.get_contract().await?;
        let latest_block_number =
            with_retry(|| async { contract.get_latest_block_number().call().await })
                .await
                .map_err(|_| {
                    BlockchainError::RPCError("failed to get latest block number".to_string())
                })?;
        Ok(latest_block_number)
    }

    pub async fn get_next_deposit_index(&self) -> Result<u32, BlockchainError> {
        let contract = self.get_contract().await?;
        let next_deposit_index = with_retry(|| async { contract.deposit_index().call().await })
            .await
            .map_err(|_| {
                BlockchainError::RPCError("failed to get next deposit index".to_string())
            })?;
        Ok(next_deposit_index)
    }

    pub async fn get_penalty(&self) -> Result<U256, BlockchainError> {
        let contract = self.get_contract().await?;
        let penalty: EthU256 = with_retry(|| async { contract.get_penalty().call().await })
            .await
            .map_err(|_| BlockchainError::RPCError("failed to get penalty fee".to_string()))?;
        let penalty = {
            let mut buf = [0u8; 32];
            penalty.to_big_endian(&mut buf);
            U256::from_bytes_be(&buf).unwrap()
        };
        Ok(penalty)
    }
}

// Event related methods
impl RollupContract {
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
        let contract = self.get_contract().await?;
        let events = with_retry(|| async {
            contract
                .block_posted_filter()
                .address(self.address.into())
                .from_block(from_eth_block)
                .to_block(to_eth_block)
                .query_with_meta()
                .await
        })
        .await
        .map_err(|_| BlockchainError::RPCError("failed to get blocks posted event".to_string()))?;
        let mut block_posited_events = Vec::new();
        for (event, meta) in events {
            block_posited_events.push(BlockPosted {
                prev_block_hash: Bytes32::from_bytes_be(&event.prev_block_hash).unwrap(),
                block_builder: Address::from_bytes_be(event.block_builder.as_bytes()).unwrap(),
                timestamp: event.timestamp,
                block_number: event.block_number.as_u32(),
                deposit_tree_root: Bytes32::from_bytes_be(&event.deposit_tree_root).unwrap(),
                signature_hash: Bytes32::from_bytes_be(&event.signature_hash).unwrap(),
                tx_hash: meta.transaction_hash,
                eth_block_number: meta.block_number.as_u64(),
                eth_tx_index: meta.transaction_index.as_u64(),
            });
        }
        block_posited_events.sort_by_key(|event| event.block_number);
        Ok(block_posited_events)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn get_full_block_with_meta(
        &self,
        block_posted_events: &[BlockPosted],
    ) -> Result<Vec<FullBlockWithMeta>, BlockchainError> {
        use crate::external_api::contract::{
            data_decoder::decode_post_block_calldata, utils::get_batch_transaction,
        };
        use std::time::Instant;

        let tx_hashes = block_posted_events
            .iter()
            .map(|e| e.tx_hash)
            .collect::<Vec<_>>();
        let instant = Instant::now();
        let txs = get_batch_transaction(&self.rpc_url, &tx_hashes).await?;
        log::info!(
            "get_batch_transaction: {:?} for {} txs",
            instant.elapsed(),
            tx_hashes.len()
        );
        let mut full_blocks = Vec::new();
        for (tx, event) in txs.iter().zip(block_posted_events) {
            let contract = self.get_contract().await?;
            let functions = contract.abi().functions();
            let full_block = decode_post_block_calldata(
                functions,
                event.prev_block_hash,
                event.deposit_tree_root,
                event.timestamp,
                event.block_number,
                event.block_builder,
                &tx.input,
            )
            .map_err(|e| {
                BlockchainError::DecodeCallDataError(format!(
                    "failed to decode post block calldata: {}",
                    e
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
            "get_deposit_leaf_inserted_event: from_eth_block={}, to_eth_block_number={}",
            from_eth_block,
            to_eth_block_number
        );
        let contract = self.get_contract().await?;
        let events = with_retry(|| async {
            contract
                .deposit_leaf_inserted_filter()
                .address(self.address.into())
                .from_block(from_eth_block)
                .to_block(to_eth_block_number)
                .query_with_meta()
                .await
        })
        .await
        .map_err(|e| {
            BlockchainError::RPCError(format!("failed to get deposit leaf inserted event: {}", e))
        })?;
        let mut deposit_leaf_inserted_events = Vec::new();
        for (event, meta) in events {
            deposit_leaf_inserted_events.push(DepositLeafInserted {
                deposit_index: event.deposit_index,
                deposit_hash: Bytes32::from_bytes_be(&event.deposit_hash).unwrap(),
                eth_block_number: meta.block_number.as_u64(),
                eth_tx_index: meta.transaction_index.as_u64(),
            });
        }
        deposit_leaf_inserted_events.sort_by_key(|event| event.deposit_index);
        Ok(deposit_leaf_inserted_events)
    }
}

fn encode_flat_g1(g1: &FlatG1) -> [[u8; 32]; 2] {
    g1.0.iter()
        .map(|e| e.to_bytes_be())
        .map(|e| e.try_into().unwrap())
        .collect::<Vec<[u8; 32]>>()
        .try_into()
        .unwrap()
}

fn encode_flat_g2(g2: &FlatG2) -> [[u8; 32]; 4] {
    g2.0.iter()
        .map(|e| e.to_bytes_be())
        .map(|e| e.try_into().unwrap())
        .collect::<Vec<[u8; 32]>>()
        .try_into()
        .unwrap()
}

#[cfg(test)]
mod tests {
    use ethers::{core::utils::Anvil, types::H256};
    use intmax2_zkp::{
        common::signature_content::SignatureContent,
        ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait as _},
    };
    use num_bigint::BigUint;

    use crate::external_api::contract::{
        rollup_contract::RollupContract, utils::get_latest_block_number,
    };

    #[tokio::test]
    async fn test_rollup_contract() -> anyhow::Result<()> {
        let anvil = Anvil::new().spawn();
        let private_key: [u8; 32] = anvil.keys()[0].to_bytes().into();
        let private_key = H256::from_slice(&private_key);
        let rpc_url = anvil.endpoint();
        let chain_id = anvil.chain_id();

        let rollup_contract = RollupContract::deploy(&rpc_url, chain_id, private_key).await?;
        let random_address = ethers::types::Address::random();
        rollup_contract
            .initialize(
                private_key,
                random_address,
                random_address,
                random_address,
                random_address,
            )
            .await?;

        let mut rng = intmax2_interfaces::utils::random::default_rng();
        let (keys, signature) = SignatureContent::rand(&mut rng);
        let pubkeys = keys.iter().map(|e| e.pubkey).collect::<Vec<_>>();
        rollup_contract
            .post_registration_block(
                private_key,
                None,
                0.into(),
                signature.block_sign_payload.tx_tree_root,
                signature.block_sign_payload.expiry.into(),
                signature.block_sign_payload.block_builder_nonce,
                signature.sender_flag,
                signature.agg_pubkey.clone(),
                signature.agg_signature.clone(),
                signature.message_point.clone(),
                pubkeys.clone(),
            )
            .await?;

        rollup_contract
            .post_non_registration_block(
                private_key,
                None,
                BigUint::from(10u32).pow(18).try_into().unwrap(),
                signature.block_sign_payload.tx_tree_root,
                signature.block_sign_payload.expiry.into(),
                signature.block_sign_payload.block_builder_nonce,
                signature.sender_flag,
                signature.agg_pubkey,
                signature.agg_signature,
                signature.message_point,
                Bytes32::rand(&mut rng),
                vec![],
            )
            .await?;

        let from_block = 0;
        let to_block = get_latest_block_number(&rpc_url).await?;
        let block_posted_events = rollup_contract
            .get_blocks_posted_event(from_block, to_block)
            .await?;
        let full_blocks = rollup_contract
            .get_full_block_with_meta(&block_posted_events)
            .await?;
        assert_eq!(full_blocks.len(), 2);
        Ok(())
    }
}
