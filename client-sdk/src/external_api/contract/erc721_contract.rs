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
    ERC721,
    "abi/TestNFT.json",
);

#[derive(Debug, Clone)]
pub struct ERC721Contract {
    pub provider: NormalProvider,
    pub address: Address,
}

impl ERC721Contract {
    pub fn new(provider: NormalProvider, address: Address) -> Self {
        Self { provider, address }
    }

    pub async fn deploy(provider: NormalProvider, private_key: B256) -> anyhow::Result<Self> {
        let signer = get_provider_with_signer(&provider, private_key);
        let contract = ERC721::deploy(signer).await?;
        let address = *contract.address();
        Ok(Self { provider, address })
    }

    pub async fn balance_of(&self, account: Address) -> Result<U256, BlockchainError> {
        let contract = ERC721::new(self.address, self.provider.clone());
        let balance = contract.balanceOf(account).call().await?;
        Ok(balance)
    }

    pub async fn owner_of(&self, token_id: U256) -> Result<Address, BlockchainError> {
        let contract = ERC721::new(self.address, self.provider.clone());
        let owner = contract.ownerOf(token_id).call().await?;
        Ok(owner)
    }

    pub async fn transfer_from(
        &self,
        signer_private_key: B256,
        gas_limit: Option<u64>,
        from: Address,
        to: Address,
        token_id: U256,
    ) -> Result<(), BlockchainError> {
        let signer = get_provider_with_signer(&self.provider, signer_private_key);
        let contract = ERC721::new(self.address, signer.clone());
        let mut tx_request = contract
            .transferFrom(from, to, token_id)
            .into_transaction_request();
        if let Some(gas_limit) = gas_limit {
            tx_request.set_gas_limit(gas_limit);
        }
        send_transaction_with_gas_bump(signer, tx_request, "transfer_from").await?;
        Ok(())
    }

    pub async fn approve(
        &self,
        signer_private_key: B256,
        gas_limit: Option<u64>,
        to: Address,
        token_id: U256,
    ) -> Result<(), BlockchainError> {
        let signer = get_provider_with_signer(&self.provider, signer_private_key);
        let contract = ERC721::new(self.address, signer.clone());
        let mut tx_request = contract.approve(to, token_id).into_transaction_request();
        if let Some(gas_limit) = gas_limit {
            tx_request.set_gas_limit(gas_limit);
        }
        send_transaction_with_gas_bump(signer, tx_request, "approve").await?;
        Ok(())
    }

    pub async fn get_approved(&self, token_id: U256) -> Result<Address, BlockchainError> {
        let contract = ERC721::new(self.address, self.provider.clone());
        let account = contract.getApproved(token_id).call().await?;
        Ok(account)
    }
}
