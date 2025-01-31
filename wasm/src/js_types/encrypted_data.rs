use intmax2_interfaces::api::store_vault_server::{interface::DataType, types::DataWithMetaData};
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsEncryptedData {
    pub data: Vec<u8>,
    pub uuid: String,
    pub timestamp: u64,
    /// Deposit, Transfer(Receive), Tx(Send)
    pub data_type: String,
}

impl JsEncryptedData {
    pub fn new(data_type: DataType, data_with_meta: DataWithMetaData) -> Self {
        Self {
            data: data_with_meta.data,
            uuid: data_with_meta.meta.uuid,
            timestamp: data_with_meta.meta.timestamp,
            data_type: data_type.to_string(),
        }
    }
}
