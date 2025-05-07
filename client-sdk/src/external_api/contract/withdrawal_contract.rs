use super::{
    error::BlockchainError,
    proxy_contract::ProxyContract,
    utils::{get_provider_with_signer, NormalProvider},
};
use crate::external_api::contract::handlers::send_transaction_with_gas_bump;
use alloy::{
    primitives::{Address, B256, U256},
    sol,
};

sol!(
    #[allow(clippy::too_many_arguments)]
    #[sol(rpc)]
    WithdrawalAbi,
    "abi/Withdrawal.json",
);

#[derive(Debug, Clone)]
pub struct WithdrawalContract {
    pub provider: NormalProvider,
    pub address: Address,
}

impl WithdrawalContract {
    pub fn new(provider: NormalProvider, address: Address) -> Self {
        Self { provider, address }
    }

    pub async fn deploy(provider: NormalProvider, private_key: B256) -> anyhow::Result<Self> {
        let signer = get_provider_with_signer(&provider, private_key);
        let impl_contract = WithdrawalAbi::deploy(signer).await?;
        let impl_address = *impl_contract.address();
        let proxy = ProxyContract::deploy(provider.clone(), private_key, impl_address, &[]).await?;
        Ok(Self {
            provider,
            address: proxy.address,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn initialize(
        &self,
        signer_private_key: B256,
        admin: Address,
        scroll_messenger_address: Address,
        withdrawal_verifier_address: Address,
        liquidity_address: Address,
        rollup_address: Address,
        contribution_address: Address,
        direct_withdrawal_token_indices: Vec<U256>,
    ) -> Result<(), BlockchainError> {
        let signer = get_provider_with_signer(&self.provider, signer_private_key);
        let contract = WithdrawalAbi::new(self.address, signer.clone());
        let tx_request = contract
            .initialize(
                admin,
                scroll_messenger_address,
                withdrawal_verifier_address,
                liquidity_address,
                rollup_address,
                contribution_address,
                direct_withdrawal_token_indices,
            )
            .into_transaction_request();
        send_transaction_with_gas_bump(signer, tx_request, "initialize").await?;
        Ok(())
    }

    pub async fn get_direct_withdrawal_token_indices(&self) -> Result<Vec<u32>, BlockchainError> {
        let contract = WithdrawalAbi::new(self.address, self.provider.clone());
        let direct_withdrawal_token_indices: Vec<U256> =
            contract.getDirectWithdrawalTokenIndices().call().await?;
        let direct_withdrawal_token_indices = direct_withdrawal_token_indices
            .iter()
            .map(|&index| index.try_into().unwrap())
            .collect();
        Ok(direct_withdrawal_token_indices)
    }
}
