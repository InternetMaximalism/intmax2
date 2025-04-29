use std::sync::Arc;

use ethers::{
    contract::abigen,
    core::k256::ecdsa::SigningKey,
    middleware::SignerMiddleware,
    providers::{Http, Provider},
    signers::Wallet,
    types::{Address, H256, U256},
};

use crate::external_api::utils::retry::with_retry;

use super::{
    error::BlockchainError,
    handlers::handle_contract_call,
    utils::{get_client, get_client_with_signer},
};

abigen!(ERC721, "abi/TestNFT.json",);

#[derive(Debug, Clone)]
pub struct ERC721Contract {
    pub rpc_url: String,
    pub chain_id: u64,
    pub address: Address,
}

impl ERC721Contract {
    pub fn new(rpc_url: &str, chain_id: u64, address: Address) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            chain_id,
            address,
        }
    }

    pub async fn deploy(rpc_url: &str, chain_id: u64, private_key: H256) -> anyhow::Result<Self> {
        let client = get_client_with_signer(rpc_url, chain_id, private_key).await?;
        let erc721_contract = ERC721::deploy::<()>(Arc::new(client), ())?.send().await?;
        let address = erc721_contract.address();
        Ok(Self::new(rpc_url, chain_id, address))
    }

    pub fn address(&self) -> Address {
        self.address
    }

    pub async fn get_contract(&self) -> Result<ERC721<Provider<Http>>, BlockchainError> {
        let client = get_client(&self.rpc_url).await?;
        let contract = ERC721::new(self.address, client);
        Ok(contract)
    }

    async fn get_contract_with_signer(
        &self,
        private_key: H256,
    ) -> Result<ERC721<SignerMiddleware<Provider<Http>, Wallet<SigningKey>>>, BlockchainError> {
        let client = get_client_with_signer(&self.rpc_url, self.chain_id, private_key).await?;
        let contract = ERC721::new(self.address, Arc::new(client));
        Ok(contract)
    }

    pub async fn balance_of(&self, account: Address) -> Result<U256, BlockchainError> {
        let contract = self.get_contract().await?;
        let balance = with_retry(|| async { contract.balance_of(account).call().await })
            .await
            .map_err(|e| BlockchainError::RPCError(format!("Failed to get balance: {}", e)))?;
        Ok(balance)
    }

    pub async fn owner_of(&self, token_id: U256) -> Result<Address, BlockchainError> {
        let contract = self.get_contract().await?;
        let owner = with_retry(|| async { contract.owner_of(token_id).call().await })
            .await
            .map_err(|e| BlockchainError::RPCError(format!("Failed to get balance: {}", e)))?;
        Ok(owner)
    }

    pub async fn transfer_from(
        &self,
        signer_private_key: H256,
        gas_limit: Option<u64>,
        from: Address,
        to: Address,
        token_id: U256,
    ) -> Result<(), BlockchainError> {
        let contract = self.get_contract_with_signer(signer_private_key).await?;
        let mut tx = contract.transfer_from(from, to, token_id);
        let client =
            get_client_with_signer(&self.rpc_url, self.chain_id, signer_private_key).await?;
        handle_contract_call(&client, &mut tx, "transfer_from", gas_limit).await?;
        Ok(())
    }

    pub async fn approve(
        &self,
        signer_private_key: H256,
        gas_limit: Option<u64>,
        to: Address,
        token_id: U256,
    ) -> Result<(), BlockchainError> {
        let contract = self.get_contract_with_signer(signer_private_key).await?;
        let mut tx = contract.approve(to, token_id);
        let client =
            get_client_with_signer(&self.rpc_url, self.chain_id, signer_private_key).await?;
        handle_contract_call(&client, &mut tx, "approve", gas_limit).await?;
        Ok(())
    }

    pub async fn get_approved(&self, token_id: U256) -> Result<Address, BlockchainError> {
        let contract = self.get_contract().await?;
        let account = with_retry(|| async { contract.get_approved(token_id).call().await })
            .await
            .map_err(|e| BlockchainError::RPCError(format!("Failed to get approved: {}", e)))?;
        Ok(account)
    }
}
