use alloy::primitives::B256;
use async_trait::async_trait;
use intmax2_zkp::common::signature_content::key_set::KeySet;

use crate::api::error::ServerError;

#[async_trait(?Send)]
pub trait WalletKeyVaultClientInterface: Sync + Send {
    async fn derive_key_from_eth(&self, eth_private_key: B256) -> Result<KeySet, ServerError>;
}
