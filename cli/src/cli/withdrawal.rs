use intmax2_client_sdk::client::{
    client::{GenericRecipient, TransferRequest},
    fee_payment::generate_fee_payment_memo,
};
use intmax2_interfaces::utils::key::KeyPair;
use intmax2_zkp::ethereum_types::{address::Address, u256::U256};

use super::{client::get_client, error::CliError, send::send_transfers};

pub async fn send_withdrawal(
    key_pair: KeyPair,
    to: Address,
    amount: U256,
    token_index: u32,
    fee_token_index: u32,
    with_claim_fee: bool,
    wait: bool,
) -> Result<(), CliError> {
    let client = get_client()?;
    let withdrawal_transfer_req = TransferRequest {
        recipient: GenericRecipient::Address(to),
        token_index,
        amount,
        description: None,
    };
    let withdrawal_transfers = client
        .generate_withdrawal_transfers(&withdrawal_transfer_req, fee_token_index, with_claim_fee)
        .await?;
    if let Some(withdrawal_fee_index) = withdrawal_transfers.withdrawal_fee_transfer_index {
        let withdrawal_fee_transfer =
            &withdrawal_transfers.transfer_requests[withdrawal_fee_index as usize];
        log::info!(
            "Withdrawal fee: {} #{}",
            withdrawal_fee_transfer.amount,
            withdrawal_fee_transfer.token_index
        );
    }
    if let Some(claim_fee_index) = withdrawal_transfers.claim_fee_transfer_index {
        let claim_fee_transfer = &withdrawal_transfers.transfer_requests[claim_fee_index as usize];
        log::info!(
            "Claim fee: {} #{}",
            claim_fee_transfer.amount,
            claim_fee_transfer.token_index
        );
    }

    let payment_memos = generate_fee_payment_memo(
        &withdrawal_transfers.transfer_requests,
        withdrawal_transfers.withdrawal_fee_transfer_index,
        withdrawal_transfers.claim_fee_transfer_index,
    )?;
    send_transfers(
        key_pair,
        &withdrawal_transfers.transfer_requests,
        payment_memos,
        fee_token_index,
        wait,
    )
    .await?;
    Ok(())
}
