use serde::{Deserialize, Serialize};

use super::{encryption::Encryption, validation::Validation};
use intmax2_zkp::{
    common::{trees::tx_tree::TxMerkleProof, witness::spent_witness::SpentWitness},
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

    // Ephemeral key to query the sender proof set
    // This is not necessary for sender but added for logging purpose
    pub sender_proof_set_ephemeral_key: U256,
}

impl Encryption for TxData {}

impl Validation for TxData {
    fn validate(&self, _pubkey: U256) -> anyhow::Result<()> {
        let tx_tree_root: PoseidonHashOut = self.tx_tree_root.try_into()?;
        self.tx_merkle_proof
            .verify(&self.spent_witness.tx, self.tx_index as u64, tx_tree_root)?;
        Ok(())
    }
}
