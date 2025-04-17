use intmax2_client_sdk::{
    client::{client::Client, config::ClientConfig},
    external_api::{
        balance_prover::BalanceProverClient,
        block_builder::BlockBuilderClient,
        contract::{
            liquidity_contract::LiquidityContract, rollup_contract::RollupContract,
            withdrawal_contract::WithdrawalContract,
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
    store_vault_server::interface::StoreVaultClientInterface,
};

use crate::env_var::EnvVar;

use super::error::CliError;

pub fn get_client() -> Result<Client, CliError> {
    let env = envy::from_env::<EnvVar>()?;
    let block_builder = Box::new(BlockBuilderClient::new());

    let use_s3 = env.use_s3.unwrap_or(true);
    let store_vault_server: Box<dyn StoreVaultClientInterface> = if use_s3 {
        Box::new(S3StoreVaultClient::new(&env.store_vault_server_base_url))
    } else {
        Box::new(StoreVaultServerClient::new(
            &env.store_vault_server_base_url,
        ))
    };

    let validity_prover = Box::new(ValidityProverClient::new(&env.validity_prover_base_url));
    let balance_prover: Box<dyn BalanceProverClientInterface> =
        if env.use_private_zkp_server.unwrap_or(true) {
            let private_zkp_server_config = PrivateZKPServerConfig {
                max_retries: env.private_zkp_server_max_retires.unwrap_or(30),
                retry_interval: env.private_zkp_server_retry_interval.unwrap_or(5),
            };
            Box::new(PrivateZKPServerClient::new(
                &env.balance_prover_base_url,
                &private_zkp_server_config,
            ))
        } else {
            Box::new(BalanceProverClient::new(&env.balance_prover_base_url))
        };
    let withdrawal_server = Box::new(WithdrawalServerClient::new(&env.withdrawal_server_base_url));

    let liquidity_contract = LiquidityContract::new(
        &env.l1_rpc_url,
        env.l1_chain_id,
        env.liquidity_contract_address,
        0,
    );
    let rollup_contract = RollupContract::new(
        &env.l2_rpc_url,
        env.l2_chain_id,
        env.rollup_contract_address,
        0,
    );
    let withdrawal_contract = WithdrawalContract::new(
        &env.l2_rpc_url,
        env.l2_chain_id,
        env.withdrawal_contract_address,
    );

    let config = ClientConfig {
        deposit_timeout: env.deposit_timeout,
        tx_timeout: env.tx_timeout,
        block_builder_request_interval: env.block_builder_request_interval,
        block_builder_request_limit: env.block_builder_request_limit,
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
