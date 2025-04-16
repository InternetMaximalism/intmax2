use base64::{prelude::BASE64_STANDARD, Engine};
use intmax2_interfaces::data::{
    data_type::DataType, encryption::BlsEncryption, meta_data::MetaData,
    transfer_data::TransferData,
};
use intmax2_zkp::{
    common::signature_content::key_set::KeySet,
    ethereum_types::{bytes32::Bytes32, u256::U256},
};
use serde::{Deserialize, Serialize};

use crate::client::receive_validation::validate_receive;

use super::{client::Client, error::ClientError, strategy::common::fetch_single_data};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferReceipt {
    pub data: TransferData,
    pub meta: MetaData,
}

impl BlsEncryption for TransferReceipt {}

pub async fn generate_transfer_receipt(
    client: &Client,
    key: KeySet,
    transfer_digest: Bytes32,
    receiver: U256,
) -> Result<String, ClientError> {
    let (meta, data) = fetch_single_data(
        client.store_vault_server.as_ref(),
        key,
        DataType::Transfer,
        transfer_digest,
    )
    .await?;
    let encrypted_data = TransferReceipt { data, meta }.encrypt(receiver, None)?;
    let encrypted_data_base64 = BASE64_STANDARD.encode(&encrypted_data);
    Ok(encrypted_data_base64)
}

pub async fn validate_transfer_receipt(
    client: &Client,
    key: KeySet,
    transfer_receipt: &str,
) -> Result<TransferData, ClientError> {
    let encrypted_data = BASE64_STANDARD.decode(transfer_receipt).map_err(|e| {
        ClientError::DeserializeError(format!(
            "Failed to decode transfer receipt as base64: {}",
            e
        ))
    })?;
    let transfer_receipt: TransferReceipt = TransferReceipt::decrypt(key, None, &encrypted_data)?;
    validate_receive(
        client.store_vault_server.as_ref(),
        client.validity_prover.as_ref(),
        key.pubkey,
        &transfer_receipt.meta,
        &transfer_receipt.data,
    )
    .await?;
    Ok(transfer_receipt.data.clone())
}
