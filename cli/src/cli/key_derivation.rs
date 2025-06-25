use super::error::CliError;
use crate::env_var::EnvVar;
use alloy::primitives::B256;
use intmax2_client_sdk::{
    client::config::env_type_to_network,
    external_api::wallet_key_vault::{mnemonic_to_spend_key, WalletKeyVaultClient},
};
use intmax2_interfaces::{
    api::wallet_key_vault::interface::WalletKeyVaultClientInterface, utils::key::PrivateKey,
};

pub async fn derive_key_from_eth_with_env(
    eth_private_key: B256,
    redeposit_index: u32,
    wallet_index: u32,
    env: &EnvVar,
) -> Result<PrivateKey, CliError> {
    let base_url = match &env.wallet_key_vault_base_url {
        Some(url) => url,
        None => {
            return Err(CliError::EnvError(
                "Wallet key vault base URL is not set in environment".into(),
            ))
        }
    };
    let network = env_type_to_network(env.env);
    let client = WalletKeyVaultClient::new(base_url.clone(), network);
    let mnemonic = client.derive_mnemonic(eth_private_key).await?;
    let spend_key = mnemonic_to_spend_key(&mnemonic, redeposit_index, wallet_index);

    Ok(spend_key)
}

pub async fn derive_spend_key_from_eth(
    eth_private_key: B256,
    redeposit_index: u32,
    wallet_index: u32,
) -> Result<PrivateKey, CliError> {
    let env = envy::from_env::<EnvVar>()?;
    if env.wallet_key_vault_base_url.is_none() {
        return Err(CliError::EnvError(
            "Wallet key vault base URL is not set".to_string(),
        ));
    }
    let network = env_type_to_network(env.env);
    let client = WalletKeyVaultClient::new(env.wallet_key_vault_base_url.unwrap(), network);
    let mnemonic = client.derive_mnemonic(eth_private_key).await?;
    let spend_key = mnemonic_to_spend_key(&mnemonic, redeposit_index, wallet_index);
    Ok(spend_key)
}
