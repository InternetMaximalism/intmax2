use intmax2_client_sdk::client::client::PaymentMemoEntry;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

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
            topic: payment_memo_entry.topic,
            memo: payment_memo_entry.memo.to_string(),
        }
    }
}

impl TryFrom<JsPaymentMemoEntry> for PaymentMemoEntry {
    type Error = JsError;

    fn try_from(js_payment_memo_entry: JsPaymentMemoEntry) -> Result<Self, JsError> {
        Ok(PaymentMemoEntry {
            transfer_index: js_payment_memo_entry.transfer_index,
            topic: js_payment_memo_entry.topic.clone(),
            memo: js_payment_memo_entry.memo.clone(),
        })
    }
}
