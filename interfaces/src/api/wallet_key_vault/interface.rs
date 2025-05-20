use alloy::{
    primitives::B256,
    signers::local::coins_bip39::{English, Mnemonic},
};
use async_trait::async_trait;

use crate::api::error::ServerError;

#[async_trait(?Send)]
pub trait WalletKeyVaultClientInterface: Sync + Send {
    async fn derive_mnemonic(
        &self,
        eth_private_key: B256,
    ) -> Result<Mnemonic<English>, ServerError>;
}
