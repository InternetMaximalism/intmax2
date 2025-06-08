use serde::{Deserialize, Serialize};

use intmax2_zkp::{
    common::{
        transfer::Transfer,
        trees::{transfer_tree::TransferMerkleProof, tx_tree::TxMerkleProof},
        tx::Tx,
    },
    ethereum_types::{bytes32::Bytes32, u256::U256},
    utils::poseidon_hash_out::PoseidonHashOut,
};

use crate::{
    data::encryption::errors::BlsEncryptionError,
    utils::{
        address::PaymentId,
        key::{PublicKey, PublicKeyPair},
    },
};

use super::{encryption::BlsEncryption, sender_proof_set::SenderProofSet, validation::Validation};

/// Backup data for receiving transfers
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyTransferData {
    // Ephemeral key to query the sender proof set
    pub sender_proof_set_ephemeral_key: U256,
    // After fetching sender proof set, this will be filled
    pub sender_proof_set: Option<SenderProofSet>,

    pub sender: U256,
    pub tx: Tx,
    pub tx_index: u32,
    pub tx_merkle_proof: TxMerkleProof,
    pub tx_tree_root: Bytes32,
    pub transfer: Transfer,
    pub transfer_index: u32,
    pub transfer_merkle_proof: TransferMerkleProof,
}

impl LegacyTransferData {
    fn into_latest(self) -> TransferData {
        let sender = PublicKeyPair {
            view: PublicKey(self.sender), // use the same key as spend key for migration
            spend: PublicKey(self.sender),
        };
        TransferData {
            sender_proof_set_ephemeral_key: self.sender_proof_set_ephemeral_key,
            sender_proof_set: self.sender_proof_set,
            sender,
            payment_id: None,
            tx: self.tx,
            tx_index: self.tx_index,
            tx_merkle_proof: self.tx_merkle_proof,
            tx_tree_root: self.tx_tree_root,
            transfer: self.transfer,
            transfer_index: self.transfer_index,
            transfer_merkle_proof: self.transfer_merkle_proof,
        }
    }
}

/// Backup data for receiving transfers
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferData {
    // Ephemeral key to query the sender proof set
    pub sender_proof_set_ephemeral_key: U256,
    // After fetching sender proof set, this will be filled
    pub sender_proof_set: Option<SenderProofSet>,
    pub sender: PublicKeyPair,
    pub payment_id: Option<PaymentId>,
    pub description_hash: Option<Bytes32>,
    pub inner_salt: Salt,
    pub tx: Tx,
    pub tx_index: u32,
    pub tx_merkle_proof: TxMerkleProof,
    pub tx_tree_root: Bytes32,
    pub transfer: Transfer,
    pub transfer_index: u32,
    pub transfer_merkle_proof: TransferMerkleProof,
}

impl TransferData {
    pub fn set_sender_proof_set(&mut self, sender_proof_set: SenderProofSet) {
        self.sender_proof_set = Some(sender_proof_set);
    }
}

impl BlsEncryption for TransferData {
    fn from_bytes(bytes: &[u8], version: u8) -> Result<Self, BlsEncryptionError> {
        match version {
            1 => {
                let legacy_data: LegacyTransferData = bincode::deserialize(bytes)?;
                Ok(legacy_data.into_latest())
            }
            2 => Ok(bincode::deserialize(bytes)?),
            _ => Err(BlsEncryptionError::UnsupportedVersion(version)),
        }
    }
}

impl Validation for TransferData {
    fn validate(&self, _pubkey: U256) -> anyhow::Result<()> {
        let tx_tree_root: PoseidonHashOut = self.tx_tree_root.try_into()?;
        self.tx_merkle_proof
            .verify(&self.tx, self.tx_index as u64, tx_tree_root)?;
        self.transfer_merkle_proof.verify(
            &self.transfer,
            self.transfer_index as u64,
            self.tx.transfer_tree_root,
        )?;
        Ok(())
    }
}
