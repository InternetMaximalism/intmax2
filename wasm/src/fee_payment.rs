use intmax2_client_sdk::client::types::TransferRequest;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

use crate::{
    client::{get_client, Config},
    init_logger,
    js_types::{
        client::JsTransferRequest,
        fee::JsWithdrawalTransfers,
        payment_memo::{JsPaymentMemo, JsPaymentMemoEntry},
    },
    utils::str_to_view_pair,
};

// Quote the fee for withdrawal and claim fee (if with_claim_fee is true), and generate the corresponding transfers
// and payment memos.
// if withdrawal_transfer.amount is 0, the withdrawal transfer will be skipped and only fees will be included
// in the transfers and payment memos.
#[wasm_bindgen]
pub async fn generate_withdrawal_transfers(
    config: &Config,
    withdrawal_transfer_request: &JsTransferRequest,
    fee_token_index: u32,
    with_claim_fee: bool,
) -> Result<JsWithdrawalTransfers, JsError> {
    init_logger();
    let client = get_client(config);
    let withdrawal_transfer = TransferRequest::try_from(withdrawal_transfer_request.clone())?;
    let withdrawal_transfers =
        intmax2_client_sdk::client::fee_payment::generate_withdrawal_transfers(
            client.withdrawal_server.as_ref(),
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
    transfer_requests: Vec<JsTransferRequest>,
    withdrawal_fee_transfer_index: Option<u32>,
    claim_fee_transfer_index: Option<u32>,
) -> Result<Vec<JsPaymentMemoEntry>, JsError> {
    init_logger();
    let transfers = transfer_requests
        .into_iter()
        .map(|t| t.try_into())
        .collect::<Result<Vec<TransferRequest>, _>>()?;
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

#[wasm_bindgen]
pub async fn get_used_memos(
    config: &Config,
    view_pair: &str,
) -> Result<Vec<JsPaymentMemo>, JsError> {
    init_logger();
    let client = get_client(config);
    let view_pair = str_to_view_pair(view_pair)?;
    let payment_memos = intmax2_client_sdk::client::fee_payment::get_used_memos(
        client.store_vault_server.as_ref(),
        view_pair,
    )
    .await?;
    let js_payment_memos = payment_memos.into_iter().map(JsPaymentMemo::from).collect();
    Ok(js_payment_memos)
}
