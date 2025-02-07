use std::collections::HashMap;

use intmax2_client_sdk::{
    client::strategy::common::fetch_sender_proof_set,
    external_api::store_vault_server::StoreVaultServerClient,
};
use intmax2_interfaces::{
    api::block_builder::interface::FeeProof,
    data::{sender_proof_set::SenderProofSet, validation::Validation},
};
use intmax2_zkp::{
    circuits::balance::send::spent_circuit::SpentPublicInputs,
    common::{signature::key_set::KeySet, witness::transfer_witness::TransferWitness},
    ethereum_types::u256::U256,
};

use super::error::FeeError;

pub async fn validate_fee_proof(
    store_vault_server_client: StoreVaultServerClient,
    beneficiary_pubkey: U256,
    required_fee: &HashMap<u32, U256>,
    required_collateral_fee: Option<&HashMap<u32, U256>>,
    fee_proof: &FeeProof,
) -> Result<(), FeeError> {
    let sender_proof_set = fetch_sender_proof_set(
        &store_vault_server_client,
        fee_proof.sender_proof_set_ephemeral_key,
    )
    .await?;

    // validate main fee
    validate_fee_single(
        beneficiary_pubkey,
        required_fee,
        &sender_proof_set,
        &fee_proof.fee_transfer_witness,
    )
    .await?;

    // validate collateral fee
    if let Some(collateral_fee) = required_collateral_fee {
        if fee_proof.collateral_block.is_none() {
            return Err(FeeError::InvalidFee(
                "Collateral block is missing".to_string(),
            ));
        }
        let collateral_block = fee_proof.collateral_block.as_ref().unwrap();
        let sender_proof_set = SenderProofSet {
            spent_proof: collateral_block.spent_proof.clone(),
            prev_balance_proof: sender_proof_set.prev_balance_proof,
        };
        let transfer_witness = &collateral_block.fee_transfer_witness;
        validate_fee_single(
            beneficiary_pubkey,
            collateral_fee,
            &sender_proof_set,
            transfer_witness,
        )
        .await?;
    }
    Ok(())
}

async fn validate_fee_single(
    beneficiary_pubkey: U256,
    required_fee: &HashMap<u32, U256>, // token index -> fee amount
    sender_proof_set: &SenderProofSet,
    transfer_witness: &TransferWitness,
) -> Result<(), FeeError> {
    // todo: validate spent proof inside `validate` method
    sender_proof_set.validate(KeySet::dummy()).map_err(|e| {
        FeeError::ProofVerificationError(format!("Failed to validate sender proof set: {}", e))
    })?;

    // validate spent proof pis
    let spent_proof = sender_proof_set.spent_proof.decompress()?;
    let spent_pis = SpentPublicInputs::from_pis(&spent_proof.public_inputs);
    if spent_pis.tx != transfer_witness.tx {
        return Err(FeeError::ProofVerificationError(
            "Tx in spent proof is not the same as transfer witness tx".to_string(),
        ));
    }
    let insufficient_flag = spent_pis
        .insufficient_flags
        .random_access(transfer_witness.transfer_index as usize);
    if insufficient_flag {
        return Err(FeeError::ProofVerificationError(
            "Insufficient flag is on in spent proof".to_string(),
        ));
    }

    // validate transfer witness
    transfer_witness
        .transfer_merkle_proof
        .verify(
            &transfer_witness.transfer,
            transfer_witness.transfer_index as u64,
            transfer_witness.tx.transfer_tree_root,
        )
        .map_err(|e| {
            FeeError::MerkleTreeError(format!("Failed to verify transfer merkle proof: {}", e))
        })?;

    // make sure that transfer is for beneficiary account
    let recipient = transfer_witness.transfer.recipient;
    if !recipient.is_pubkey {
        return Err(FeeError::InvalidRecipient(
            "Recipient is not a pubkey".to_string(),
        ));
    }
    let recipient = recipient.to_pubkey().unwrap();
    if recipient != beneficiary_pubkey {
        return Err(FeeError::InvalidRecipient(
            "Recipient is not the beneficiary".to_string(),
        ));
    }

    // make sure that the fee is correct
    if !required_fee.contains_key(&transfer_witness.transfer.token_index) {
        return Err(FeeError::InvalidFee(
            "Fee token index is not correct".to_string(),
        ));
    }
    let requested_fee = required_fee
        .get(&transfer_witness.transfer.token_index)
        .unwrap();
    if transfer_witness.transfer.amount < *requested_fee {
        return Err(FeeError::InvalidFee(format!(
            "Transfer amount is not enough: requested_fee: {}, transfer_amount: {}",
            requested_fee, transfer_witness.transfer.amount
        )));
    }
    Ok(())
}
