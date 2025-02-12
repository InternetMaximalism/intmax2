use intmax2_interfaces::{
    api::{
        error::ServerError,
        store_vault_server::interface::{DataType, StoreVaultClientInterface},
        validity_prover::interface::ValidityProverClientInterface,
    },
    data::{
        encryption::{errors::EncryptionError, Encryption},
        proof_compression::ProofCompressionError,
        transfer_data::TransferData,
        validation::Validation as _,
    },
};
use intmax2_zkp::{
    circuits::balance::send::spent_circuit::SpentPublicInputs,
    common::{generic_address::GenericAddress, signature::key_set::KeySet, transfer::Transfer},
};
use thiserror::Error;

use super::strategy::{common::fetch_sender_proof_set, error::StrategyError};

#[derive(Debug, Error)]
pub enum ReceiveValidationError {
    #[error("Server client error: {0}")]
    ServerError(#[from] ServerError),

    #[error("Encryption error: {0}")]
    EncryptionError(#[from] EncryptionError),

    #[error("Proof compression error: {0}")]
    ProofCompressionError(#[from] ProofCompressionError),

    #[error("Balance insufficient before sync")]
    BalanceInsufficientBeforeSync,

    #[error("Strategy error: {0}")]
    StrategyError(#[from] StrategyError),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("General error: {0}")]
    GeneralError(String),
}

/// Validate the Transfer corresponding to the given transfer_uuid.
pub async fn validate_receive<S: StoreVaultClientInterface, V: ValidityProverClientInterface>(
    store_vault_server: &S,
    validity_prover: &V,
    key: KeySet,
    transfer_uuid: &str,
) -> Result<Transfer, ReceiveValidationError> {
    let encrypted_transfer_data_with_meta_data = store_vault_server
        .get_data_batch(key, DataType::Transfer, &[transfer_uuid.to_string()])
        .await?;
    let transfer_data =
        TransferData::decrypt(&encrypted_transfer_data_with_meta_data[0].data, key)?;
    transfer_data
        .validate(key)
        .map_err(|e| StrategyError::ValidationError(e.to_string()))?;

    let recipient = transfer_data.transfer.recipient;
    if recipient != GenericAddress::from_pubkey(key.pubkey) {
        return Err(ReceiveValidationError::GeneralError(
            "Recipient is not the same as the key".to_string(),
        ));
    }
    // check if tx_tree_root included on the block
    let block_number = validity_prover
        .get_block_number_by_tx_tree_root(transfer_data.tx_tree_root)
        .await?;
    if block_number.is_none() {
        return Err(ReceiveValidationError::GeneralError(
            "Tx tree root is not included on any block".to_string(),
        ));
    }
    let sender_proof_set = fetch_sender_proof_set(
        store_vault_server,
        transfer_data.sender_proof_set_ephemeral_key,
    )
    .await?;
    sender_proof_set.validate(KeySet::dummy()).map_err(|e| {
        ReceiveValidationError::ValidationError(format!(
            "Failed to validate sender proof set: {}",
            e
        ))
    })?;

    // validate spent proof pis
    let spent_proof = sender_proof_set.spent_proof.decompress()?;
    let spent_pis = SpentPublicInputs::from_pis(&spent_proof.public_inputs);
    if spent_pis.tx != transfer_data.tx {
        return Err(ReceiveValidationError::GeneralError(
            "Tx in spent proof is not the same as transfer witness tx".to_string(),
        ));
    }
    let insufficient_flag = spent_pis
        .insufficient_flags
        .random_access(transfer_data.transfer_index as usize);
    if insufficient_flag {
        return Err(ReceiveValidationError::GeneralError(
            "Insufficient flag is on in spent proof".to_string(),
        ));
    }
    Ok(transfer_data.transfer)
}
