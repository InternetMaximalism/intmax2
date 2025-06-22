use intmax2_client_sdk::client::{
    fee_payment::generate_fee_payment_memo,
    types::{GenericRecipient, TransferRequest},
};
use intmax2_interfaces::utils::{
    fee::{Fee, FeeList},
    key::KeyPair,
};
use intmax2_zkp::ethereum_types::{address::Address, u256::U256};

use crate::{cli::fee_cap::validate_fee_cap, env_var::EnvVar};

use super::{client::get_client, error::CliError, send::send_transfers};

#[allow(clippy::too_many_arguments)]
pub async fn send_withdrawal(
    key_pair: KeyPair,
    to: Address,
    amount: U256,
    token_index: u32,
    description: Option<String>,
    fee_token_index: u32,
    with_claim_fee: bool,
    wait: bool,
) -> Result<(), CliError> {
    let env = envy::from_env::<EnvVar>()?;
    let client = get_client()?;

    let withdrawal_transfer_request = build_transfer_request(to, amount, token_index, description);

    let withdrawal_transfers = client
        .generate_withdrawal_transfers(
            &withdrawal_transfer_request,
            fee_token_index,
            with_claim_fee,
        )
        .await?;

    validate_and_log_transfer_fee(
        "Withdrawal fee",
        withdrawal_transfers.withdrawal_fee_transfer_index,
        &withdrawal_transfers.transfer_requests,
        &env.max_withdrawal_fee,
    )?;
    validate_and_log_transfer_fee(
        "Claim fee",
        withdrawal_transfers.claim_fee_transfer_index,
        &withdrawal_transfers.transfer_requests,
        &env.max_claim_fee,
    )?;

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

fn build_transfer_request(
    to: Address,
    amount: U256,
    token_index: u32,
    description: Option<String>,
) -> TransferRequest {
    TransferRequest {
        recipient: GenericRecipient::Address(to),
        token_index,
        amount,
        description,
    }
}

fn validate_and_log_transfer_fee(
    label: &str,
    index: Option<u32>,
    transfer_requests: &[TransferRequest],
    fee_caps: &Option<FeeList>,
) -> Result<(), CliError> {
    if let Some(i) = index {
        let i = i as usize;
        if i >= transfer_requests.len() {
            return Err(CliError::FailedToSendFee(format!(
                "{} index {} out of bounds (len = {})",
                label,
                i,
                transfer_requests.len()
            )));
        }
        let t = &transfer_requests[i];
        let fee = Fee {
            amount: t.amount,
            token_index: t.token_index,
        };
        validate_fee_cap(&fee, fee_caps)?;
        log::info!("{}: {} #{}", label, t.amount, t.token_index);
    }
    Ok(())
}
