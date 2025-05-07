use super::{
    error::BlockchainError,
    handlers::send_transaction_with_gas_bump,
    proxy_contract::ProxyContract,
    utils::{get_provider_with_signer, NormalProvider},
};
use alloy::{
    network::TransactionBuilder,
    primitives::{Address, B256},
    sol,
};

sol!(
    #[sol(rpc)]
    BlockBuilderRegistry,
    "abi/BlockBuilderRegistry.json",
);

#[derive(Debug, Clone)]
pub struct BlockBuilderRegistryContract {
    pub provider: NormalProvider,
    pub address: Address,
}

impl BlockBuilderRegistryContract {
    pub fn new(provider: NormalProvider, address: Address) -> Self {
        Self { provider, address }
    }

    pub async fn deploy(provider: NormalProvider, private_key: B256) -> anyhow::Result<Self> {
        let signer = get_provider_with_signer(&provider, private_key);
        let impl_contract = BlockBuilderRegistry::deploy(signer).await?;
        let impl_address = *impl_contract.address();
        let proxy = ProxyContract::deploy(provider.clone(), private_key, impl_address, &[]).await?;
        Ok(Self {
            provider,
            address: proxy.address,
        })
    }

    pub async fn emit_heart_beat(
        &self,
        signer_private_key: B256,
        gas_limit: Option<u64>,
        url: &str,
    ) -> Result<(), BlockchainError> {
        let signer = get_provider_with_signer(&self.provider, signer_private_key);
        let contract = BlockBuilderRegistry::new(self.address, signer.clone());
        let mut tx_request = contract
            .emitHeartbeat(url.to_string())
            .into_transaction_request();
        if let Some(gas_limit) = gas_limit {
            tx_request.set_gas_limit(gas_limit);
        }
        send_transaction_with_gas_bump(signer, tx_request, "emit_heart_beat").await?;
        Ok(())
    }
}
