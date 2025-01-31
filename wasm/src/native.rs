use crate::{
    init_logger,
    js_types::{
        auth::JsAuth,
        data::{JsDepositData, JsTransferData, JsTxData},
    },
    utils::str_privkey_to_keyset,
};
use intmax2_client_sdk::external_api::store_vault_server::generate_auth_for_get_data_sequence;
use intmax2_interfaces::data::{
    deposit_data::DepositData, encryption::Encryption as _, transfer_data::TransferData,
    tx_data::TxData,
};
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

/// Decrypt the deposit data.
#[wasm_bindgen]
pub async fn decrypt_deposit_data(
    private_key: &str,
    data: &[u8],
) -> Result<JsDepositData, JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let deposit_data =
        DepositData::decrypt(data, key).map_err(|e| JsError::new(&format!("{}", e)))?;
    Ok(deposit_data.into())
}

/// Decrypt the transfer data. This is also used to decrypt the withdrawal data.
#[wasm_bindgen]
pub async fn decrypt_transfer_data(
    private_key: &str,
    data: &[u8],
) -> Result<JsTransferData, JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let transfer_data =
        TransferData::decrypt(data, key).map_err(|e| JsError::new(&format!("{}", e)))?;
    Ok(transfer_data.into())
}

/// Decrypt the tx data.
#[wasm_bindgen]
pub async fn decrypt_tx_data(private_key: &str, data: &[u8]) -> Result<JsTxData, JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let tx_data = TxData::decrypt(data, key).map_err(|e| JsError::new(&format!("{}", e)))?;
    Ok(tx_data.into())
}

#[wasm_bindgen]
pub async fn generate_auth_for_store_vault(private_key: &str) -> Result<JsAuth, JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let auth = generate_auth_for_get_data_sequence(key);
    Ok(auth.into())
}

// #[wasm_bindgen]
// pub async fn fetch_encrypted_data(
//     config: &Config,
//     auth: &JsAuth,
//     cursor: &Option<JsMetaData>,
//     limit: &Option<u32>,
// ) -> Result<JsEncryptedData, JsError> {
