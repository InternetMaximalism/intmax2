use intmax2_interfaces::{
    api::store_vault_server::types::DataWithMetaData, data::data_type::DataType,
};
use intmax2_zkp::ethereum_types::u32limb_trait::U32LimbTrait;
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsEncryptedData {
    pub data: Vec<u8>,
    pub digest: String,
    pub timestamp: u64,
    /// Deposit, Transfer(Receive), Tx(Send)
    pub data_type: String,
}

impl JsEncryptedData {
    pub fn new(data_type: DataType, data_with_meta: DataWithMetaData) -> Self {
        Self {
            data: data_with_meta.data,
            digest: data_with_meta.meta.digest.to_hex(),
            timestamp: data_with_meta.meta.timestamp,
            data_type: data_type.to_string(),
        }
    }
}
