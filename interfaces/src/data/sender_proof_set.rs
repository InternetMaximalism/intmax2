use intmax2_zkp::circuits::balance::{
    balance_pis::BalancePublicInputs, send::spent_circuit::SpentPublicInputs,
};
use serde::{Deserialize, Serialize};

use crate::{
    data::encryption::errors::BlsEncryptionError, utils::circuit_verifiers::CircuitVerifiers,
};

use super::{
    encryption::BlsEncryption,
    proof_compression::{CompressedBalanceProof, CompressedSpentProof},
    validation::Validation,
};

/// Common data for all transfers in a batch transfer
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SenderProofSet {
    pub spent_proof: CompressedSpentProof,
    pub prev_balance_proof: CompressedBalanceProof,
}

impl BlsEncryption for SenderProofSet {
    fn from_bytes(bytes: &[u8], version: u8) -> Result<Self, BlsEncryptionError> {
        match version {
            1 | 2 => Ok(bincode::deserialize(bytes)?),
            _ => Err(BlsEncryptionError::UnsupportedVersion(version)),
        }
    }
}

impl Validation for SenderProofSet {
    fn validate(&self) -> anyhow::Result<()> {
        let verifiers = CircuitVerifiers::load();
        let spent_proof = self.spent_proof.decompress()?;
        verifiers.get_spent_vd().verify(spent_proof.clone())?;
        let prev_balance_proof = self.prev_balance_proof.decompress()?;
        let balance_vd = CircuitVerifiers::load().get_balance_vd();
        balance_vd.verify(prev_balance_proof.clone())?;
        let spent_pis = SpentPublicInputs::from_pis(&spent_proof.public_inputs)
            .map_err(|e| anyhow::anyhow!("Failed to convert spent proof public inputs: {}", e))?;
        let prev_balance_pis = BalancePublicInputs::from_pis(&prev_balance_proof.public_inputs)?;
        // Validation of public inputs
        if !spent_pis.is_valid {
            anyhow::bail!("Invalid spent proof: is_valid is false");
        }
        if spent_pis.prev_private_commitment != prev_balance_pis.private_commitment {
            anyhow::bail!(
                "Invalid spent proof: prev_private_commitment mismatch: {} != {}",
                spent_pis.prev_private_commitment,
                prev_balance_pis.private_commitment
            );
        }
        Ok(())
    }
}
