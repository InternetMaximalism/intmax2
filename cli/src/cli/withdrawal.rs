use intmax2_client_sdk::client::{
    fee_payment::generate_fee_payment_memo, sync::utils::generate_salt,
};
use intmax2_zkp::{
    common::{generic_address::GenericAddress, signature::key_set::KeySet, transfer::Transfer},
    ethereum_types::{address::Address, u256::U256},
};

use super::{client::get_client, error::CliError, send::send_transfers};

pub async fn send_withdrawal(
    key: KeySet,
    to: Address,
    amount: U256,
    token_index: u32,
    fee_token_index: u32,
    with_claim_fee: bool,
) -> Result<(), CliError> {
    let client = get_client()?;
    let withdrawal_transfer = Transfer {
        recipient: GenericAddress::from_address(to),
        token_index,
        amount,
        salt: generate_salt(),
    };
    let withdrawal_transfers = client
        .generate_withdrawal_transfers(&withdrawal_transfer, fee_token_index, with_claim_fee)
        .await?;
    if let Some(withdrawal_fee_index) = withdrawal_transfers.withdrawal_fee_transfer_index {
        let withdrawal_fee_transfer =
            &withdrawal_transfers.transfers[withdrawal_fee_index as usize];
        log::info!(
            "Withdrawal fee: {} #{}",
            withdrawal_fee_transfer.amount,
            withdrawal_fee_transfer.token_index
        );
    }
    if let Some(claim_fee_index) = withdrawal_transfers.claim_fee_transfer_index {
        let claim_fee_transfer = &withdrawal_transfers.transfers[claim_fee_index as usize];
        log::info!(
            "Claim fee: {} #{}",
            claim_fee_transfer.amount,
            claim_fee_transfer.token_index
        );
    }

    let payment_memos = generate_fee_payment_memo(
        &withdrawal_transfers.transfers,
        withdrawal_transfers.withdrawal_fee_transfer_index,
        withdrawal_transfers.claim_fee_transfer_index,
    )?;
    send_transfers(
        key,
        &withdrawal_transfers.transfers,
        payment_memos,
        fee_token_index,
    )
    .await?;
    Ok(())
}
