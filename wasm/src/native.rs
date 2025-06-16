use crate::{
    client::{get_client, Config},
    init_logger,
    js_types::{
        account::JsAccountInfo,
        auth::{JsAuth, JsFlatG2},
        cursor::JsMetaDataCursor,
        data::{JsDepositData, JsTransferData, JsTxData},
        deposit::JsDepositInfo,
        encrypted_data::JsEncryptedData,
        multisig::{
            JsMultiEciesStep1Response, JsMultiEciesStep2Response, JsMultiEciesStep3Response,
            JsMultisigStep1Response, JsMultisigStep2Response, JsMultisigStep3Response,
        },
        utils::{parse_bytes32, parse_intmax_address, parse_payment_id},
    },
    utils::{str_to_keyset, str_to_view_pair},
};
use intmax2_client_sdk::{
    client::multisig,
    external_api::{
        s3_store_vault::generate_auth_for_get_data_sequence_s3,
        store_vault_server::generate_auth_for_get_data_sequence,
    },
};
use intmax2_interfaces::{
    api::store_vault_server::types::{CursorOrder, MetaDataCursor},
    data::{
        data_type::DataType,
        deposit_data::DepositData,
        encryption::{
            bls::v1::{algorithm::encrypt_bls, multisig as multisig_encryption},
            BlsEncryption as _,
        },
        transfer_data::TransferData,
        tx_data::TxData,
    },
    utils::{
        address::{AddressType, IntmaxAddress},
        random::default_rng,
        signature::Auth,
    },
};
use intmax2_zkp::{
    common::signature_content::{self, flatten::FlatG2},
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
};
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

/// Validate if the given address is a valid Intmax address without checking the network.
#[wasm_bindgen]
pub fn validate_intmax_address(address: &str) -> bool {
    address.parse::<IntmaxAddress>().is_ok()
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct AddressAuxInfo {
    pub payment_id: Option<String>,
    pub network: String,
}

/// Extracts auxiliary information from an Intmax address string, returning an `AddressAuxInfo`
/// which contains the optional payment ID (if present) and the network as a string.
#[wasm_bindgen]
pub fn extract_address_aux_info(address: &str) -> Result<AddressAuxInfo, JsError> {
    let address = parse_intmax_address(address)?;
    let payment_id = match address.addr_type {
        AddressType::Standard => None,
        AddressType::Integrated(payment_id) => Some(payment_id.to_string()),
    };
    Ok(AddressAuxInfo {
        payment_id,
        network: address.network.to_string(),
    })
}

/// Generate integrated address from a standard address and a payment ID.
#[wasm_bindgen]
pub fn generate_integrated_address(address: &str, payment_id: &str) -> Result<String, JsError> {
    let address = parse_intmax_address(address)?;
    let payment_id = parse_payment_id(payment_id)?;
    if address.addr_type != AddressType::Standard {
        return Err(JsError::new("Address must be a standard address"));
    }
    let integrated_address = IntmaxAddress::integrated(
        address.network,
        address.public_spend,
        address.public_view,
        payment_id,
    );
    Ok(integrated_address.to_string())
}

/// Decrypt the deposit data.
#[wasm_bindgen]
pub async fn decrypt_deposit_data(view_pair: &str, data: &[u8]) -> Result<JsDepositData, JsError> {
    init_logger();
    let view_pair = str_to_view_pair(view_pair)?;
    let deposit_data = DepositData::decrypt(view_pair.view, None, data)
        .map_err(|e| JsError::new(&format!("{e}")))?;
    Ok(deposit_data.into())
}

/// Decrypt the transfer data. This is also used to decrypt the withdrawal data.
#[wasm_bindgen]
pub async fn decrypt_transfer_data(
    view_pair: &str,
    data: &[u8],
) -> Result<JsTransferData, JsError> {
    init_logger();
    let view_pair = str_to_view_pair(view_pair)?;
    let transfer_data = TransferData::decrypt(view_pair.view, None, data)
        .map_err(|e| JsError::new(&format!("{e}")))?;
    Ok(transfer_data.into())
}

/// Decrypt the tx data.
#[wasm_bindgen]
pub async fn decrypt_tx_data(view_pair: &str, data: &[u8]) -> Result<JsTxData, JsError> {
    init_logger();
    let view_pair = str_to_view_pair(view_pair)?;
    let tx_data = TxData::decrypt(view_pair.view, Some(view_pair.view.to_public_key()), data)
        .map_err(|e| JsError::new(&format!("{e}")))?;
    Ok(tx_data.into())
}

#[wasm_bindgen]
pub async fn generate_auth_for_store_vault(
    view_pair: &str,
    use_s3: bool,
) -> Result<JsAuth, JsError> {
    init_logger();
    let view_pair = str_to_view_pair(view_pair)?;
    let auth = if use_s3 {
        generate_auth_for_get_data_sequence_s3(view_pair.view)
    } else {
        generate_auth_for_get_data_sequence(view_pair.view)
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
        .map_err(|e| JsError::new(&format!("failed to convert JsAuth to Auth: {e}")))?;
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
pub async fn get_account_info(config: &Config, public_key: &str) -> Result<JsAccountInfo, JsError> {
    init_logger();
    let pubkey = parse_bytes32(public_key)?.into();
    let client = get_client(config);
    let account_info = client.validity_prover.get_account_info(pubkey).await?;
    Ok(account_info.into())
}

#[wasm_bindgen]
pub async fn get_deposit_info(
    config: &Config,
    pubkey_salt_hash: &str,
) -> Result<Option<JsDepositInfo>, JsError> {
    init_logger();
    let pubkey_salt_hash = parse_bytes32(pubkey_salt_hash)?;
    let client = get_client(config);
    let deposit_info = client
        .validity_prover
        .get_deposit_info(pubkey_salt_hash)
        .await?;
    Ok(deposit_info.map(JsDepositInfo::from))
}

#[wasm_bindgen]
pub async fn sign_message(private_key: &str, message: &[u8]) -> Result<JsFlatG2, JsError> {
    init_logger();
    let key = str_to_keyset(private_key)?;
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
pub fn calc_simple_aggregated_pubkey(signers: Vec<String>) -> Result<String, JsError> {
    init_logger();
    let signers: Vec<U256> = signers
        .iter()
        .map(|s| U256::from_hex(s).map_err(|_| JsError::new("Failed to parse public key")))
        .collect::<Result<Vec<_>, _>>()?;
    let aggregated_pubkey = multisig::simple_aggregated_pubkey(&signers);

    Ok(aggregated_pubkey
        .map_err(|_| JsError::new("Failed to calculate aggregated public key"))?
        .to_hex())
}

#[wasm_bindgen]
pub fn encrypt_message(pubkey: &str, data: &[u8]) -> Vec<u8> {
    init_logger();
    let pubkey = U256::from_hex(pubkey)
        .map_err(|_| JsError::new("Failed to parse public key"))
        .unwrap();

    encrypt_bls(pubkey, data, &mut default_rng())
}

#[wasm_bindgen]
pub fn decrypt_bls_interaction_step1(
    client_key: &str,
    encrypted_data: &[u8],
) -> Result<JsMultiEciesStep1Response, JsError> {
    init_logger();
    let client_key = str_to_keyset(client_key)?;
    let response_step1 =
        multisig_encryption::decrypt_bls_interaction_step1(client_key, encrypted_data);

    Ok(JsMultiEciesStep1Response {
        encrypted_data: response_step1.encrypted_data,
        client_pubkey: response_step1.client_pubkey.to_hex(),
    })
}

#[wasm_bindgen]
pub fn decrypt_bls_interaction_step2(
    server_key: &str,
    step1_response: &JsMultiEciesStep1Response,
) -> Result<JsMultiEciesStep2Response, JsError> {
    init_logger();
    let server_key = str_to_keyset(server_key)?;
    let response_step2 = multisig_encryption::decrypt_bls_interaction_step2(
        server_key,
        &step1_response.try_into().unwrap(),
    )
    .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(response_step2.into())
}

#[wasm_bindgen]
pub fn decrypt_bls_interaction_step3(
    client_key: &str,
    step1_response: &JsMultiEciesStep1Response,
    step2_response: &JsMultiEciesStep2Response,
) -> Result<JsMultiEciesStep3Response, JsError> {
    init_logger();
    let client_key = str_to_keyset(client_key)?;
    let response_step3 = multisig_encryption::decrypt_bls_interaction_step3(
        client_key,
        &step1_response.try_into().unwrap(),
        &step2_response.try_into().unwrap(),
    )
    .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(JsMultiEciesStep3Response {
        message: response_step3.message,
    })
}

#[wasm_bindgen]
pub fn multi_signature_interaction_step1(
    client_private_key: &str,
    message: &[u8],
) -> Result<JsMultisigStep1Response, JsError> {
    init_logger();
    let client_key = str_to_keyset(client_private_key)?;
    let response_step1 = multisig::multi_signature_interaction_step1(client_key, message);

    Ok(JsMultisigStep1Response::from(response_step1))
}

#[wasm_bindgen]
pub fn multi_signature_interaction_step2(
    server_private_key: &str,
    step1_response: &JsMultisigStep1Response,
) -> Result<JsMultisigStep2Response, JsError> {
    init_logger();
    let server_key = str_to_keyset(server_private_key)?;
    let response_step2 = multisig::multi_signature_interaction_step2(
        server_key,
        &step1_response.try_into().unwrap(),
    );

    Ok(JsMultisigStep2Response::from(response_step2))
}

#[wasm_bindgen]
pub fn multi_signature_interaction_step3(
    client_private_key: &str,
    step1_response: &JsMultisigStep1Response,
    step2_response: &JsMultisigStep2Response,
) -> Result<JsMultisigStep3Response, JsError> {
    init_logger();
    let client_key = str_to_keyset(client_private_key)?;
    let response_step3 = multisig::multi_signature_interaction_step3(
        client_key,
        &step1_response.try_into().unwrap(),
        &step2_response.try_into().unwrap(),
    )
    .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(JsMultisigStep3Response::from(response_step3))
}
