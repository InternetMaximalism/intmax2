use crate::{
    client::{get_client, Config},
    init_logger,
    js_types::{
        account::JsAccountInfo,
        auth::{JsAuth, JsFlatG2},
        cursor::JsMetaDataCursor,
        data::{JsDepositData, JsTransferData, JsTxData},
        encrypted_data::JsEncryptedData,
    },
    utils::{parse_h256_as_u256, str_privkey_to_keyset},
};
use intmax2_client_sdk::external_api::{
    s3_store_vault::generate_auth_for_get_data_sequence_s3,
    store_vault_server::generate_auth_for_get_data_sequence,
};
use intmax2_interfaces::{
    api::store_vault_server::types::{CursorOrder, MetaDataCursor},
    data::{
        data_type::DataType, deposit_data::DepositData, encryption::BlsEncryption as _,
        transfer_data::TransferData, tx_data::TxData,
    },
    utils::signature::Auth,
};
use intmax2_zkp::{
    common::signature_content::{self, flatten::FlatG2},
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
        DepositData::decrypt(key, None, data).map_err(|e| JsError::new(&format!("{}", e)))?;
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
        TransferData::decrypt(key, None, data).map_err(|e| JsError::new(&format!("{}", e)))?;
    Ok(transfer_data.into())
}

/// Decrypt the tx data.
#[wasm_bindgen]
pub async fn decrypt_tx_data(private_key: &str, data: &[u8]) -> Result<JsTxData, JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let tx_data = TxData::decrypt(key, Some(key.pubkey), data)
        .map_err(|e| JsError::new(&format!("{}", e)))?;
    Ok(tx_data.into())
}

#[wasm_bindgen]
pub async fn generate_auth_for_store_vault(
    private_key: &str,
    use_s3: bool,
) -> Result<JsAuth, JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let auth = if use_s3 {
        generate_auth_for_get_data_sequence_s3(key)
    } else {
        generate_auth_for_get_data_sequence(key)
    };
    Ok(auth.into())
}

#[wasm_bindgen]
pub async fn fetch_encrypted_data(
    config: &Config,
    auth: &JsAuth,
    cursor: &JsMetaDataCursor,
) -> Result<Vec<JsEncryptedData>, JsError> {
    init_logger();
    let client = get_client(config);
    let sv = client.store_vault_server;
    let auth: Auth = auth
        .clone()
        .try_into()
        .map_err(|e| JsError::new(&format!("failed to convert JsAuth to Auth: {}", e)))?;
    let cursor: MetaDataCursor = cursor.clone().try_into()?;
    let mut data_array = Vec::new();
    let (deposit_data, _) = sv
        .get_data_sequence_with_auth(&DataType::Deposit.to_topic(), &cursor, &auth)
        .await?;
    data_array.extend(
        deposit_data
            .into_iter()
            .map(|data| JsEncryptedData::new(DataType::Deposit, data)),
    );
    let (transfer_data, _) = sv
        .get_data_sequence_with_auth(&DataType::Transfer.to_topic(), &cursor, &auth)
        .await?;
    data_array.extend(
        transfer_data
            .into_iter()
            .map(|data| JsEncryptedData::new(DataType::Transfer, data)),
    );
    let (tx_data, _) = sv
        .get_data_sequence_with_auth(&DataType::Tx.to_topic(), &cursor, &auth)
        .await?;
    data_array.extend(
        tx_data
            .into_iter()
            .map(|data| JsEncryptedData::new(DataType::Tx, data)),
    );
    data_array.sort_by_key(|data| (data.timestamp, data.digest.clone()));
    if cursor.order == CursorOrder::Desc {
        data_array.reverse();
    }
    data_array.truncate(cursor.limit.unwrap_or(data_array.len() as u32) as usize);
    Ok(data_array)
}

#[wasm_bindgen]
pub async fn sign_message(private_key: &str, message: &[u8]) -> Result<JsFlatG2, JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let signature = signature_content::sign_tools::sign_message(key.privkey, message);
    Ok(FlatG2::from(signature).into())
}

#[wasm_bindgen]
pub async fn verify_signature(
    signature: &JsFlatG2,
    public_key: &str,
    message: &[u8],
) -> Result<bool, JsError> {
    init_logger();
    let public_key =
        U256::from_hex(public_key).map_err(|_| JsError::new("Failed to parse public key"))?;
    let signature = FlatG2::try_from(signature.clone())
        .map_err(|_| JsError::new("Failed to parse signature"))?;

    let result =
        signature_content::sign_tools::verify_signature(signature.into(), public_key, message);

    Ok(result.is_ok())
}

#[wasm_bindgen]
pub async fn get_account_info(config: &Config, public_key: &str) -> Result<JsAccountInfo, JsError> {
    init_logger();
    let pubkey = parse_h256_as_u256(public_key)?;
    let client = get_client(config);
    let account_info = client.validity_prover.get_account_info(pubkey).await?;
    Ok(account_info.into())
}
