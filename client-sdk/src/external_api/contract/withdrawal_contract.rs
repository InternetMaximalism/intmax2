use std::sync::Arc;

use ethers::{
    contract::abigen,
    core::k256::ecdsa::SigningKey,
    middleware::SignerMiddleware,
    providers::{Http, Provider},
    signers::Wallet,
    types::{self, H256},
};

use crate::external_api::{contract::utils::get_latest_block_number, utils::retry::with_retry};

use super::{
    error::BlockchainError,
    handlers::handle_contract_call,
    proxy_contract::ProxyContract,
    utils::{get_client, get_client_with_signer},
};

abigen!(WithdrawalAbi, "abi/Withdrawal.json",);

#[derive(Debug, Clone)]
pub struct WithdrawalContract {
    pub rpc_url: String,
    pub chain_id: u64,
    pub address: ethers::types::Address,
}

impl WithdrawalContract {
    pub fn new(rpc_url: &str, chain_id: u64, address: ethers::types::Address) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            chain_id,
            address,
        }
    }

    pub async fn get_eth_block_number(&self) -> Result<u64, BlockchainError> {
        get_latest_block_number(&self.rpc_url).await
    }

    pub async fn deploy(rpc_url: &str, chain_id: u64, private_key: H256) -> anyhow::Result<Self> {
        let client = get_client_with_signer(rpc_url, chain_id, private_key).await?;
        let impl_contract = WithdrawalAbi::deploy::<()>(Arc::new(client), ())?
            .send()
            .await?;
        let impl_address = impl_contract.address();
        let proxy =
            ProxyContract::deploy(rpc_url, chain_id, private_key, impl_address, &[]).await?;
        let address = proxy.address();
        Ok(Self::new(rpc_url, chain_id, address))
    }

    pub fn address(&self) -> ethers::types::Address {
        self.address
    }

    pub async fn get_contract(
        &self,
    ) -> Result<withdrawal_abi::WithdrawalAbi<Provider<Http>>, BlockchainError> {
        let client = get_client(&self.rpc_url).await?;
        let contract = WithdrawalAbi::new(self.address, client);
        Ok(contract)
    }

    pub async fn get_contract_with_signer(
        &self,
        private_key: H256,
    ) -> Result<
        withdrawal_abi::WithdrawalAbi<SignerMiddleware<Provider<Http>, Wallet<SigningKey>>>,
        BlockchainError,
    > {
        let client = get_client_with_signer(&self.rpc_url, self.chain_id, private_key).await?;
        let contract = WithdrawalAbi::new(self.address, Arc::new(client));
        Ok(contract)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn initialize(
        &self,
        signer_private_key: H256,
        admin: types::Address,
        scroll_messenger_address: types::Address,
        withdrawal_verifier_address: types::Address,
        liquidity_address: types::Address,
        rollup_address: types::Address,
        contribution_address: types::Address,
        direct_withdrawal_token_indices: Vec<types::U256>,
    ) -> Result<H256, BlockchainError> {
        let contract = self.get_contract_with_signer(signer_private_key).await?;
        let mut tx = contract.initialize(
            admin,
            scroll_messenger_address,
            withdrawal_verifier_address,
            liquidity_address,
            rollup_address,
            contribution_address,
            direct_withdrawal_token_indices,
        );
        let client =
            get_client_with_signer(&self.rpc_url, self.chain_id, signer_private_key).await?;
        let tx_hash = handle_contract_call(&client, &mut tx, "initialize", None).await?;
        Ok(tx_hash)
    }

    pub async fn get_direct_withdrawal_token_indices(&self) -> Result<Vec<u32>, BlockchainError> {
        let contract = self.get_contract().await?;
        let direct_withdrawal_token_indices: Vec<types::U256> =
            with_retry(|| async { contract.get_direct_withdrawal_token_indices().call().await })
                .await
                .map_err(|_| {
                    BlockchainError::RPCError(
                        "failed to get direct withdrawal token indices".to_string(),
                    )
                })?;
        let direct_withdrawal_token_indices = direct_withdrawal_token_indices
            .iter()
            .map(|index| index.as_u32())
            .collect();
        Ok(direct_withdrawal_token_indices)
    }
}
