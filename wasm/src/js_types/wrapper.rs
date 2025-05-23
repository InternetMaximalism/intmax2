use intmax2_client_sdk::client::client::TxRequestMemo;
use intmax2_zkp::common::block_builder::BlockProposal;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

use super::common::JsTx;

#[derive(Debug, Clone)]
#[wasm_bindgen]
pub struct JsTxRequestMemo {
    data: String,
}

impl JsTxRequestMemo {
    pub fn from_tx_request_memo(memo: &TxRequestMemo) -> Self {
        Self {
            data: serde_json::to_string(memo).unwrap(),
        }
    }

    pub fn to_tx_request_memo(&self) -> Result<TxRequestMemo, JsError> {
        serde_json::from_str(&self.data)
            .map_err(|e| JsError::new(&format!("failed to parse tx request memo {e}")))
    }
}

#[wasm_bindgen]
impl JsTxRequestMemo {
    pub fn tx(&self) -> Result<JsTx, JsError> {
        let memo = self.to_tx_request_memo()?;
        let tx = memo.tx.into();
        Ok(tx)
    }

    pub fn is_registration_block(&self) -> Result<bool, JsError> {
        let memo = self.to_tx_request_memo()?;
        Ok(memo.is_registration_block)
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen]
pub struct JsBlockProposal {
    data: String,
}

impl JsBlockProposal {
    pub fn from_block_proposal(proposal: &BlockProposal) -> Self {
        Self {
            data: serde_json::to_string(proposal).unwrap(),
        }
    }

    pub fn to_block_proposal(&self) -> Result<BlockProposal, JsError> {
        serde_json::from_str(&self.data)
            .map_err(|e| JsError::new(&format!("failed to parse block proposal {e}")))
    }
}

#[wasm_bindgen]
impl JsBlockProposal {
    #[wasm_bindgen]
    pub fn tx_tree_root(&self) -> Result<String, JsError> {
        let proposal = self.to_block_proposal()?;
        Ok(proposal.block_sign_payload.tx_tree_root.to_string())
    }
}
