use std::{path::PathBuf, sync::Arc};

use intmax2_client_sdk::{
    client::{client::Client, config::ClientConfig},
    external_api::{
        balance_prover::BalanceProverClient,
        block_builder::BlockBuilderClient,
        contract::{
            convert::convert_address_to_alloy, liquidity_contract::LiquidityContract,
            rollup_contract::RollupContract, utils::get_provider_with_fallback,
            withdrawal_contract::WithdrawalContract,
        },
        local_backup_store_vault::{
            local_store_vault::LocalStoreVaultClient, LocalBackupStoreVaultClient,
        },
        private_zkp_server::{PrivateZKPServerClient, PrivateZKPServerConfig},
        s3_store_vault::S3StoreVaultClient,
        store_vault_server::StoreVaultServerClient,
        validity_prover::ValidityProverClient,
        withdrawal_server::WithdrawalServerClient,
    },
};
use intmax2_interfaces::api::{
    balance_prover::interface::BalanceProverClientInterface,
    store_vault_server::{interface::StoreVaultClientInterface, types::StoreVaultType},
};

use crate::env_var::EnvVar;

use super::error::CliError;

pub fn get_client() -> Result<Client, CliError> {
    let env = envy::from_env::<EnvVar>()?;
    let root_path = get_backup_root_path(&env)?;

    let block_builder = Box::new(BlockBuilderClient::new());
    let store_vault_server = build_store_vault(&env, root_path)?;
    let validity_prover = Box::new(ValidityProverClient::new(&env.validity_prover_base_url));
    let balance_prover = build_balance_prover(&env)?;
    let withdrawal_server = Box::new(WithdrawalServerClient::new(&env.withdrawal_server_base_url));
    let (liquidity_contract, rollup_contract, withdrawal_contract) = build_contracts(&env)?;

    let config = ClientConfig {
        deposit_timeout: env.deposit_timeout,
        tx_timeout: env.tx_timeout,
        block_builder_query_wait_time: env.block_builder_query_wait_time,
        block_builder_query_interval: env.block_builder_query_interval,
        block_builder_query_limit: env.block_builder_query_limit,
        is_faster_mining: env.is_faster_mining,
    };

    let client = Client {
        block_builder,
        store_vault_server,
        validity_prover,
        balance_prover,
        withdrawal_server,
        liquidity_contract,
        rollup_contract,
        withdrawal_contract,
        config,
    };

    Ok(client)
}

pub fn get_backup_root_path(env: &EnvVar) -> Result<PathBuf, CliError> {
    Ok(env
        .local_backup_path
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join(".intmax2/backup")
        }))
}

fn build_store_vault(
    env: &EnvVar,
    root_path: PathBuf,
) -> Result<Box<dyn StoreVaultClientInterface>, CliError> {
    use StoreVaultType::*;

    if env.store_vault_type != Local && env.store_vault_server_base_url.is_none() {
        return Err(CliError::EnvError(
            "store_vault_server_base_url is required".to_string(),
        ));
    }

    // Got default url from .env.example
    let default_url = "https://stage.api.node.intmax.io/store-vault-server".to_string();
    let url = env
        .store_vault_server_base_url
        .as_ref()
        .unwrap_or(&default_url);

    Ok(match env.store_vault_type {
        Local => Box::new(LocalStoreVaultClient::new(root_path)),
        LegacyRemote => Box::new(StoreVaultServerClient::new(url)),
        Remote => Box::new(S3StoreVaultClient::new(url)),
        RemoteWithBackup => {
            let inner = Box::new(S3StoreVaultClient::new(url));
            Box::new(LocalBackupStoreVaultClient::new(Arc::new(inner), root_path))
        }
        LegacyRemoteWithBackup => {
            let inner = Box::new(StoreVaultServerClient::new(url));
            Box::new(LocalBackupStoreVaultClient::new(Arc::new(inner), root_path))
        }
    })
}

fn build_balance_prover(env: &EnvVar) -> Result<Box<dyn BalanceProverClientInterface>, CliError> {
    if env.use_private_zkp_server.unwrap_or(true) {
        let config = PrivateZKPServerConfig {
            max_retries: env.private_zkp_server_max_retires.unwrap_or(30),
            retry_interval: env.private_zkp_server_retry_interval.unwrap_or(5),
        };
        Ok(Box::new(PrivateZKPServerClient::new(
            &env.balance_prover_base_url,
            &config,
        )))
    } else {
        Ok(Box::new(BalanceProverClient::new(
            &env.balance_prover_base_url,
        )))
    }
}

fn build_contracts(
    env: &EnvVar,
) -> Result<(LiquidityContract, RollupContract, WithdrawalContract), CliError> {
    let l1_provider = get_provider_with_fallback(std::slice::from_ref(&env.l1_rpc_url))?;
    let l2_provider = get_provider_with_fallback(std::slice::from_ref(&env.l2_rpc_url))?;

    let liquidity_contract = LiquidityContract::new(
        l1_provider,
        convert_address_to_alloy(env.liquidity_contract_address),
    );
    let rollup_contract = RollupContract::new(
        l2_provider.clone(),
        convert_address_to_alloy(env.rollup_contract_address),
    );
    let withdrawal_contract = WithdrawalContract::new(
        l2_provider,
        convert_address_to_alloy(env.withdrawal_contract_address),
    );

    Ok((liquidity_contract, rollup_contract, withdrawal_contract))
}
