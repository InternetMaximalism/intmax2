use intmax2_interfaces::{api::block_builder::interface::Fee, data::user_data::UserData};
use intmax2_zkp::{
    common::{
        private_state::FullPrivateState, salt::Salt, transfer::Transfer,
        trees::transfer_tree::TransferTree, tx::Tx, witness::spent_witness::SpentWitness,
    },
    constants::{NUM_TRANSFERS_IN_TX, TRANSFER_TREE_HEIGHT},
};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};

use super::error::SyncError;

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

pub fn generate_salt() -> Salt {
    let mut rng = rand::thread_rng();
    Salt::rand(&mut rng)
}

pub fn quote_withdrawal_claim_fee(
    fee_token_index: Option<u32>,
    fees: Option<Vec<Fee>>,
) -> Result<Option<Fee>, SyncError> {
    if fees.is_none() {
        return Ok(None);
    }
    let fees = fees.unwrap();
    let fee_token_index =
        fee_token_index.ok_or(SyncError::FeeError("fee token index is needed".to_string()))?;
    let fee = fees
        .iter()
        .find(|fee| fee.token_index == fee_token_index)
        .ok_or(SyncError::FeeError(format!(
            "fee with token index {} not found",
            fee_token_index
        )))?;
    Ok(Some(fee.clone()))
}

pub fn generate_spent_witness(
    full_private_state: &FullPrivateState,
    tx_nonce: u32,
    transfers: &[Transfer],
) -> Result<SpentWitness, SyncError> {
    let transfer_tree = generate_transfer_tree(transfers);
    let tx = Tx {
        nonce: tx_nonce,
        transfer_tree_root: transfer_tree.get_root(),
    };
    let new_salt = generate_salt();
    let spent_witness = SpentWitness::new(
        &full_private_state.asset_tree,
        &full_private_state.to_private_state(),
        &transfer_tree.leaves(), // this is padded
        tx,
        new_salt,
    )
    .map_err(|e| {
        SyncError::WitnessGenerationError(format!("failed to generate spent witness: {}", e))
    })?;
    Ok(spent_witness)
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
