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

abigen!(ERC20, "abi/TestERC20.json",);

#[derive(Debug, Clone)]
pub struct ERC20Contract {
    pub rpc_url: String,
    pub chain_id: u64,
    pub address: Address,
}

impl ERC20Contract {
    pub fn new(rpc_url: &str, chain_id: u64, address: Address) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            chain_id,
            address,
        }
    }

    pub async fn deploy(
        rpc_url: &str,
        chain_id: u64,
        private_key: H256,
        initial_address: Address,
    ) -> anyhow::Result<Self> {
        let client = get_client_with_signer(rpc_url, chain_id, private_key).await?;
        let erc20_contract = ERC20::deploy::<Address>(Arc::new(client), initial_address)?
            .send()
            .await?;
        let address = erc20_contract.address();
        Ok(Self::new(rpc_url, chain_id, address))
    }

    pub fn address(&self) -> Address {
        self.address
    }

    pub async fn get_contract(&self) -> Result<ERC20<Provider<Http>>, BlockchainError> {
        let client = get_client(&self.rpc_url).await?;
        let contract = ERC20::new(self.address, client);
        Ok(contract)
    }

    async fn get_contract_with_signer(
        &self,
        private_key: H256,
    ) -> Result<ERC20<SignerMiddleware<Provider<Http>, Wallet<SigningKey>>>, BlockchainError> {
        let client = get_client_with_signer(&self.rpc_url, self.chain_id, private_key).await?;
        let contract = ERC20::new(self.address, Arc::new(client));
        Ok(contract)
    }

    pub async fn balance_of(&self, account: Address) -> Result<U256, BlockchainError> {
        let contract = self.get_contract().await?;
        let balance = with_retry(|| async { contract.balance_of(account).call().await })
            .await
            .map_err(|e| BlockchainError::RPCError(format!("Failed to get balance: {}", e)))?;
        Ok(balance)
    }

    pub async fn transfer(
        &self,
        signer_private_key: H256,
        gas_limit: Option<u64>,
        to: Address,
        amount: U256,
    ) -> Result<(), BlockchainError> {
        let contract = self.get_contract_with_signer(signer_private_key).await?;
        let mut tx = contract.transfer(to, amount);
        let client =
            get_client_with_signer(&self.rpc_url, self.chain_id, signer_private_key).await?;
        handle_contract_call(&client, &mut tx, "transfer", gas_limit).await?;
        Ok(())
    }

    pub async fn approve(
        &self,
        signer_private_key: H256,
        gas_limit: Option<u64>,
        spender: Address,
        amount: U256,
    ) -> Result<(), BlockchainError> {
        let contract = self.get_contract_with_signer(signer_private_key).await?;
        let mut tx = contract.approve(spender, amount);
        let client =
            get_client_with_signer(&self.rpc_url, self.chain_id, signer_private_key).await?;
        handle_contract_call(&client, &mut tx, "approve", gas_limit).await?;
        Ok(())
    }

    pub async fn allowance(
        &self,
        owner: Address,
        spender: Address,
    ) -> Result<U256, BlockchainError> {
        let contract = self.get_contract().await?;
        let allowance = with_retry(|| async { contract.allowance(owner, spender).call().await })
            .await
            .map_err(|e| BlockchainError::RPCError(format!("Failed to get allowance: {}", e)))?;
        Ok(allowance)
    }
}
