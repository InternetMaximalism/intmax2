use super::{
    error::BlockchainError,
    handlers::send_transaction_with_gas_bump,
    utils::{get_provider_with_signer, NormalProvider},
};
use alloy::{
    network::TransactionBuilder,
    primitives::{Address, B256, U256},
    sol,
};

sol!(
    #[sol(rpc)]
    ERC20,
    "abi/TestERC20.json",
);

#[derive(Debug, Clone)]
pub struct ERC20Contract {
    pub provider: NormalProvider,
    pub address: Address,
}

impl ERC20Contract {
    pub fn new(provider: NormalProvider, address: Address) -> Self {
        Self { provider, address }
    }

    pub async fn deploy(
        provider: NormalProvider,
        private_key: B256,
        initial_address: Address,
    ) -> anyhow::Result<Self> {
        let signer = get_provider_with_signer(&provider, private_key);
        let contract = ERC20::deploy(signer, initial_address).await?;
        let address = *contract.address();
        Ok(Self { provider, address })
    }

    pub async fn balance_of(&self, account: Address) -> Result<U256, BlockchainError> {
        let contract = ERC20::new(self.address, self.provider.clone());
        let balance = contract.balanceOf(account).call().await?;
        Ok(balance)
    }

    pub async fn transfer(
        &self,
        signer_private_key: B256,
        gas_limit: Option<u64>,
        to: Address,
        amount: U256,
    ) -> Result<(), BlockchainError> {
        let signer = get_provider_with_signer(&self.provider, signer_private_key);
        let contract = ERC20::new(self.address, signer.clone());
        let mut tx_request = contract.transfer(to, amount).into_transaction_request();
        if let Some(gas_limit) = gas_limit {
            tx_request.set_gas_limit(gas_limit);
        }
        send_transaction_with_gas_bump(signer, tx_request, "transfer").await?;
        Ok(())
    }

    pub async fn approve(
        &self,
        signer_private_key: B256,
        gas_limit: Option<u64>,
        spender: Address,
        amount: U256,
    ) -> Result<(), BlockchainError> {
        let signer = get_provider_with_signer(&self.provider, signer_private_key);
        let contract = ERC20::new(self.address, signer.clone());
        let mut tx_request = contract.approve(spender, amount).into_transaction_request();
        if let Some(gas_limit) = gas_limit {
            tx_request.set_gas_limit(gas_limit);
        }
        send_transaction_with_gas_bump(signer, tx_request, "approve").await?;
        Ok(())
    }

    pub async fn allowance(
        &self,
        owner: Address,
        spender: Address,
    ) -> Result<U256, BlockchainError> {
        let contract = ERC20::new(self.address, self.provider.clone());
        let allowance = contract.allowance(owner, spender).call().await?;
        Ok(allowance)
    }
}
