use super::utils::{get_provider_with_signer, NormalProvider};
use alloy::{
    primitives::{Address, Bytes, B256},
    providers::Provider as _,
    sol,
};

sol!(
    #[sol(rpc)]
    ERC1967Proxy,
    "abi/ERC1967Proxy.json",
);

pub struct ProxyContract {
    pub provider: NormalProvider,
    pub address: Address,
    pub deployed_block_number: u64,
}

impl ProxyContract {
    pub async fn deploy(
        provider: NormalProvider,
        private_key: B256,
        impl_address: Address,
        constructor: &[u8],
    ) -> anyhow::Result<ProxyContract> {
        let signer = get_provider_with_signer(&provider, private_key);
        let contract =
            ERC1967Proxy::deploy(signer, impl_address, Bytes::from(constructor.to_vec())).await?;
        let deployed_block_number = provider.get_block_number().await?;
        Ok(ProxyContract {
            provider,
            address: *contract.address(),
            deployed_block_number,
        })
    }
}
