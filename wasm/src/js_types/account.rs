use intmax2_interfaces::api::validity_prover::interface::AccountInfo;
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsAccountInfo {
    pub account_id: Option<u64>,
    pub block_number: u32,
    pub last_block_number: u32,
}

impl From<AccountInfo> for JsAccountInfo {
    fn from(account_info: AccountInfo) -> Self {
        Self {
            account_id: account_info.account_id,
            block_number: account_info.block_number,
            last_block_number: account_info.last_block_number,
        }
    }
}
