use ark_std::Zero;
use plonky2::{
    field::extension::Extendable, hash::hash_types::RichField, plonk::config::GenericConfig,
};
use serde::{Deserialize, Serialize};

use intmax2_zkp::{
    common::{
        signature::key_set::KeySet, transfer::Transfer, trees::transfer_tree::TransferMerkleProof,
    },
    ethereum_types::u256::U256,
    utils::poseidon_hash_out::PoseidonHashOut,
};

use super::{
    common_tx_data::CommonTxData,
    encryption::{decrypt, encrypt},
};

// backup data for receiving transfers
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(bound = "")]
pub struct TransferData<F, C, const D: usize>
where
    F: RichField + Extendable<D>,
    C: GenericConfig<D, F = F>,
{
    // Info to query the sender's prev balance proof
    pub sender: U256,
    pub prev_block_number: u32,
    pub prev_private_commitment: PoseidonHashOut,

    // Info to update the sender's balance proof
    pub tx_data: CommonTxData<F, C, D>,

    // Used for updating receiver's balance proof
    pub transfer: Transfer,
    pub transfer_index: u32,
    pub transfer_merkle_proof: TransferMerkleProof,
}

impl<F, C, const D: usize> TransferData<F, C, D>
where
    F: RichField + Extendable<D>,
    C: GenericConfig<D, F = F>,
{
    fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }

    fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        let data = bincode::deserialize(bytes)?;
        Ok(data)
    }

    pub fn encrypt(&self, pubkey: U256) -> Vec<u8> {
        encrypt(pubkey, &self.to_bytes())
    }

    pub fn decrypt(bytes: &[u8], key: KeySet) -> anyhow::Result<Self> {
        if key.privkey.is_zero() {
            anyhow::bail!("Invalid private key");
        }

        let data = decrypt(key, bytes)?;
        let data = Self::from_bytes(&data)?;
        data.validate(key)?;
        Ok(data)
    }

    pub fn validate(&self, _key: KeySet) -> anyhow::Result<()> {
        self.tx_data
            .validate()
            .map_err(|e| anyhow::anyhow!("tx data validation failed: {}", e))?;
        self.transfer_merkle_proof
            .verify(
                &self.transfer,
                self.transfer_index as u64,
                self.tx_data.tx.transfer_tree_root,
            )
            .map_err(|e| anyhow::anyhow!("transfer merkle proof validation failed: {}", e))?;
        Ok(())
    }
}
