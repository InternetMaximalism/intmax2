use intmax2_client_sdk::client::client::PaymentMemoEntry;
use intmax2_zkp::ethereum_types::u32limb_trait::U32LimbTrait;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

use super::utils::parse_bytes32;

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsPaymentMemoEntry {
    pub transfer_index: u32,
    pub topic: String,
    pub memo: String,
}

impl From<PaymentMemoEntry> for JsPaymentMemoEntry {
    fn from(payment_memo_entry: PaymentMemoEntry) -> Self {
        Self {
            transfer_index: payment_memo_entry.transfer_index,
            topic: payment_memo_entry.topic.to_hex(),
            memo: payment_memo_entry.memo.to_string(),
        }
    }
}

impl TryFrom<JsPaymentMemoEntry> for PaymentMemoEntry {
    type Error = JsError;

    fn try_from(js_payment_memo_entry: JsPaymentMemoEntry) -> Result<Self, JsError> {
        let topic = parse_bytes32(&js_payment_memo_entry.topic)?;
        Ok(PaymentMemoEntry {
            transfer_index: js_payment_memo_entry.transfer_index,
            topic,
            memo: js_payment_memo_entry.memo.clone(),
        })
    }
}
