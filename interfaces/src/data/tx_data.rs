use serde::{Deserialize, Serialize};

use super::{encryption::BlsEncryption, transfer_data::TransferData, validation::Validation};
use intmax2_zkp::{
    common::{
        error::CommonError,
        trees::{transfer_tree::TransferTree, tx_tree::TxMerkleProof},
        witness::spent_witness::SpentWitness,
    },
    constants::TRANSFER_TREE_HEIGHT,
    ethereum_types::{bytes32::Bytes32, u256::U256},
    utils::poseidon_hash_out::PoseidonHashOut,
};

// tx data for syncing sender's balance proof
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxData {
    pub tx_index: u32,
    pub tx_merkle_proof: TxMerkleProof,
    pub tx_tree_root: Bytes32,
    pub spent_witness: SpentWitness, // to update sender's private state
    pub transfer_digests: Vec<Bytes32>, // transfer digests
    pub transfer_types: Vec<String>, // transfer types

    // Ephemeral key to query the sender proof set
    pub sender_proof_set_ephemeral_key: U256,
}

impl TxData {
    pub fn get_transfer_data(
        &self,
        sender: U256,
        transfer_index: u32,
    ) -> Result<TransferData, CommonError> {
        let transfers = self.spent_witness.transfers.clone();
        if transfer_index >= transfers.len() as u32 {
            return Err(CommonError::InvalidData(format!(
                "transfer index: {transfer_index} is out of range"
            )));
        }
        let mut transfer_tree = TransferTree::new(TRANSFER_TREE_HEIGHT);
        for transfer in &transfers {
            transfer_tree.push(*transfer);
        }
        let transfer_merkle_tree = transfer_tree.prove(transfer_index as u64);
        Ok(TransferData {
            sender_proof_set_ephemeral_key: self.sender_proof_set_ephemeral_key,
            sender_proof_set: None,
            sender,
            tx: self.spent_witness.tx,
            tx_index: self.tx_index,
            tx_merkle_proof: self.tx_merkle_proof.clone(),
            tx_tree_root: self.tx_tree_root,
            transfer: transfers[transfer_index as usize],
            transfer_index,
            transfer_merkle_proof: transfer_merkle_tree,
        })
    }
}

impl BlsEncryption for TxData {}

impl Validation for TxData {
    fn validate(&self, _pubkey: U256) -> anyhow::Result<()> {
        let tx_tree_root: PoseidonHashOut = self.tx_tree_root.try_into()?;
        self.tx_merkle_proof
            .verify(&self.spent_witness.tx, self.tx_index as u64, tx_tree_root)?;
        Ok(())
    }
}
