use intmax2_client_sdk::client::{
    fee_payment::generate_fee_payment_memo, sync::utils::generate_salt,
};
use intmax2_zkp::{
    common::{signature_content::key_set::KeySet, transfer::Transfer},
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
    wait: bool,
) -> Result<(), CliError> {
    let client = get_client()?;

    let withdrawal_transfer = build_transfer(to, amount, token_index);

    let withdrawal_transfers = client
        .generate_withdrawal_transfers(&withdrawal_transfer, fee_token_index, with_claim_fee)
        .await?;

    log_transfer_fee(
        "Withdrawal fee",
        withdrawal_transfers.withdrawal_fee_transfer_index,
        &withdrawal_transfers.transfers,
    )?;
    log_transfer_fee(
        "Claim fee",
        withdrawal_transfers.claim_fee_transfer_index,
        &withdrawal_transfers.transfers,
    )?;

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
        wait,
    )
    .await?;

    Ok(())
}

fn build_transfer(to: Address, amount: U256, token_index: u32) -> Transfer {
    Transfer {
        recipient: to.into(),
        token_index,
        amount,
        salt: generate_salt(),
    }
}

fn log_transfer_fee(
    label: &str,
    index: Option<u32>,
    transfers: &[Transfer],
) -> Result<(), CliError> {
    if let Some(i) = index {
        let i = i as usize;
        if i >= transfers.len() {
            return Err(CliError::FailedToSendFee(format!(
                "{} index {} out of bounds (len = {})",
                label,
                i,
                transfers.len()
            )));
        }

        let t = &transfers[i];
        log::info!("{}: {} #{}", label, t.amount, t.token_index);
    }
    Ok(())
}
