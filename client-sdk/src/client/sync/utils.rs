use intmax2_interfaces::{
    api::validity_prover::interface::ValidityProverClientInterface, data::user_data::UserData,
};
use intmax2_zkp::{
    common::{salt::Salt, transfer::Transfer, trees::transfer_tree::TransferTree},
    constants::{NUM_TRANSFERS_IN_TX, TRANSFER_TREE_HEIGHT},
};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};

use crate::external_api::utils::time::sleep_for;

use super::error::SyncError;

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

pub fn generate_salt() -> Salt {
    let mut rng = rand::thread_rng();
    Salt::rand(&mut rng)
}

pub fn generate_transfer_tree(transfers: &[Transfer]) -> TransferTree {
    let mut transfers = transfers.to_vec();
    transfers.resize(NUM_TRANSFERS_IN_TX, Transfer::default());
    let mut transfer_tree = TransferTree::new(TRANSFER_TREE_HEIGHT);
    for transfer in &transfers {
        transfer_tree.push(*transfer);
    }
    transfer_tree
}

pub fn get_balance_proof(
    user_data: &UserData,
) -> Result<Option<ProofWithPublicInputs<F, C, D>>, SyncError> {
    let balance_proof = user_data
        .balance_proof
        .as_ref()
        .map(|bp| bp.decompress())
        .transpose()?;
    Ok(balance_proof)
}

const MAX_VALIDITY_PROVER_SYNC_TRIES: u32 = 8;
const VALIDITY_PROVER_SYNC_SLEEP_TIME: u64 = 10;

pub async fn wait_till_validity_prover_synced<V: ValidityProverClientInterface>(
    validity_prover: &V,
    block_number: u32,
) -> Result<(), SyncError> {
    let mut tries = 0;
    let mut synced_block_number = validity_prover.get_block_number().await?;
    while synced_block_number < block_number {
        if tries > MAX_VALIDITY_PROVER_SYNC_TRIES {
            return Err(SyncError::ValidityProverIsNotSynced(format!(
                "tried to sync block number {} for {} times but still not synced",
                block_number, tries
            )));
        }
        tries += 1;
        log::warn!(
            "validity prover is not synced with block number {}, current block number is {}",
            block_number,
            synced_block_number
        );

        sleep_for(VALIDITY_PROVER_SYNC_SLEEP_TIME).await;
        synced_block_number = validity_prover.get_block_number().await?;
    }

    let mut validity_proof_block_number = validity_prover.get_validity_proof_block_number().await?;
    while validity_proof_block_number < block_number {
        if tries > MAX_VALIDITY_PROVER_SYNC_TRIES {
            return Err(SyncError::ValidityProverIsNotSynced(format!(
                "tried to sync validity proof block number {} for {} times but still not synced",
                block_number, tries
            )));
        }
        tries += 1;
        log::warn!(
            "validity prover is not synced with validity proof block number {}, current validity proof block number is {}",
            block_number,
            validity_proof_block_number
        );
        sleep_for(VALIDITY_PROVER_SYNC_SLEEP_TIME).await;
        validity_proof_block_number = validity_prover.get_validity_proof_block_number().await?;
    }
    Ok(())
}
