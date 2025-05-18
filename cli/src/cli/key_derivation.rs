use alloy::primitives::B256;
use intmax2_client_sdk::external_api::wallet_key_vault::WalletKeyVaultClient;
use intmax2_interfaces::api::wallet_key_vault::interface::WalletKeyVaultClientInterface;
use intmax2_zkp::common::signature_content::key_set::KeySet;

use crate::env_var::EnvVar;

use super::error::CliError;

pub async fn derive_key_from_eth(eth_private_key: B256) -> Result<KeySet, CliError> {
    let env = envy::from_env::<EnvVar>()?;
    if env.wallet_key_vault_base_url.is_none() {
        return Err(CliError::EnvError(
            "Wallet key vault base URL is not set".to_string(),
        ));
    }
    let client = WalletKeyVaultClient::new(env.wallet_key_vault_base_url.unwrap());
    let key = client.derive_key_from_eth(eth_private_key).await?;
    Ok(key)
}
