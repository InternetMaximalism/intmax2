use super::{
    error::BlockchainError,
    handlers::send_transaction_with_gas_bump,
    utils::{get_provider_with_signer, NormalProvider},
};
use alloy::{
    network::TransactionBuilder,
    primitives::{Address, Bytes, B256, U256},
    sol,
};

sol!(
    #[sol(rpc)]
    ERC1155,
    "abi/TestERC1155.json",
);

#[derive(Debug, Clone)]
pub struct ERC1155Contract {
    pub provider: NormalProvider,
    pub address: Address,
}

impl ERC1155Contract {
    pub fn new(provider: NormalProvider, address: Address) -> Self {
        Self { provider, address }
    }

    pub async fn deploy(provider: NormalProvider, private_key: B256) -> anyhow::Result<Self> {
        let signer = get_provider_with_signer(&provider, private_key);
        let contract = ERC1155::deploy(signer).await?;
        let address = *contract.address();
        Ok(Self { provider, address })
    }

    // this is for test method
    pub async fn setup(&self, private_key: B256) -> Result<(), BlockchainError> {
        let signer = get_provider_with_signer(&self.provider, private_key);
        let contract = ERC1155::new(self.address, signer.clone());

        // Get the signer's address from the private key
        let private_key_signer =
            alloy::signers::local::PrivateKeySigner::from_bytes(&private_key).unwrap();
        let address = private_key_signer.address();

        let token_id = U256::from(0);
        let amount = U256::from(100);
        let tx_request = contract
            .mint(address, token_id, amount, Bytes::default())
            .into_transaction_request();
        send_transaction_with_gas_bump(signer, tx_request, "mint").await?;
        Ok(())
    }

    pub async fn balance_of(
        &self,
        account: Address,
        token_id: U256,
    ) -> Result<U256, BlockchainError> {
        let contract = ERC1155::new(self.address, self.provider.clone());
        let balance = contract.balanceOf(account, token_id).call().await?;
        Ok(balance)
    }

    pub async fn transfer_from(
        &self,
        signer_private_key: B256,
        gas_limit: Option<u64>,
        from: Address,
        to: Address,
        token_id: U256,
        amount: U256,
    ) -> Result<(), BlockchainError> {
        let signer = get_provider_with_signer(&self.provider, signer_private_key);
        let contract = ERC1155::new(self.address, signer.clone());
        let mut tx_request = contract
            .safeTransferFrom(from, to, token_id, amount, Bytes::default())
            .into_transaction_request();
        if let Some(gas_limit) = gas_limit {
            tx_request.set_gas_limit(gas_limit);
        }
        send_transaction_with_gas_bump(signer, tx_request, "transfer_from").await?;
        Ok(())
    }

    pub async fn set_approval_for_all(
        &self,
        signer_private_key: B256,
        gas_limit: Option<u64>,
        operator: Address,
        approved: bool,
    ) -> Result<(), BlockchainError> {
        let signer = get_provider_with_signer(&self.provider, signer_private_key);
        let contract = ERC1155::new(self.address, signer.clone());
        let mut tx_request = contract
            .setApprovalForAll(operator, approved)
            .into_transaction_request();
        if let Some(gas_limit) = gas_limit {
            tx_request.set_gas_limit(gas_limit);
        }
        send_transaction_with_gas_bump(signer, tx_request, "set_approval_for_all").await?;
        Ok(())
    }

    pub async fn is_approved_for_all(
        &self,
        account: Address,
        operator: Address,
    ) -> Result<bool, BlockchainError> {
        let contract = ERC1155::new(self.address, self.provider.clone());
        let approved = contract.isApprovedForAll(account, operator).call().await?;
        Ok(approved)
    }
}
