use alloy::primitives::U256;
use intmax2_client_sdk::external_api::contract::{
    block_builder_reward::BlockBuilderRewardContract,
    convert::{convert_address_to_alloy, convert_bytes32_to_b256},
    utils::get_address_from_private_key,
};
use intmax2_interfaces::{
    api::withdrawal_server::interface::WithdrawalStatus, utils::key::ViewPair,
};
use intmax2_zkp::ethereum_types::{address::Address, bytes32::Bytes32};

use crate::{cli::client::get_client, env_var::EnvVar};

use super::error::CliError;

pub async fn claim_withdrawals(
    view_pair: ViewPair,
    eth_private_key: Bytes32,
) -> Result<(), CliError> {
    let signer_private_key = convert_bytes32_to_b256(eth_private_key);
    let client = get_client()?;
    let withdrawal_infos = client.get_withdrawal_info(view_pair.view).await?;

    let mut claim_withdrawals = Vec::new();

    for info in &withdrawal_infos {
        if info.status != WithdrawalStatus::NeedClaim {
            continue;
        }

        let withdrawal = info.contract_withdrawal.clone();
        let withdrawal_hash = withdrawal.withdrawal_hash();

        if client
            .liquidity_contract
            .check_if_claimable(withdrawal_hash)
            .await?
        {
            log::info!(
                    "Withdrawal to claim #{}: recipient: {}, token_index: {}, amount: {}, withdrawal_hash: {}",
                    claim_withdrawals.len(),
                    withdrawal.recipient,
                    withdrawal.token_index,
                    withdrawal.amount,
                    withdrawal_hash
                );
            claim_withdrawals.push(withdrawal);
        }
    }

    if claim_withdrawals.is_empty() {
        println!("No withdrawals to claim");
        return Ok(());
    }

    client
        .liquidity_contract
        .claim_withdrawals(signer_private_key, None, &claim_withdrawals)
        .await?;

    Ok(())
}

pub async fn claim_builder_reward(eth_private_key: Bytes32) -> Result<(), CliError> {
    let env = envy::from_env::<EnvVar>()?;

    let signer_private_key = convert_bytes32_to_b256(eth_private_key);
    let user_address = get_address_from_private_key(signer_private_key);
    log::info!("Claiming block builder reward for user address: {user_address}");

    let reward_address = env
        .reward_contract_address
        .ok_or_else(|| CliError::EnvError("REWARD_CONTRACT_ADDRESS is not set".to_string()))?;

    let reward_contract = build_reward_contract(reward_address).await?;
    let claimable_periods = fetch_claimable_periods(&reward_contract, user_address).await?;

    if claimable_periods.is_empty() {
        println!("No block builder rewards to claim");
        return Ok(());
    }

    log::info!("Claiming block builder rewards for periods: {claimable_periods:?}");
    reward_contract
        .batch_claim_reward(signer_private_key, None, &claimable_periods)
        .await?;

    Ok(())
}

async fn build_reward_contract(
    reward_address: Address,
) -> Result<BlockBuilderRewardContract, CliError> {
    let client = get_client()?;
    let provider = client.rollup_contract.provider.clone();
    let address = convert_address_to_alloy(reward_address);
    Ok(BlockBuilderRewardContract::new(provider, address))
}

async fn fetch_claimable_periods(
    contract: &BlockBuilderRewardContract,
    user_address: alloy::primitives::Address,
) -> Result<Vec<u64>, CliError> {
    let current_period = contract.get_current_period().await?;
    log::info!("Current reward period: {current_period}");

    let mut claimable_periods = Vec::new();

    for period_index in 0..current_period {
        let reward = contract
            .get_claimable_reward(period_index, user_address)
            .await?;
        if reward > U256::ZERO {
            log::info!("Claiming block builder reward for period {period_index}: {reward}");
            claimable_periods.push(period_index);
        }
    }

    Ok(claimable_periods)
}
