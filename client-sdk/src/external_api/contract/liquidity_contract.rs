use super::{
    convert::{
        convert_address_to_alloy, convert_address_to_intmax, convert_bytes32_to_b256,
        convert_u256_to_alloy, convert_u256_to_intmax,
    },
    error::BlockchainError,
    handlers::send_transaction_with_gas_bump,
    proxy_contract::ProxyContract,
    utils::{get_provider_with_signer, NormalProvider},
};
use alloy::{
    network::TransactionBuilder,
    primitives::{Address, Bytes, B256, U256},
    sol,
};
use intmax2_interfaces::{
    api::withdrawal_server::interface::ContractWithdrawal, data::deposit_data::TokenType,
};
use intmax2_zkp::ethereum_types::{
    address::Address as ZkpAddress, bytes32::Bytes32, u256::U256 as ZkpU256,
    u32limb_trait::U32LimbTrait as _,
};
use serde::{Deserialize, Serialize};

use crate::external_api::{
    contract::convert::{convert_b256_to_bytes32, convert_tx_hash_to_bytes32},
    utils::retry::with_retry,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deposited {
    pub deposit_id: u64,
    pub depositor: ZkpAddress,
    pub pubkey_salt_hash: Bytes32,
    pub token_index: u32,
    pub amount: ZkpU256,
    pub is_eligible: bool,
    pub deposited_at: u64,

    // meta data
    pub tx_hash: Bytes32,
    pub eth_block_number: u64,
    pub eth_tx_index: u64,
}

impl Deposited {
    pub fn to_deposit(&self) -> intmax2_zkp::common::deposit::Deposit {
        intmax2_zkp::common::deposit::Deposit {
            depositor: self.depositor,
            pubkey_salt_hash: self.pubkey_salt_hash,
            amount: self.amount,
            token_index: self.token_index,
            is_eligible: self.is_eligible,
        }
    }
}

sol!(
    #[allow(clippy::too_many_arguments)]
    #[sol(rpc)]
    Liquidity,
    "abi/Liquidity.json",
);

#[derive(Debug, Clone)]
pub struct LiquidityContract {
    pub provider: NormalProvider,
    pub address: Address,
}

impl LiquidityContract {
    pub fn new(provider: NormalProvider, address: Address) -> Self {
        Self { provider, address }
    }

    pub async fn deploy(provider: NormalProvider, private_key: B256) -> anyhow::Result<Self> {
        let signer = get_provider_with_signer(&provider, private_key);
        let contract = Liquidity::deploy(signer).await?;
        let impl_address = *contract.address();
        let proxy = ProxyContract::deploy(provider.clone(), private_key, impl_address, &[]).await?;
        let address = proxy.address;
        Ok(Self { provider, address })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn initialize(
        &self,
        signer_private_key: B256,
        admin: Address,
        l_1_scroll_messenger: Address,
        rollup: Address,
        withdrawal: Address,
        claim: Address,
        analyzer: Address,
        contribution: Address,
        initial_erc20_tokens: Vec<Address>,
    ) -> Result<B256, BlockchainError> {
        let signer = get_provider_with_signer(&self.provider, signer_private_key);
        let contract = Liquidity::new(self.address, signer.clone());
        let tx_request = contract
            .initialize(
                admin,
                l_1_scroll_messenger,
                rollup,
                withdrawal,
                claim,
                analyzer,
                contribution,
                initial_erc20_tokens,
            )
            .into_transaction_request();
        let tx_hash = send_transaction_with_gas_bump(signer, tx_request, "initialize").await?;
        Ok(tx_hash)
    }

    pub async fn get_aml_permitter(&self) -> Result<Address, BlockchainError> {
        let contract = Liquidity::new(self.address, self.provider.clone());
        let aml_permitter = contract.amlPermitter().call().await?;
        Ok(aml_permitter)
    }

    pub async fn get_eligibility_permitter(&self) -> Result<Address, BlockchainError> {
        let contract = Liquidity::new(self.address, self.provider.clone());
        let eligibility_permitter =
            with_retry(|| async { contract.eligibilityPermitter().call().await })
                .await
                .map_err(|e| {
                    BlockchainError::TransactionError(format!(
                        "Error getting eligibility permitter: {:?}",
                        e
                    ))
                })?;
        Ok(eligibility_permitter)
    }

    pub async fn get_token_index(
        &self,
        token_type: TokenType,
        token_address: ZkpAddress,
        token_id: ZkpU256,
    ) -> Result<Option<u32>, BlockchainError> {
        if token_type != TokenType::NATIVE && token_address == ZkpAddress::zero() {
            // The contract will revert in this invalid case so we just return None before calling the contract
            return Ok(None);
        }
        let contract = Liquidity::new(self.address, self.provider.clone());
        let token_id = convert_u256_to_alloy(token_id);
        let token_address = convert_address_to_alloy(token_address);
        let result = contract
            .getTokenIndex(token_type as u8, token_address, token_id)
            .call()
            .await?;
        let is_found = result._0;
        let token_index = result._1;
        if !is_found {
            Ok(None)
        } else {
            Ok(Some(token_index))
        }
    }

    pub async fn get_token_info(
        &self,
        token_index: u32,
    ) -> Result<(TokenType, ZkpAddress, ZkpU256), BlockchainError> {
        let contract = Liquidity::new(self.address, self.provider.clone());
        let token_info = contract.getTokenInfo(token_index).call().await?;

        let token_type: u8 = token_info.tokenType;
        let token_type = TokenType::try_from(token_type)
            .map_err(|e| BlockchainError::ParseError(format!("Invalid token type: {:?}", e)))?;
        let token_address = convert_address_to_intmax(token_info.tokenAddress);
        let token_id = convert_u256_to_intmax(token_info.tokenId);
        Ok((token_type, token_address, token_id))
    }

    pub async fn get_last_deposit_id(&self) -> Result<u64, BlockchainError> {
        let contract = Liquidity::new(self.address, self.provider.clone());
        let deposit_id = contract.getLastDepositId().call().await?;
        Ok(deposit_id.to::<u64>())
    }

    pub async fn check_if_deposit_exists(&self, deposit_id: u64) -> Result<bool, BlockchainError> {
        let contract = Liquidity::new(self.address, self.provider.clone());
        let deposit_id = U256::from(deposit_id);
        let deposit_data = contract.getDepositData(deposit_id).call().await?;
        let exists = deposit_data.sender != Address::ZERO;
        Ok(exists)
    }

    pub async fn check_if_claimable(
        &self,
        withdrawal_hash: Bytes32,
    ) -> Result<bool, BlockchainError> {
        let contract = Liquidity::new(self.address, self.provider.clone());
        let withdrawal_hash_bytes = convert_bytes32_to_b256(withdrawal_hash);
        let block_number = contract
            .claimableWithdrawals(withdrawal_hash_bytes)
            .call()
            .await?;
        Ok(block_number != U256::ZERO)
    }

    pub async fn deposit_native(
        &self,
        signer_private_key: B256,
        gas_limit: Option<u64>,
        pubkey_salt_hash: Bytes32,
        amount: ZkpU256,
        aml_permission: &[u8],
        eligibility_permission: &[u8],
    ) -> Result<(), BlockchainError> {
        let signer = get_provider_with_signer(&self.provider, signer_private_key);
        let contract = Liquidity::new(self.address, signer.clone());
        let recipient_salt_hash = convert_bytes32_to_b256(pubkey_salt_hash);
        let amount = convert_u256_to_alloy(amount);
        let mut tx_request = contract
            .depositNativeToken(
                recipient_salt_hash,
                Bytes::copy_from_slice(aml_permission),
                Bytes::copy_from_slice(eligibility_permission),
            )
            .into_transaction_request();
        tx_request.set_value(amount);
        if let Some(gas_limit) = gas_limit {
            tx_request.set_gas_limit(gas_limit);
        }
        send_transaction_with_gas_bump(signer, tx_request, "deposit_native_token").await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn deposit_erc20(
        &self,
        signer_private_key: B256,
        gas_limit: Option<u64>,
        pubkey_salt_hash: Bytes32,
        amount: ZkpU256,
        token_address: ZkpAddress,
        aml_permission: &[u8],
        eligibility_permission: &[u8],
    ) -> Result<(), BlockchainError> {
        let signer = get_provider_with_signer(&self.provider, signer_private_key);
        let contract = Liquidity::new(self.address, signer.clone());
        let recipient_salt_hash = convert_bytes32_to_b256(pubkey_salt_hash);
        let amount = convert_u256_to_alloy(amount);
        let token_address = convert_address_to_alloy(token_address);
        let mut tx_request = contract
            .depositERC20(
                token_address,
                recipient_salt_hash,
                amount,
                Bytes::copy_from_slice(aml_permission),
                Bytes::copy_from_slice(eligibility_permission),
            )
            .into_transaction_request();
        if let Some(gas_limit) = gas_limit {
            tx_request.set_gas_limit(gas_limit);
        }
        send_transaction_with_gas_bump(signer, tx_request, "deposit_erc20_token").await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn deposit_erc721(
        &self,
        signer_private_key: B256,
        gas_limit: Option<u64>,
        pubkey_salt_hash: Bytes32,
        token_address: ZkpAddress,
        token_id: ZkpU256,
        aml_permission: &[u8],
        eligibility_permission: &[u8],
    ) -> Result<(), BlockchainError> {
        let signer = get_provider_with_signer(&self.provider, signer_private_key);
        let contract = Liquidity::new(self.address, signer.clone());
        let recipient_salt_hash = convert_bytes32_to_b256(pubkey_salt_hash);
        let token_id = convert_u256_to_alloy(token_id);
        let token_address = convert_address_to_alloy(token_address);
        let mut tx_request = contract
            .depositERC721(
                token_address,
                recipient_salt_hash,
                token_id,
                Bytes::copy_from_slice(aml_permission),
                Bytes::copy_from_slice(eligibility_permission),
            )
            .into_transaction_request();
        if let Some(gas_limit) = gas_limit {
            tx_request.set_gas_limit(gas_limit);
        }
        send_transaction_with_gas_bump(signer, tx_request, "deposit_erc721_token").await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn deposit_erc1155(
        &self,
        signer_private_key: B256,
        gas_limit: Option<u64>,
        pubkey_salt_hash: Bytes32,
        token_address: ZkpAddress,
        token_id: ZkpU256,
        amount: ZkpU256,
        aml_permission: &[u8],
        eligibility_permission: &[u8],
    ) -> Result<(), BlockchainError> {
        let signer = get_provider_with_signer(&self.provider, signer_private_key);
        let contract = Liquidity::new(self.address, signer.clone());
        let recipient_salt_hash = convert_bytes32_to_b256(pubkey_salt_hash);
        let token_id = convert_u256_to_alloy(token_id);
        let token_address = convert_address_to_alloy(token_address);
        let amount = convert_u256_to_alloy(amount);
        let mut tx_request = contract
            .depositERC1155(
                token_address,
                recipient_salt_hash,
                token_id,
                amount,
                Bytes::copy_from_slice(aml_permission),
                Bytes::copy_from_slice(eligibility_permission),
            )
            .into_transaction_request();
        if let Some(gas_limit) = gas_limit {
            tx_request.set_gas_limit(gas_limit);
        }
        send_transaction_with_gas_bump(signer, tx_request, "deposit_erc1155_token").await?;
        Ok(())
    }

    pub async fn claim_withdrawals(
        &self,
        signer_private_key: B256,
        gas_limit: Option<u64>,
        withdrawals: &[ContractWithdrawal],
    ) -> Result<(), BlockchainError> {
        let withdrawals = withdrawals
            .iter()
            .map(|w| WithdrawalLib::Withdrawal {
                recipient: convert_address_to_alloy(w.recipient),
                tokenIndex: w.token_index,
                amount: convert_u256_to_alloy(w.amount),
                nullifier: convert_bytes32_to_b256(w.nullifier),
            })
            .collect::<Vec<_>>();
        let signer = get_provider_with_signer(&self.provider, signer_private_key);
        let contract = Liquidity::new(self.address, signer.clone());
        let mut tx_request = contract
            .claimWithdrawals(withdrawals)
            .into_transaction_request();
        if let Some(gas_limit) = gas_limit {
            tx_request.set_gas_limit(gas_limit);
        }
        send_transaction_with_gas_bump(signer, tx_request, "claim_withdrawals").await?;
        Ok(())
    }

    pub async fn get_deposited_events(
        &self,
        from_eth_block: u64,
        to_eth_block: u64,
    ) -> Result<Vec<Deposited>, BlockchainError> {
        log::info!(
            "get_deposited_event: from_eth_block={}, to_eth_block={}",
            from_eth_block,
            to_eth_block
        );
        let contract = Liquidity::new(self.address, self.provider.clone());
        let events = contract
            .event_filter::<Liquidity::Deposited>()
            .address(self.address)
            .from_block(from_eth_block)
            .to_block(to_eth_block)
            .query()
            .await?;
        let mut deposited_events = Vec::new();
        for (event, meta) in events {
            deposited_events.push(Deposited {
                deposit_id: event.depositId.to::<u64>(),
                depositor: convert_address_to_intmax(event.sender),
                pubkey_salt_hash: convert_b256_to_bytes32(event.recipientSaltHash),
                token_index: event.tokenIndex,
                amount: convert_u256_to_intmax(event.amount),
                is_eligible: event.isEligible,
                deposited_at: event.depositedAt.to::<u64>(),
                tx_hash: convert_tx_hash_to_bytes32(meta.transaction_hash.unwrap()),
                eth_block_number: meta.block_number.unwrap(),
                eth_tx_index: meta.transaction_index.unwrap(),
            });
        }
        deposited_events.sort_by_key(|event| event.deposit_id);
        Ok(deposited_events)
    }
}
