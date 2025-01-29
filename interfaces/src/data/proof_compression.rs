use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{
        circuit_data::VerifierCircuitData,
        config::PoseidonGoldilocksConfig,
        proof::{CompressedProofWithPublicInputs, ProofWithPublicInputs},
    },
};
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};

use crate::utils::circuit_verifiers::CircuitVerifiers;

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

#[derive(Debug, thiserror::Error)]
pub enum ProofCompressionError {
    #[error("Compression error")]
    CompressionError,

    #[error("Decompression error")]
    DecompressionError,

    #[error("Serialization error")]
    SerializationError,

    #[error("Deserialization error")]
    DeserializationError,
}

type Result<T> = std::result::Result<T, ProofCompressionError>;

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedBalanceProof(#[serde_as(as = "Base64")] pub Vec<u8>);

impl CompressedBalanceProof {
    pub fn new(input: &ProofWithPublicInputs<F, C, D>) -> Result<Self> {
        let balance_vd = CircuitVerifiers::load().get_balance_vd();
        let serialized = serialize(&balance_vd, input)?;
        Ok(Self(serialized))
    }
    pub fn decompress(&self) -> Result<ProofWithPublicInputs<F, C, D>> {
        let balance_vd = CircuitVerifiers::load().get_balance_vd();
        let proof = deserialize(&balance_vd, &self.0)?;
        Ok(proof)
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedValidityProof(#[serde_as(as = "Base64")] pub Vec<u8>);

impl CompressedValidityProof {
    pub fn new(input: &ProofWithPublicInputs<F, C, D>) -> Result<Self> {
        let validity_vd = CircuitVerifiers::load().get_validity_vd();
        let serialized = serialize(&validity_vd, input)?;
        Ok(Self(serialized))
    }
    pub fn decompress(&self) -> Result<ProofWithPublicInputs<F, C, D>> {
        let validity_vd = CircuitVerifiers::load().get_validity_vd();
        let proof = deserialize(&validity_vd, &self.0)?;
        Ok(proof)
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedTransitionProof(#[serde_as(as = "Base64")] pub Vec<u8>);

impl CompressedTransitionProof {
    pub fn new(input: &ProofWithPublicInputs<F, C, D>) -> Result<Self> {
        let transition_vd = CircuitVerifiers::load().get_transition_vd();
        let serialized = serialize(&transition_vd, input)?;
        Ok(Self(serialized))
    }
    pub fn decompress(&self) -> Result<ProofWithPublicInputs<F, C, D>> {
        let transition_vd = CircuitVerifiers::load().get_transition_vd();
        let proof = deserialize(&transition_vd, &self.0)?;
        Ok(proof)
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedSingleWithdrawalProof(#[serde_as(as = "Base64")] pub Vec<u8>);

impl CompressedSingleWithdrawalProof {
    pub fn new(input: &ProofWithPublicInputs<F, C, D>) -> Result<Self> {
        let single_withdrawal_vd = CircuitVerifiers::load().get_single_withdrawal_vd();
        let serialized = serialize(&single_withdrawal_vd, input)?;
        Ok(Self(serialized))
    }
    pub fn decompress(&self) -> Result<ProofWithPublicInputs<F, C, D>> {
        let single_withdrawal_vd = CircuitVerifiers::load().get_single_withdrawal_vd();
        let proof = deserialize(&single_withdrawal_vd, &self.0)?;
        Ok(proof)
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedSingleClaimProof(#[serde_as(as = "Base64")] pub Vec<u8>);

impl CompressedSingleClaimProof {
    pub fn new(input: &ProofWithPublicInputs<F, C, D>) -> Result<Self> {
        // We don't have single_claim_vd yet because of serialization issues
        let serialized =
            bincode::serialize(input).map_err(|_| ProofCompressionError::SerializationError)?;
        Ok(Self(serialized))
    }
    pub fn decompress(&self) -> Result<ProofWithPublicInputs<F, C, D>> {
        // Deserialize directly because we don't have single_claim_vd yet
        let proof: ProofWithPublicInputs<F, C, D> = bincode::deserialize(&self.0)
            .map_err(|_| ProofCompressionError::DeserializationError)?;
        Ok(proof)
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedSpentProof(#[serde_as(as = "Base64")] pub Vec<u8>);

impl CompressedSpentProof {
    pub fn new(input: &ProofWithPublicInputs<F, C, D>) -> Result<Self> {
        // We don't have spent_vd yet because of serialization issues
        let serialized =
            bincode::serialize(input).map_err(|_| ProofCompressionError::SerializationError)?;
        Ok(Self(serialized))
    }
    pub fn decompress(&self) -> Result<ProofWithPublicInputs<F, C, D>> {
        // Deserialize directly because we don't have spent_vd yet
        let proof: ProofWithPublicInputs<F, C, D> = bincode::deserialize(&self.0)
            .map_err(|_| ProofCompressionError::DeserializationError)?;
        Ok(proof)
    }
}

fn serialize(
    vd: &VerifierCircuitData<F, C, D>,
    input: &ProofWithPublicInputs<F, C, D>,
) -> Result<Vec<u8>> {
    let compressed = input
        .clone()
        .compress(&vd.verifier_only.circuit_digest, &vd.common)
        .map_err(|_| ProofCompressionError::CompressionError)?;
    let serialized =
        bincode::serialize(&compressed).map_err(|_| ProofCompressionError::SerializationError)?;
    Ok(serialized)
}

fn deserialize(
    vd: &VerifierCircuitData<F, C, D>,
    bytes: &[u8],
) -> Result<ProofWithPublicInputs<F, C, D>> {
    let compressed: CompressedProofWithPublicInputs<F, C, D> =
        bincode::deserialize(bytes).map_err(|_| ProofCompressionError::DeserializationError)?;
    let proof = compressed
        .decompress(&vd.verifier_only.circuit_digest, &vd.common)
        .map_err(|_| ProofCompressionError::DecompressionError)?;
    Ok(proof)
}

#[cfg(test)]
mod tests {
    use plonky2::recursion::dummy_circuit::cyclic_base_proof;

    use crate::utils::circuit_verifiers::CircuitVerifiers;

    use super::CompressedBalanceProof;

    #[test]
    fn test_serialize_and_deserialize_balance_proof() {
        let balance_vd = CircuitVerifiers::load().get_balance_vd();
        let proof = cyclic_base_proof(
            &balance_vd.common,
            &balance_vd.verifier_only,
            vec![].into_iter().collect(),
        );
        let compressed = CompressedBalanceProof::new(&proof).unwrap();
        let _decompress = compressed.decompress().unwrap();
        let _json_str = serde_json::to_string(&compressed).unwrap();
    }
}
