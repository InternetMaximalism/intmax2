use crate::{
    client::{get_client, Config},
    init_logger,
    js_types::{
        auth::{JsAuth, JsFlatG2},
        data::{JsDepositData, JsTransferData, JsTxData},
        encrypted_data::JsEncryptedData,
    },
    utils::str_privkey_to_keyset,
};
use intmax2_client_sdk::external_api::store_vault_server::generate_auth_for_get_data_sequence;
use intmax2_interfaces::{
    api::store_vault_server::{interface::DataType, types::CursorOrder},
    data::{
        deposit_data::DepositData, encryption::Encryption as _, meta_data::MetaData,
        transfer_data::TransferData, tx_data::TxData,
    },
    utils::signature::Auth,
};
use intmax2_zkp::{
    common::signature::{self, flatten::FlatG2},
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
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

#[wasm_bindgen]
pub async fn fetch_encrypted_data(
    config: &Config,
    auth: &JsAuth,
    timestamp: Option<u64>,
    uuid: Option<String>,
    limit: Option<u32>,
    order: String, // asc or desc
) -> Result<Vec<JsEncryptedData>, JsError> {
    init_logger();
    let client = get_client(config);
    let sv = client.store_vault_server;
    if (timestamp.is_none() && uuid.is_some()) || (timestamp.is_some() && uuid.is_none()) {
        return Err(JsError::new(
            "Either both timestamp and uuid should be provided or none",
        ));
    }
    let auth: Auth = auth
        .clone()
        .try_into()
        .map_err(|e| JsError::new(&format!("failed to convert JsAuth to Auth: {}", e)))?;
    let order: CursorOrder = order
        .parse()
        .map_err(|e| JsError::new(&format!("failed to parse order: {}", e)))?;

    let metadata_cursor = if let (Some(timestamp), Some(uuid)) = (timestamp, uuid) {
        Some(MetaData { timestamp, uuid })
    } else {
        None
    };
    let mut data_array = Vec::new();
    let (deposit_data, _) = sv
        .get_data_sequence_native(DataType::Deposit, &metadata_cursor, &limit, &order, &auth)
        .await?;
    data_array.extend(
        deposit_data
            .into_iter()
            .map(|data| JsEncryptedData::new(DataType::Deposit, data)),
    );
    let (transfer_data, _) = sv
        .get_data_sequence_native(DataType::Transfer, &metadata_cursor, &limit, &order, &auth)
        .await?;
    data_array.extend(
        transfer_data
            .into_iter()
            .map(|data| JsEncryptedData::new(DataType::Transfer, data)),
    );
    let (tx_data, _) = sv
        .get_data_sequence_native(DataType::Tx, &metadata_cursor, &limit, &order, &auth)
        .await?;
    data_array.extend(
        tx_data
            .into_iter()
            .map(|data| JsEncryptedData::new(DataType::Tx, data)),
    );
    data_array.sort_by_key(|data| (data.timestamp, data.uuid.clone()));
    if order == CursorOrder::Desc {
        data_array.reverse();
    }
    data_array.truncate(limit.unwrap_or(data_array.len() as u32) as usize);
    Ok(data_array)
}

#[wasm_bindgen]
pub async fn sign_message(private_key: &str, message: &[u8]) -> Result<JsFlatG2, JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let signature = signature::sign::sign_message(key.privkey, message);

    Ok(FlatG2::from(signature).into())
}

#[wasm_bindgen]
pub async fn verify_signature(
    signature: &JsFlatG2,
    public_key: &str,
    message: &[u8],
) -> Result<(), JsError> {
    let public_key =
        U256::from_hex(public_key).map_err(|_| JsError::new("Failed to parse public key"))?;
    let signature = FlatG2::try_from(signature.clone())
        .map_err(|_| JsError::new("Failed to parse signature"))?;

    signature::sign::verify_signature(signature.into(), public_key, message)
        .map_err(|e| JsError::new(&e.to_string()))?;

    Ok(())
}
