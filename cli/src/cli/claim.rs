use intmax2_client_sdk::external_api::contract::convert::convert_bytes32_to_h256;
use intmax2_interfaces::api::withdrawal_server::interface::WithdrawalStatus;
use intmax2_zkp::{common::signature_content::key_set::KeySet, ethereum_types::bytes32::Bytes32};

use crate::cli::client::get_client;

use super::error::CliError;

pub async fn claim_withdrawals(key: KeySet, eth_private_key: Bytes32) -> Result<(), CliError> {
    let signer_private_key = convert_bytes32_to_h256(eth_private_key);
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
        .claim_withdrawals(signer_private_key, &claim_withdrawals)
        .await?;
    Ok(())
}
