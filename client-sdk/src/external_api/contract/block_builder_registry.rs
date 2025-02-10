use std::sync::Arc;

use ethers::{
    contract::abigen,
    core::k256::ecdsa::SigningKey,
    middleware::SignerMiddleware,
    providers::{Http, Provider},
    signers::Wallet,
    types::{Address as EthAddress, H256},
};

use super::{
    error::BlockchainError,
    handlers::handle_contract_call,
    proxy_contract::ProxyContract,
    utils::{get_client, get_client_with_signer},
};

abigen!(BlockBuilderRegistry, "abi/BlockBuilderRegistry.json",);

#[derive(Debug, Clone)]
pub struct BlockBuilderRegistryContract {
    pub rpc_url: String,
    pub chain_id: u64,
    pub address: EthAddress,
}

impl BlockBuilderRegistryContract {
    pub fn new(rpc_url: &str, chain_id: u64, address: EthAddress) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            chain_id,
            address,
        }
    }

    pub async fn deploy(rpc_url: &str, chain_id: u64, private_key: H256) -> anyhow::Result<Self> {
        let client = get_client_with_signer(rpc_url, chain_id, private_key).await?;
        let impl_contract = BlockBuilderRegistry::deploy::<()>(Arc::new(client), ())?
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
    ) -> Result<BlockBuilderRegistry<Provider<Http>>, BlockchainError> {
        let client = get_client(&self.rpc_url).await?;
        let contract = BlockBuilderRegistry::new(self.address, client);
        Ok(contract)
    }

    async fn get_contract_with_signer(
        &self,
        private_key: H256,
    ) -> Result<
        BlockBuilderRegistry<SignerMiddleware<Provider<Http>, Wallet<SigningKey>>>,
        BlockchainError,
    > {
        let client = get_client_with_signer(&self.rpc_url, self.chain_id, private_key).await?;
        let contract = BlockBuilderRegistry::new(self.address, Arc::new(client));
        Ok(contract)
    }

    pub async fn emit_heart_beat(
        &self,
        signer_private_key: H256,
        url: &str,
    ) -> Result<(), BlockchainError> {
        let contract = self.get_contract_with_signer(signer_private_key).await?;
        let mut tx = contract.emit_heartbeat(url.to_string());
        let client =
            get_client_with_signer(&self.rpc_url, self.chain_id, signer_private_key).await?;
        handle_contract_call(&client, &mut tx, "emit_heart_beat").await?;
        Ok(())
    }
}
