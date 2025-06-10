use crate::js_types::data::{JsDepositData, JsTransferData, JsTxData};
use intmax2_client_sdk::client::types::{
    DepositResult, GenericRecipient, TransferRequest, TxResult,
};
use intmax2_zkp::ethereum_types::u32limb_trait::U32LimbTrait as _;
use serde::{Deserialize, Serialize};
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsTransferRequest {
    pub recipient: String,
    pub token_index: u32,
    pub amount: String,
    pub description: Option<String>,
}

impl From<TransferRequest> for JsTransferRequest {
    fn from(request: TransferRequest) -> Self {
        Self {
            recipient: request.recipient.to_string(),
            token_index: request.token_index,
            amount: request.amount.to_string(),
            description: request.description,
        }
    }
}

impl TryFrom<JsTransferRequest> for TransferRequest {
    type Error = JsError;

    fn try_from(js_request: JsTransferRequest) -> Result<Self, Self::Error> {
        let recipient: GenericRecipient = js_request
            .recipient
            .parse()
            .map_err(|e| JsError::new(&format!("Invalid recipient: {e}")))?;
        let amount = js_request
            .amount
            .parse()
            .map_err(|e| JsError::new(&format!("Invalid amount: {e}")))?;
        Ok(Self {
            recipient,
            token_index: js_request.token_index,
            amount,
            description: js_request.description,
        })
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsTxResult {
    pub tx_tree_root: String,
    pub tx_digest: String,
    pub tx_data: JsTxData,
    pub transfer_data_vec: Vec<JsTransferData>,
    pub backup_csv: String,
}

impl From<TxResult> for JsTxResult {
    fn from(tx_result: TxResult) -> Self {
        Self {
            tx_tree_root: tx_result.tx_tree_root.to_hex(),
            tx_digest: tx_result.tx_digest.to_hex(),
            tx_data: tx_result.tx_data.into(),
            transfer_data_vec: tx_result
                .transfer_data_vec
                .into_iter()
                .map(Into::into)
                .collect(),
            backup_csv: tx_result.backup_csv,
        }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsDepositResult {
    pub deposit_data: JsDepositData,
    pub deposit_digest: String,
    pub backup_csv: String,
}

impl From<DepositResult> for JsDepositResult {
    fn from(deposit_result: DepositResult) -> Self {
        Self {
            deposit_data: deposit_result.deposit_data.into(),
            deposit_digest: deposit_result.deposit_digest.to_string(),
            backup_csv: deposit_result.backup_csv,
        }
    }
}
