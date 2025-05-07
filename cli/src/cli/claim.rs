use alloy::primitives::U256;
use intmax2_client_sdk::external_api::contract::{
    block_builder_reward::BlockBuilderRewardContract,
    convert::{convert_address_to_alloy, convert_bytes32_to_b256},
    utils::get_address_from_private_key,
};
use intmax2_interfaces::api::withdrawal_server::interface::WithdrawalStatus;
use intmax2_zkp::{common::signature_content::key_set::KeySet, ethereum_types::bytes32::Bytes32};

use crate::{cli::client::get_client, env_var::EnvVar};

use super::error::CliError;

pub async fn claim_withdrawals(key: KeySet, eth_private_key: Bytes32) -> Result<(), CliError> {
    let signer_private_key = convert_bytes32_to_b256(eth_private_key);
    let client = get_client()?;
    let withdrawal_info = client.get_withdrawal_info(key).await?;
    let mut claim_withdrawals = Vec::new();
    for withdrawal_info in withdrawal_info.iter() {
        let withdrawal = withdrawal_info.contract_withdrawal.clone();
        if withdrawal_info.status == WithdrawalStatus::NeedClaim {
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
    }
    if claim_withdrawals.is_empty() {
        println!("No withdrawals to claim");
        return Ok(());
    }
    let liquidity_contract = client.liquidity_contract.clone();
    liquidity_contract
        .claim_withdrawals(signer_private_key, None, &claim_withdrawals)
        .await?;
    Ok(())
}

pub async fn claim_builder_reward(eth_private_key: Bytes32) -> Result<(), CliError> {
    let env = envy::from_env::<EnvVar>()?;
    let signer_private_key = convert_bytes32_to_b256(eth_private_key);
    let user_address = get_address_from_private_key(signer_private_key);
    log::info!(
        "Claiming block builder reward for user address: {}",
        user_address
    );

    if env.reward_contract_address.is_none() {
        return Err(CliError::EnvError(
            "REWARD_CONTRACT_ADDRESS is not set".to_string(),
        ));
    }

    let provider = get_client()?.rollup_contract.provider.clone();
    let reward_contract_address = convert_address_to_alloy(env.reward_contract_address.unwrap());
    let reward_contract = BlockBuilderRewardContract::new(provider, reward_contract_address);
    let current_period = reward_contract.get_current_period().await?;
    log::info!("Current period: {}", current_period);

    let mut claimable_periods = Vec::new();
    for period_number in 0..current_period {
        let claimable_reward = reward_contract
            .get_claimable_reward(period_number, user_address)
            .await?;
        if claimable_reward > U256::ZERO {
            log::info!(
                "Claiming block builder reward for period {}: {}",
                period_number,
                claimable_reward
            );
            claimable_periods.push(period_number);
        }
    }
    if claimable_periods.is_empty() {
        println!("No block builder rewards to claim");
        return Ok(());
    }
    log::info!(
        "Claiming block builder rewards for periods: {:?}",
        claimable_periods
    );
    reward_contract
        .batch_claim_reward(signer_private_key, None, &claimable_periods)
        .await?;
    Ok(())
}
