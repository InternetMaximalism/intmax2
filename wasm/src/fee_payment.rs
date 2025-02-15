use intmax2_zkp::common::transfer::Transfer;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

use crate::{
    client::{get_client, Config},
    init_logger,
    js_types::{common::JsTransfer, fee::JsWithdrawalTransfers, payment_memo::JsPaymentMemoEntry},
};

// Quote the fee for withdrawal and claim fee (if with_claim_fee is true), and generate the corresponding transfers
// and payment memos.
// if withdrawal_transfer.amount is 0, the withdrawal transfer will be skipped and only fees will be included
// in the transfers and payment memos.
#[wasm_bindgen]
pub async fn generate_withdrawal_transfers(
    config: &Config,
    withdrawal_transfer: &JsTransfer,
    fee_token_index: u32,
    with_claim_fee: bool,
) -> Result<JsWithdrawalTransfers, JsError> {
    init_logger();
    let client = get_client(config);
    let withdrawal_transfer = Transfer::try_from(withdrawal_transfer.clone())?;
    let withdrawal_transfers =
        intmax2_client_sdk::client::fee_payment::generate_withdrawal_transfers(
            &client.withdrawal_server,
            &client.withdrawal_contract,
            &withdrawal_transfer,
            fee_token_index,
            with_claim_fee,
        )
        .await?;
    Ok(JsWithdrawalTransfers::from(withdrawal_transfers))
}

/// Generate fee payment memo from given transfers and fee transfer indices
#[wasm_bindgen]
pub fn generate_fee_payment_memo(
    transfers: Vec<JsTransfer>,
    withdrawal_fee_transfer_index: Option<u32>,
    claim_fee_transfer_index: Option<u32>,
) -> Result<Vec<JsPaymentMemoEntry>, JsError> {
    init_logger();
    let transfers = transfers
        .into_iter()
        .map(|t| t.try_into())
        .collect::<Result<Vec<Transfer>, _>>()?;
    let payment_memos = intmax2_client_sdk::client::fee_payment::generate_fee_payment_memo(
        &transfers,
        withdrawal_fee_transfer_index,
        claim_fee_transfer_index,
    )?;
    let js_payment_memos = payment_memos
        .into_iter()
        .map(JsPaymentMemoEntry::from)
        .collect();
    Ok(js_payment_memos)
}
