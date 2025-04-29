use std::sync::Arc;

use ethers::{
    contract::abigen,
    core::k256::ecdsa::SigningKey,
    middleware::SignerMiddleware,
    providers::{Http, Provider},
    signers::Wallet,
    types::{Address, Bytes, H256, U256},
};

use crate::external_api::utils::retry::with_retry;

use super::{
    error::BlockchainError,
    handlers::handle_contract_call,
    utils::{get_address, get_client, get_client_with_signer},
};

abigen!(ERC1155, "abi/TestERC1155.json",);

#[derive(Debug, Clone)]
pub struct ERC1155Contract {
    pub rpc_url: String,
    pub chain_id: u64,
    pub address: Address,
}

impl ERC1155Contract {
    pub fn new(rpc_url: &str, chain_id: u64, address: Address) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            chain_id,
            address,
        }
    }

    pub async fn deploy(rpc_url: &str, chain_id: u64, private_key: H256) -> anyhow::Result<Self> {
        let client = get_client_with_signer(rpc_url, chain_id, private_key).await?;
        let erc1155_contract = ERC1155::deploy::<()>(Arc::new(client), ())?.send().await?;
        let address = erc1155_contract.address();
        Ok(Self::new(rpc_url, chain_id, address))
    }

    // this is for test method
    pub async fn setup(&self, private_key: H256) -> Result<(), BlockchainError> {
        let contract = self.get_contract_with_signer(private_key).await?;
        let address = get_address(self.chain_id, private_key);
        let mut tx = contract.mint(address, 0.into(), 100.into(), Bytes::default());
        let client = get_client_with_signer(&self.rpc_url, self.chain_id, private_key).await?;
        handle_contract_call(&client, &mut tx, "mint", None).await?;
        Ok(())
    }

    pub fn address(&self) -> Address {
        self.address
    }

    pub async fn get_contract(&self) -> Result<ERC1155<Provider<Http>>, BlockchainError> {
        let client = get_client(&self.rpc_url).await?;
        let contract = ERC1155::new(self.address, client);
        Ok(contract)
    }

    async fn get_contract_with_signer(
        &self,
        private_key: H256,
    ) -> Result<ERC1155<SignerMiddleware<Provider<Http>, Wallet<SigningKey>>>, BlockchainError>
    {
        let client = get_client_with_signer(&self.rpc_url, self.chain_id, private_key).await?;
        let contract = ERC1155::new(self.address, Arc::new(client));
        Ok(contract)
    }

    pub async fn balance_of(
        &self,
        account: Address,
        token_id: U256,
    ) -> Result<U256, BlockchainError> {
        let contract = self.get_contract().await?;
        let balance = with_retry(|| async { contract.balance_of(account, token_id).call().await })
            .await
            .map_err(|e| BlockchainError::RPCError(format!("Failed to get balance: {}", e)))?;
        Ok(balance)
    }

    pub async fn transfer_from(
        &self,
        signer_private_key: H256,
        gas_limit: Option<u64>,
        from: Address,
        to: Address,
        token_id: U256,
        amount: U256,
    ) -> Result<(), BlockchainError> {
        let contract = self.get_contract_with_signer(signer_private_key).await?;
        let mut tx = contract.safe_transfer_from(from, to, token_id, amount, Bytes::default());
        let client =
            get_client_with_signer(&self.rpc_url, self.chain_id, signer_private_key).await?;
        handle_contract_call(&client, &mut tx, "transfer_from", gas_limit).await?;
        Ok(())
    }

    pub async fn set_approval_for_all(
        &self,
        signer_private_key: H256,
        gas_limit: Option<u64>,
        operator: Address,
        approved: bool,
    ) -> Result<(), BlockchainError> {
        let contract = self.get_contract_with_signer(signer_private_key).await?;
        let mut tx = contract.set_approval_for_all(operator, approved);
        let client =
            get_client_with_signer(&self.rpc_url, self.chain_id, signer_private_key).await?;
        handle_contract_call(&client, &mut tx, "set_approval_for_all", gas_limit).await?;
        Ok(())
    }

    pub async fn is_approved_for_all(
        &self,
        account: Address,
        operator: Address,
    ) -> Result<bool, BlockchainError> {
        let contract = self.get_contract().await?;
        let account =
            with_retry(|| async { contract.is_approved_for_all(account, operator).call().await })
                .await
                .map_err(|e| BlockchainError::RPCError(format!("Failed to get approved: {}", e)))?;
        Ok(account)
    }
}
