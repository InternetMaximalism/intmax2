use std::sync::Arc;

use ethers::{
    contract::abigen,
    core::k256::ecdsa::SigningKey,
    middleware::SignerMiddleware,
    providers::{Http, Provider},
    signers::Wallet,
    types::{Address as EthAddress, H256},
};
use intmax2_interfaces::{
    api::{
        validity_prover::interface::Deposited, withdrawal_server::interface::ContractWithdrawal,
    },
    data::deposit_data::TokenType,
};
use intmax2_zkp::ethereum_types::{
    address::Address, bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait as _,
};

use crate::external_api::{
    contract::{utils::get_latest_block_number, EVENT_BLOCK_RANGE},
    utils::retry::with_retry,
};

use super::{
    error::BlockchainError,
    handlers::handle_contract_call,
    proxy_contract::ProxyContract,
    utils::{get_client, get_client_with_signer},
};

abigen!(Liquidity, "abi/Liquidity.json",);

#[derive(Debug, Clone)]
pub struct LiquidityContract {
    pub rpc_url: String,
    pub chain_id: u64,
    pub address: EthAddress,
    pub deployed_block_number: u64,
}

impl LiquidityContract {
    pub fn new(
        rpc_url: &str,
        chain_id: u64,
        address: EthAddress,
        deployed_block_number: u64,
    ) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            chain_id,
            address,
            deployed_block_number,
        }
    }

    pub async fn deploy(rpc_url: &str, chain_id: u64, private_key: H256) -> anyhow::Result<Self> {
        let client = get_client_with_signer(rpc_url, chain_id, private_key).await?;
        let impl_contract = Liquidity::deploy::<()>(Arc::new(client), ())?
            .send()
            .await?;
        let impl_address = impl_contract.address();
        let proxy =
            ProxyContract::deploy(rpc_url, chain_id, private_key, impl_address, &[]).await?;
        let address = proxy.address();
        let deployed_block_number = proxy.deployed_block_number();
        Ok(Self::new(rpc_url, chain_id, address, deployed_block_number))
    }

    pub fn address(&self) -> EthAddress {
        self.address
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn initialize(
        &self,
        signer_private_key: H256,
        admin: EthAddress,
        l_1_scroll_messenger: EthAddress,
        rollup: EthAddress,
        withdrawal: EthAddress,
        claim: EthAddress,
        analyzer: EthAddress,
        contribution: EthAddress,
        initial_erc20_tokens: Vec<EthAddress>,
    ) -> Result<H256, BlockchainError> {
        let contract = self.get_contract_with_signer(signer_private_key).await?;
        let mut tx = contract.initialize(
            admin,
            l_1_scroll_messenger,
            rollup,
            withdrawal,
            claim,
            analyzer,
            contribution,
            initial_erc20_tokens,
        );
        let client =
            get_client_with_signer(&self.rpc_url, self.chain_id, signer_private_key).await?;
        let tx_hash = handle_contract_call(&client, &mut tx, "initialize").await?;
        Ok(tx_hash)
    }

    pub async fn get_contract(
        &self,
    ) -> Result<liquidity::Liquidity<Provider<Http>>, BlockchainError> {
        let client = get_client(&self.rpc_url).await?;
        let contract = Liquidity::new(self.address, client);
        Ok(contract)
    }

    async fn get_contract_with_signer(
        &self,
        private_key: H256,
    ) -> Result<
        liquidity::Liquidity<SignerMiddleware<Provider<Http>, Wallet<SigningKey>>>,
        BlockchainError,
    > {
        let client = get_client_with_signer(&self.rpc_url, self.chain_id, private_key).await?;
        let contract = Liquidity::new(self.address, Arc::new(client));
        Ok(contract)
    }

    pub async fn get_token_index(
        &self,
        token_type: TokenType,
        token_address: Address,
        token_id: U256,
    ) -> Result<Option<u32>, BlockchainError> {
        if token_type != TokenType::NATIVE && token_address == Address::zero() {
            // The contract will revert in this invalid case so we just return None before calling the contract
            return Ok(None);
        }
        let contract = self.get_contract().await?;
        let token_id = ethers::types::U256::from_big_endian(&token_id.to_bytes_be());
        let token_address = EthAddress::from_slice(&token_address.to_bytes_be());
        let (is_found, token_index) = with_retry(|| async {
            contract
                .get_token_index(token_type as u8, token_address, token_id)
                .call()
                .await
        })
        .await
        .map_err(|e| BlockchainError::RPCError(format!("Error getting token index: {:?}", e)))?;
        if !is_found {
            Ok(None)
        } else {
            Ok(Some(token_index))
        }
    }

    pub async fn get_token_info(
        &self,
        token_index: u32,
    ) -> Result<(TokenType, Address, U256), BlockchainError> {
        let contract = self.get_contract().await?;
        let token_info = with_retry(|| async { contract.get_token_info(token_index).call().await })
            .await
            .map_err(|e| BlockchainError::RPCError(format!("Error getting token info: {:?}", e)))?;

        let token_type: u8 = token_info.token_type;
        let token_type = TokenType::try_from(token_type)
            .map_err(|e| BlockchainError::ParseError(format!("Invalid token type: {:?}", e)))?;
        let token_address = Address::from_bytes_be(token_info.token_address.as_bytes()).unwrap();
        let token_id = {
            let mut buf = [0u8; 32];
            token_info.token_id.to_big_endian(&mut buf);
            U256::from_bytes_be(&buf).unwrap()
        };
        Ok((token_type, token_address, token_id))
    }

    pub async fn get_last_deposit_id(&self) -> Result<u64, BlockchainError> {
        let contract = self.get_contract().await?;
        let deposit_id = with_retry(|| async { contract.get_last_deposit_id().call().await })
            .await
            .map_err(|e| {
                BlockchainError::RPCError(format!("Error getting last deposit id: {:?}", e))
            })?;
        Ok(deposit_id.as_u64())
    }

    pub async fn check_if_deposit_exists(&self, deposit_id: u64) -> Result<bool, BlockchainError> {
        let contract = self.get_contract().await?;
        let deposit_id = ethers::types::U256::from(deposit_id);
        let deposit_data: DepositData =
            with_retry(|| async { contract.get_deposit_data(deposit_id).call().await })
                .await
                .map_err(|e| {
                    BlockchainError::RPCError(format!("Error while getting deposit data: {:?}", e))
                })?;
        let exists = deposit_data.sender != EthAddress::zero();
        Ok(exists)
    }

    pub async fn check_if_claimable(
        &self,
        withdrawal_hash: Bytes32,
    ) -> Result<bool, BlockchainError> {
        let contract: Liquidity<Provider<Http>> = self.get_contract().await?;
        let withdrawal_hash: [u8; 32] = withdrawal_hash.to_bytes_be().try_into().unwrap();
        let block_number: ethers::types::U256 =
            with_retry(|| async { contract.claimable_withdrawals(withdrawal_hash).call().await })
                .await
                .map_err(|e| {
                    BlockchainError::RPCError(format!("Error checking if claimed: {:?}", e))
                })?;
        Ok(block_number != ethers::types::U256::zero())
    }

    pub async fn deposit_native(
        &self,
        signer_private_key: H256,
        pubkey_salt_hash: Bytes32,
        amount: U256,
        aml_permission: &[u8],
        eligibility_permission: &[u8],
    ) -> Result<(), BlockchainError> {
        let contract = self.get_contract_with_signer(signer_private_key).await?;
        let recipient_salt_hash: [u8; 32] = pubkey_salt_hash.to_bytes_be().try_into().unwrap();
        let amount = ethers::types::U256::from_big_endian(&amount.to_bytes_be());
        let mut tx = contract
            .deposit_native_token(
                recipient_salt_hash,
                aml_permission.to_vec().into(),
                eligibility_permission.to_vec().into(),
            )
            .value(amount);
        let client =
            get_client_with_signer(&self.rpc_url, self.chain_id, signer_private_key).await?;
        handle_contract_call(&client, &mut tx, "deposit_native_token").await?;
        Ok(())
    }

    pub async fn deposit_erc20(
        &self,
        signer_private_key: H256,
        pubkey_salt_hash: Bytes32,
        amount: U256,
        token_address: Address,
        aml_permission: &[u8],
        eligibility_permission: &[u8],
    ) -> Result<(), BlockchainError> {
        let contract = self.get_contract_with_signer(signer_private_key).await?;
        let recipient_salt_hash: [u8; 32] = pubkey_salt_hash.to_bytes_be().try_into().unwrap();
        let amount = ethers::types::U256::from_big_endian(&amount.to_bytes_be());
        let token_address = EthAddress::from_slice(&token_address.to_bytes_be());
        let mut tx = contract.deposit_erc20(
            token_address,
            recipient_salt_hash,
            amount,
            aml_permission.to_vec().into(),
            eligibility_permission.to_vec().into(),
        );
        let client =
            get_client_with_signer(&self.rpc_url, self.chain_id, signer_private_key).await?;
        handle_contract_call(&client, &mut tx, "deposit_erc20_token").await?;
        Ok(())
    }

    pub async fn deposit_erc721(
        &self,
        signer_private_key: H256,
        pubkey_salt_hash: Bytes32,
        token_address: Address,
        token_id: U256,
        aml_permission: &[u8],
        eligibility_permission: &[u8],
    ) -> Result<(), BlockchainError> {
        let contract = self.get_contract_with_signer(signer_private_key).await?;
        let recipient_salt_hash: [u8; 32] = pubkey_salt_hash.to_bytes_be().try_into().unwrap();
        let token_id = ethers::types::U256::from_big_endian(&token_id.to_bytes_be());
        let token_address = EthAddress::from_slice(&token_address.to_bytes_be());
        let mut tx = contract.deposit_erc721(
            token_address,
            recipient_salt_hash,
            token_id,
            aml_permission.to_vec().into(),
            eligibility_permission.to_vec().into(),
        );
        let client =
            get_client_with_signer(&self.rpc_url, self.chain_id, signer_private_key).await?;
        handle_contract_call(&client, &mut tx, "deposit_erc721_token").await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn deposit_erc1155(
        &self,
        signer_private_key: H256,
        pubkey_salt_hash: Bytes32,
        token_address: Address,
        token_id: U256,
        amount: U256,
        aml_permission: &[u8],
        eligibility_permission: &[u8],
    ) -> Result<(), BlockchainError> {
        let contract = self.get_contract_with_signer(signer_private_key).await?;
        let recipient_salt_hash: [u8; 32] = pubkey_salt_hash.to_bytes_be().try_into().unwrap();
        let amount = ethers::types::U256::from_big_endian(&amount.to_bytes_be());
        let token_id = ethers::types::U256::from_big_endian(&token_id.to_bytes_be());
        let token_address = EthAddress::from_slice(&token_address.to_bytes_be());
        let mut tx = contract.deposit_erc1155(
            token_address,
            recipient_salt_hash,
            token_id,
            amount,
            aml_permission.to_vec().into(),
            eligibility_permission.to_vec().into(),
        );
        let client =
            get_client_with_signer(&self.rpc_url, self.chain_id, signer_private_key).await?;
        handle_contract_call(&client, &mut tx, "deposit_erc1155_token").await?;
        Ok(())
    }

    pub async fn claim_withdrawals(
        &self,
        signer_private_key: H256,
        withdrawals: &[ContractWithdrawal],
    ) -> Result<(), BlockchainError> {
        let withdrawals = withdrawals
            .iter()
            .map(|w| {
                let recipient = EthAddress::from_slice(&w.recipient.to_bytes_be());
                let token_index = w.token_index;
                let amount = ethers::types::U256::from_big_endian(&w.amount.to_bytes_be());
                let nullifier: [u8; 32] = w.nullifier.to_bytes_be().try_into().unwrap();
                Withdrawal {
                    recipient,
                    token_index,
                    amount,
                    nullifier,
                }
            })
            .collect::<Vec<_>>();
        let contract = self.get_contract_with_signer(signer_private_key).await?;
        let mut tx = contract.claim_withdrawals(withdrawals);
        let client =
            get_client_with_signer(&self.rpc_url, self.chain_id, signer_private_key).await?;
        handle_contract_call(&client, &mut tx, "claim_withdrawals").await?;
        Ok(())
    }

    pub async fn get_deposited_events(
        &self,
        from_block: u64,
    ) -> Result<(Vec<Deposited>, u64), BlockchainError> {
        log::info!("get_deposited_event: from_block={:?}", from_block);
        let mut events = Vec::new();
        let mut from_block = from_block;
        let mut is_final = false;
        let final_to_block = loop {
            let mut to_block = from_block + EVENT_BLOCK_RANGE - 1;
            let latest_block_number = get_latest_block_number(&self.rpc_url).await?;
            if to_block > latest_block_number {
                to_block = latest_block_number;
                is_final = true;
            }
            if from_block > to_block {
                break to_block;
            }
            log::info!(
                "get_deposited_event: from_block={}, to_block={}",
                from_block,
                to_block
            );
            let contract = self.get_contract().await?;
            let new_events = with_retry(|| async {
                contract
                    .deposited_filter()
                    .address(self.address.into())
                    .from_block(from_block)
                    .to_block(to_block)
                    .query_with_meta()
                    .await
            })
            .await
            .map_err(|_| BlockchainError::RPCError("failed to get deposited event".to_string()))?;
            events.extend(new_events);
            if is_final {
                break to_block;
            }
            from_block += EVENT_BLOCK_RANGE;
        };
        let mut deposited_events = Vec::new();
        for (event, meta) in events {
            deposited_events.push(Deposited {
                deposit_id: event.deposit_id.as_u64(),
                depositor: Address::from_bytes_be(&event.sender.to_fixed_bytes()).unwrap(),
                pubkey_salt_hash: Bytes32::from_bytes_be(&event.recipient_salt_hash).unwrap(),
                token_index: event.token_index,
                amount: {
                    let mut buf = [0u8; 32];
                    event.amount.to_big_endian(&mut buf);
                    U256::from_bytes_be(&buf).unwrap()
                },
                is_eligible: event.is_eligible,
                deposited_at: event.deposited_at.as_u64(),
                tx_hash: Bytes32::from_bytes_be(&meta.transaction_hash.to_fixed_bytes()).unwrap(),
            });
        }
        deposited_events.sort_by_key(|event| event.deposit_id);
        Ok((deposited_events, final_to_block))
    }
}
