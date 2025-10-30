use intmax2_client_sdk::client::{misc::payment_memo::PaymentMemo, types::PaymentMemoEntry};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

use crate::js_types::{common::JsMetaData, data::JsTransferData};

#[derive(Debug, Clone, Serialize, Deserialize)]
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
            memo: payment_memo_entry.memo,
        }
    }
}

impl TryFrom<JsPaymentMemoEntry> for PaymentMemoEntry {
    type Error = JsError;

    fn try_from(js_payment_memo_entry: JsPaymentMemoEntry) -> Result<Self, JsError> {
        Ok(PaymentMemoEntry {
            transfer_index: js_payment_memo_entry.transfer_index,
            topic: js_payment_memo_entry.topic,
            memo: js_payment_memo_entry.memo,
        })
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsPaymentMemo {
    pub meta: JsMetaData,
    pub transfer_data: JsTransferData,
    pub memo: String,
}

impl From<PaymentMemo> for JsPaymentMemo {
    fn from(payment_memo: PaymentMemo) -> Self {
        Self {
            meta: JsMetaData::from(payment_memo.meta),
            transfer_data: JsTransferData::from(payment_memo.transfer_data),
            memo: payment_memo.memo,
        }
    }
}
