use intmax2_interfaces::api::validity_prover::interface::DepositInfo;
use intmax2_zkp::ethereum_types::u32limb_trait::U32LimbTrait;
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsDepositInfo {
    pub deposit_id: u64,
    pub token_index: u32,
    pub deposit_hash: String,
    pub block_number: Option<u32>,
    pub deposit_index: Option<u32>,
    pub l1_deposit_tx_hash: String,
}

impl From<DepositInfo> for JsDepositInfo {
    fn from(deposit_info: DepositInfo) -> Self {
        Self {
            deposit_id: deposit_info.deposit_id,
            token_index: deposit_info.token_index,
            deposit_hash: deposit_info.deposit_hash.to_hex(),
            block_number: deposit_info.block_number,
            deposit_index: deposit_info.deposit_index,
            l1_deposit_tx_hash: deposit_info.l1_deposit_tx_hash.to_hex(),
        }
    }
}
