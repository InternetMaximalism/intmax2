use std::sync::Arc;

use ethers::{
    contract::abigen,
    core::k256::ecdsa::SigningKey,
    middleware::SignerMiddleware,
    providers::{Http, Provider},
    signers::Wallet,
    types::{Address as EthAddress, H256, U256 as EthU256},
};

use crate::external_api::utils::retry::with_retry;

use super::{
    error::BlockchainError,
    handlers::handle_contract_call,
    proxy_contract::ProxyContract,
    utils::{get_client, get_client_with_signer},
};

abigen!(BlockBuilderReward, "abi/BlockBuilderReward.json",);

#[derive(Debug, Clone)]
pub struct BlockBuilderRewardContract {
    pub rpc_url: String,
    pub chain_id: u64,
    pub address: EthAddress,
}

impl BlockBuilderRewardContract {
    pub fn new(rpc_url: &str, chain_id: u64, address: EthAddress) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            chain_id,
            address,
        }
    }

    pub async fn deploy(rpc_url: &str, chain_id: u64, private_key: H256) -> anyhow::Result<Self> {
        let client = get_client_with_signer(rpc_url, chain_id, private_key).await?;
        let impl_contract = BlockBuilderReward::deploy::<()>(Arc::new(client), ())?
            .send()
            .await?;
        let impl_address = impl_contract.address();
        let proxy =
            ProxyContract::deploy(rpc_url, chain_id, private_key, impl_address, &[]).await?;
        let address = proxy.address();
        Ok(Self::new(rpc_url, chain_id, address))
    }

    pub fn address(&self) -> EthAddress {
        self.address
    }

    pub async fn get_contract(
        &self,
    ) -> Result<BlockBuilderReward<Provider<Http>>, BlockchainError> {
        let client = get_client(&self.rpc_url).await?;
        let contract = BlockBuilderReward::new(self.address, client);
        Ok(contract)
    }

    async fn get_contract_with_signer(
        &self,
        private_key: H256,
    ) -> Result<
        BlockBuilderReward<SignerMiddleware<Provider<Http>, Wallet<SigningKey>>>,
        BlockchainError,
    > {
        let client = get_client_with_signer(&self.rpc_url, self.chain_id, private_key).await?;
        let contract = BlockBuilderReward::new(self.address, Arc::new(client));
        Ok(contract)
    }

    pub async fn get_current_period(&self) -> Result<u64, BlockchainError> {
        let contract = self.get_contract().await?;
        let period = with_retry(|| async { contract.get_current_period().call().await })
            .await
            .map_err(|e| {
                BlockchainError::RPCError(format!("Error getting current period: {}", e))
            })?;
        Ok(period.as_u64())
    }

    pub async fn get_claimable_reward(
        &self,
        period_number: u64,
        user: EthAddress,
    ) -> Result<EthU256, BlockchainError> {
        let contract = self.get_contract().await?;
        let reward = with_retry(|| async {
            contract
                .get_claimable_reward(period_number.into(), user)
                .call()
                .await
        })
        .await
        .map_err(|e| BlockchainError::RPCError(format!("Error getting claimable reward: {}", e)))?;
        Ok(reward)
    }

    pub async fn claim_reward(
        &self,
        signer_private_key: H256,
        gas_limit: Option<u64>,
        period_number: u64,
    ) -> Result<(), BlockchainError> {
        let contract = self.get_contract_with_signer(signer_private_key).await?;
        let mut tx = contract.claim_reward(period_number.into());
        let client =
            get_client_with_signer(&self.rpc_url, self.chain_id, signer_private_key).await?;
        handle_contract_call(&client, &mut tx, "claim_reward", gas_limit).await?;
        Ok(())
    }

    pub async fn batch_claim_reward(
        &self,
        signer_private_key: H256,
        gas_limit: Option<u64>,
        period_numbers: &[u64],
    ) -> Result<(), BlockchainError> {
        let period_numbers = period_numbers
            .iter()
            .map(|&num| num.into())
            .collect::<Vec<EthU256>>();
        let contract = self.get_contract_with_signer(signer_private_key).await?;
        let mut tx = contract.batch_claim_reward(period_numbers);
        let client =
            get_client_with_signer(&self.rpc_url, self.chain_id, signer_private_key).await?;
        handle_contract_call(&client, &mut tx, "batch_claim_reward", gas_limit).await?;
        Ok(())
    }
}
