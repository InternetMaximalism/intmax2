use intmax2_interfaces::{
    api::{
        balance_prover::interface::BalanceProverClientInterface,
        validity_prover::interface::ValidityProverClientInterface,
    },
    data::{deposit_data::DepositData, transfer_data::TransferData, tx_data::TxData},
};
use intmax2_zkp::{
    circuits::balance::{
        balance_pis::BalancePublicInputs, balance_processor::get_prev_balance_pis,
        send::spent_circuit::SpentPublicInputs,
    },
    common::{
        private_state::FullPrivateState,
        salt::Salt,
        signature::key_set::KeySet,
        witness::{
            deposit_witness::DepositWitness, private_transition_witness::PrivateTransitionWitness,
            receive_deposit_witness::ReceiveDepositWitness,
            receive_transfer_witness::ReceiveTransferWitness, transfer_witness::TransferWitness,
            tx_witness::TxWitness,
        },
    },
    ethereum_types::{bytes32::Bytes32, u256::U256},
    utils::leafable::Leafable as _,
};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};

use crate::client::{
    strategy::utils::wait_till_validity_prover_synced, sync::utils::generate_spent_witness,
};

use super::error::SyncError;

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

pub async fn receive_deposit(
    validity_prover: &dyn ValidityProverClientInterface,
    balance_prover: &dyn BalanceProverClientInterface,
    key: KeySet,
    full_private_state: &mut FullPrivateState,
    new_salt: Salt,
    prev_balance_proof: &Option<ProofWithPublicInputs<F, C, D>>,
    deposit_data: &DepositData,
) -> Result<ProofWithPublicInputs<F, C, D>, SyncError> {
    let prev_balance_pis = get_prev_balance_pis(key.pubkey, prev_balance_proof);
    let receive_block_number = prev_balance_pis.public_state.block_number;
    // Generate witness
    let deposit_info = validity_prover
        .get_deposit_info(deposit_data.deposit_hash().unwrap())
        .await?
        .ok_or(SyncError::DepositInfoNotFound(
            deposit_data.deposit_hash().unwrap(),
        ))?;
    if receive_block_number < deposit_info.block_number {
        return Err(SyncError::InternalError(
            "Deposit block number is greater than receive block number".to_string(),
        ));
    }
    let deposit_merkle_proof = validity_prover
        .get_deposit_merkle_proof(receive_block_number, deposit_info.deposit_index)
        .await?;
    let deposit_witness = DepositWitness {
        deposit_salt: deposit_data.deposit_salt,
        deposit_index: deposit_info.deposit_index,
        deposit: deposit_data.deposit().unwrap(),
        deposit_merkle_proof,
    };
    let deposit = deposit_data.deposit().unwrap();
    let nullifier: Bytes32 = deposit.poseidon_hash().into();
    let private_transition_witness = PrivateTransitionWitness::new(
        full_private_state,
        deposit.token_index,
        deposit.amount,
        nullifier,
        new_salt,
    )
    .map_err(|e| SyncError::WitnessGenerationError(format!("PrivateTransitionWitness {}", e)))?;
    let receive_deposit_witness = ReceiveDepositWitness {
        deposit_witness,
        private_transition_witness,
    };

    // prove deposit
    let balance_proof = balance_prover
        .prove_receive_deposit(
            key,
            key.pubkey,
            &receive_deposit_witness,
            prev_balance_proof,
        )
        .await?;

    Ok(balance_proof)
}

#[allow(clippy::too_many_arguments)]
pub async fn receive_transfer(
    validity_prover: &dyn ValidityProverClientInterface,
    balance_prover: &dyn BalanceProverClientInterface,
    key: KeySet,
    full_private_state: &mut FullPrivateState,
    new_salt: Salt,
    sender_balance_proof: &ProofWithPublicInputs<F, C, D>, /* sender's balance proof after
                                                            * applying tx */
    prev_balance_proof: &Option<ProofWithPublicInputs<F, C, D>>, /* receiver's prev balance
                                                                  * proof */
    transfer_data: &TransferData,
) -> Result<ProofWithPublicInputs<F, C, D>, SyncError> {
    let prev_balance_pis = get_prev_balance_pis(key.pubkey, prev_balance_proof);
    let receive_block_number = prev_balance_pis.public_state.block_number;
    let sender_balance_pis = BalancePublicInputs::from_pis(&sender_balance_proof.public_inputs);
    if receive_block_number < prev_balance_pis.public_state.block_number {
        return Err(SyncError::InternalError(
            "receive block number is not greater than prev balance proof".to_string(),
        ));
    }
    if sender_balance_pis.last_tx_hash != transfer_data.tx.hash() {
        return Err(SyncError::InternalError(format!(
            "last_tx_hash mismatch last_tx_hash: {} != tx_hash: {}",
            sender_balance_pis.last_tx_hash,
            transfer_data.tx.hash()
        )));
    }
    if sender_balance_pis
        .last_tx_insufficient_flags
        .random_access(transfer_data.transfer_index as usize)
    {
        return Err(SyncError::InternalError(
            "last_tx_insufficient_flags is true".to_string(),
        ));
    }

    // Generate witness
    let transfer_witness = TransferWitness {
        tx: transfer_data.tx,
        transfer: transfer_data.transfer,
        transfer_index: transfer_data.transfer_index,
        transfer_merkle_proof: transfer_data.transfer_merkle_proof.clone(),
    };
    let nullifier: Bytes32 = transfer_witness.transfer.commitment().into();
    let private_transition_witness = PrivateTransitionWitness::new(
        full_private_state,
        transfer_data.transfer.token_index,
        transfer_data.transfer.amount,
        nullifier,
        new_salt,
    )
    .map_err(|e| SyncError::WitnessGenerationError(format!("PrivateTransitionWitness {}", e)))?;
    let block_merkle_proof = validity_prover
        .get_block_merkle_proof(
            receive_block_number,
            sender_balance_pis.public_state.block_number,
        )
        .await?;
    let receive_transfer_witness = ReceiveTransferWitness {
        transfer_witness,
        private_transition_witness,
        sender_balance_proof: sender_balance_proof.clone(),
        block_merkle_proof,
    };

    // prove transfer
    let balance_proof = balance_prover
        .prove_receive_transfer(
            key,
            key.pubkey,
            &receive_transfer_witness,
            prev_balance_proof,
        )
        .await?;

    Ok(balance_proof)
}

pub async fn update_send_by_sender(
    validity_prover: &dyn ValidityProverClientInterface,
    balance_prover: &dyn BalanceProverClientInterface,
    key: KeySet,
    full_private_state: &mut FullPrivateState,
    prev_balance_proof: &Option<ProofWithPublicInputs<F, C, D>>,
    tx_block_number: u32,
    tx_data: &TxData,
) -> Result<ProofWithPublicInputs<F, C, D>, SyncError> {
    // sync check
    wait_till_validity_prover_synced(validity_prover, true, tx_block_number).await?;
    let prev_balance_pis = get_prev_balance_pis(key.pubkey, prev_balance_proof);
    if tx_block_number <= prev_balance_pis.public_state.block_number {
        return Err(SyncError::InternalError(
            "tx block number is not greater than prev balance proof".to_string(),
        ));
    }
    if prev_balance_pis.private_commitment != full_private_state.to_private_state().commitment() {
        return Err(SyncError::InternalError(
            "prev balance proof private commitment is not equal to full private state commitment"
                .to_string(),
        ));
    }

    // get witness
    let validity_witness = validity_prover
        .get_validity_witness(tx_block_number)
        .await?;
    let validity_pis = validity_witness.to_validity_pis().map_err(|e| {
        SyncError::InternalError(format!(
            "failed to convert validity witness to validity public inputs: {}",
            e
        ))
    })?;
    let sender_leaves = validity_witness.block_witness.get_sender_tree().leaves();
    let tx_witness = TxWitness {
        validity_pis: validity_pis.clone(),
        sender_leaves: sender_leaves.clone(),
        tx: tx_data.spent_witness.tx,
        tx_index: tx_data.tx_index,
        tx_merkle_proof: tx_data.tx_merkle_proof.clone(),
    };
    let update_witness = validity_prover
        .get_update_witness(
            key.pubkey,
            tx_block_number,
            prev_balance_pis.public_state.block_number,
            true,
        )
        .await?;
    log::info!(
        "update_witness.last_block_number: {}, tx_block_number: {}",
        update_witness.get_last_block_number(),
        tx_block_number
    );

    let sender_leaf = sender_leaves
        .iter()
        .find(|leaf| leaf.sender == key.pubkey)
        .ok_or(SyncError::InternalError(
            "sender leaf not found in sender leaves".to_string(),
        ))?;
    // update private state only if sender leaf has returned signature and validity_pis is valid
    let update_private_state = sender_leaf.did_return_sig && validity_pis.is_valid_block;
    let spent_witness = generate_spent_witness(
        full_private_state,
        tx_data.spent_witness.tx.nonce,
        &tx_data.spent_witness.transfers,
    )
    .await?;
    if update_private_state {
        spent_witness
            .update_private_state(full_private_state)
            .map_err(|e| SyncError::FailedToUpdatePrivateState(e.to_string()))?;
    }
    let spent_proof = balance_prover.prove_spent(key, &spent_witness).await?;
    let balance_proof = balance_prover
        .prove_send(
            key,
            key.pubkey,
            &tx_witness,
            &update_witness,
            &spent_proof,
            prev_balance_proof,
        )
        .await?;
    let balance_pis = BalancePublicInputs::from_pis(&balance_proof.public_inputs);
    if balance_pis.private_commitment != full_private_state.to_private_state().commitment() {
        return Err(SyncError::InternalError(format!(
            "balance proof new private commitment {} is not equal to full private state commitment{}",
            balance_pis.private_commitment, full_private_state.to_private_state().commitment(
        ))
        ));
    }
    Ok(balance_proof)
}

/// Update balance proof to the tx specified by tx_block_number and common_tx_data by receiver side.
pub async fn update_send_by_receiver(
    validity_prover: &dyn ValidityProverClientInterface,
    balance_prover: &dyn BalanceProverClientInterface,
    key: KeySet,
    sender: U256,
    tx_block_number: u32,
    transfer_data: &TransferData,
) -> Result<ProofWithPublicInputs<F, C, D>, SyncError> {
    wait_till_validity_prover_synced(validity_prover, true, tx_block_number).await?;

    // inputs validation
    let sender_proof_set = transfer_data.sender_proof_set.as_ref().unwrap();
    let spent_proof = sender_proof_set.spent_proof.decompress()?;
    let prev_balance_proof = sender_proof_set.prev_balance_proof.decompress()?;
    let prev_balance_pis = BalancePublicInputs::from_pis(&prev_balance_proof.public_inputs);
    let prev_block_number = prev_balance_pis.public_state.block_number;
    if tx_block_number <= prev_block_number {
        return Err(SyncError::InvalidTransferError(
            "tx block number is not greater than prev balance proof".to_string(),
        ));
    }
    if transfer_data.sender_proof_set.is_none() {
        return Err(SyncError::InternalError(
            "sender_proof_set is not set yet".to_string(),
        ));
    }

    let spent_pis = SpentPublicInputs::from_pis(&spent_proof.public_inputs);
    if spent_pis.prev_private_commitment != prev_balance_pis.private_commitment {
        return Err(SyncError::InvalidTransferError(
           format!("balance proof's prev_private_commitment: {} != spent_proof.prev_private_commitment: {}",
              prev_balance_pis.private_commitment, spent_pis.prev_private_commitment)
        ));
    }
    // get witness
    let validity_witness = validity_prover
        .get_validity_witness(tx_block_number)
        .await?;
    let validity_pis = validity_witness.to_validity_pis().map_err(|e| {
        SyncError::InternalError(format!(
            "failed to convert validity witness to validity public inputs: {}",
            e
        ))
    })?;
    let sender_leaves = validity_witness.block_witness.get_sender_tree().leaves();
    // validation
    if !validity_pis.is_valid_block {
        return Err(SyncError::InvalidTransferError(
            "tx included in invalid block".to_string(),
        ));
    }
    let sender_leaf = sender_leaves
        .iter()
        .find(|leaf| leaf.sender == sender)
        .ok_or(SyncError::InvalidTransferError(
            "sender leaf not found in sender leaves".to_string(),
        ))?;
    if !sender_leaf.did_return_sig {
        return Err(SyncError::InvalidTransferError(
            "sender did not return signature".to_string(),
        ));
    }
    let tx_witness = TxWitness {
        validity_pis,
        sender_leaves,
        tx: transfer_data.tx,
        tx_index: transfer_data.tx_index,
        tx_merkle_proof: transfer_data.tx_merkle_proof.clone(),
    };
    let update_witness = validity_prover
        .get_update_witness(
            sender,
            tx_block_number,
            prev_balance_pis.public_state.block_number,
            true,
        )
        .await?;
    let last_block_number = update_witness.get_last_block_number();
    if prev_block_number < last_block_number {
        return Err(SyncError::InvalidTransferError(format!(
            "prev_block_number {} is less than last_block_number {}",
            prev_block_number, last_block_number
        )));
    }
    // prove tx send
    let balance_proof = balance_prover
        .prove_send(
            key,
            sender,
            &tx_witness,
            &update_witness,
            &spent_proof,
            &Some(prev_balance_proof.clone()),
        )
        .await?;

    Ok(balance_proof)
}

/// Update prev_balance_proof to block_number or do noting if already synced later than block_number.
///
/// Assumes that there are no send transactions between the block_number of prev_balance_proof and block_number.
pub async fn update_no_send(
    validity_prover: &dyn ValidityProverClientInterface,
    balance_prover: &dyn BalanceProverClientInterface,
    key: KeySet,
    prev_balance_proof: &Option<ProofWithPublicInputs<F, C, D>>,
    to_block_number: u32,
) -> Result<ProofWithPublicInputs<F, C, D>, SyncError> {
    wait_till_validity_prover_synced(validity_prover, true, to_block_number).await?;
    let prev_balance_pis = get_prev_balance_pis(key.pubkey, prev_balance_proof);
    let prev_block_number = prev_balance_pis.public_state.block_number;
    if to_block_number <= prev_block_number {
        // no need to update balance proof
        return Ok(prev_balance_proof.clone().unwrap());
    }

    // get update witness
    let update_witness = validity_prover
        .get_update_witness(
            key.pubkey,
            to_block_number,
            prev_balance_pis.public_state.block_number,
            false,
        )
        .await?;
    let last_block_number = update_witness.get_last_block_number();
    if prev_block_number < last_block_number {
        return Err(SyncError::InternalError(format!(
            "prev_block_number {} is less than last_block_number {}",
            prev_block_number, last_block_number
        )));
    }
    let balance_proof = balance_prover
        .prove_update(key, key.pubkey, &update_witness, prev_balance_proof)
        .await?;
    Ok(balance_proof)
}
