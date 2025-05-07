use super::{
    error::BlockchainError,
    handlers::send_transaction_with_gas_bump,
    proxy_contract::ProxyContract,
    utils::{get_provider_with_signer, NormalProvider},
};
use alloy::{
    network::TransactionBuilder,
    primitives::{Address, B256, U256},
    sol,
};

sol!(
    #[sol(rpc)]
    BlockBuilderReward,
    "abi/BlockBuilderReward.json",
);

#[derive(Debug, Clone)]
pub struct BlockBuilderRewardContract {
    pub provider: NormalProvider,
    pub address: Address,
}

impl BlockBuilderRewardContract {
    pub fn new(provider: NormalProvider, address: Address) -> Self {
        Self { provider, address }
    }

    pub async fn deploy(provider: NormalProvider, private_key: B256) -> anyhow::Result<Self> {
        let signer = get_provider_with_signer(&provider, private_key);
        let impl_contract = BlockBuilderReward::deploy(signer).await?;
        let impl_address = *impl_contract.address();
        let proxy = ProxyContract::deploy(provider.clone(), private_key, impl_address, &[]).await?;
        Ok(Self {
            provider,
            address: proxy.address,
        })
    }

    pub async fn get_current_period(&self) -> Result<u64, BlockchainError> {
        let contract = BlockBuilderReward::new(self.address, self.provider.clone());
        let period = contract.getCurrentPeriod().call().await?;
        Ok(period.to::<u64>())
    }

    pub async fn get_claimable_reward(
        &self,
        period_number: u64,
        user: Address,
    ) -> Result<U256, BlockchainError> {
        let contract = BlockBuilderReward::new(self.address, self.provider.clone());
        let reward = contract
            .getClaimableReward(U256::from(period_number), user)
            .call()
            .await?;
        Ok(reward)
    }

    pub async fn claim_reward(
        &self,
        signer_private_key: B256,
        gas_limit: Option<u64>,
        period_number: u64,
    ) -> Result<(), BlockchainError> {
        let signer = get_provider_with_signer(&self.provider, signer_private_key);
        let contract = BlockBuilderReward::new(self.address, signer.clone());
        let mut tx_request = contract
            .claimReward(U256::from(period_number))
            .into_transaction_request();
        if let Some(gas_limit) = gas_limit {
            tx_request.set_gas_limit(gas_limit);
        }
        send_transaction_with_gas_bump(signer, tx_request, "claim_reward").await?;
        Ok(())
    }

    pub async fn batch_claim_reward(
        &self,
        signer_private_key: B256,
        gas_limit: Option<u64>,
        period_numbers: &[u64],
    ) -> Result<(), BlockchainError> {
        let period_numbers = period_numbers
            .iter()
            .map(|&num| U256::from(num))
            .collect::<Vec<U256>>();
        let signer = get_provider_with_signer(&self.provider, signer_private_key);
        let contract = BlockBuilderReward::new(self.address, signer.clone());
        let mut tx_request = contract
            .batchClaimReward(period_numbers)
            .into_transaction_request();
        if let Some(gas_limit) = gas_limit {
            tx_request.set_gas_limit(gas_limit);
        }
        send_transaction_with_gas_bump(signer, tx_request, "batch_claim_reward").await?;
        Ok(())
    }
}
