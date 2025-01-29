use base64::prelude::*;
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{
        circuit_data::{CommonCircuitData, VerifierCircuitData},
        config::PoseidonGoldilocksConfig,
        proof::{CompressedProofWithPublicInputs, ProofWithPublicInputs},
    },
    util::serialization::{Buffer, IoResult, Read, Write},
};

const D: usize = 2;
type C = PoseidonGoldilocksConfig;
type F = GoldilocksField;

pub(crate) fn encode_plonky2_proof(
    proof: ProofWithPublicInputs<F, C, D>,
    circuit_data: &VerifierCircuitData<F, C, D>,
) -> anyhow::Result<String> {
    let compressed_proof = proof
        .compress(
            &circuit_data.verifier_only.circuit_digest,
            &circuit_data.common,
        )
        .map_err(|e| anyhow::anyhow!("Failed to compress proof: {}", e))?;

    let proof_bytes = compressed_proof_to_bytes(&compressed_proof)
        .map_err(|e| anyhow::anyhow!("Failed to serialize proof: {}", e))?;

    Ok(BASE64_STANDARD.encode(&proof_bytes))
}

pub(crate) fn decode_plonky2_proof(
    encoded_proof: &str,
    circuit_data: &VerifierCircuitData<F, C, D>,
) -> anyhow::Result<ProofWithPublicInputs<F, C, D>> {
    let decoded_proof = BASE64_STANDARD.decode(encoded_proof)?;
    let compressed_proof = compressed_proof_from_bytes(decoded_proof, &circuit_data.common)
        .map_err(|e| anyhow::anyhow!(e))?;

    compressed_proof.decompress(
        &circuit_data.verifier_only.circuit_digest,
        &circuit_data.common,
    )
}

pub(crate) fn compressed_proof_to_bytes(
    compressed_proof_with_pis: &CompressedProofWithPublicInputs<F, C, D>,
) -> IoResult<Vec<u8>> {
    let mut buffer = Vec::new();

    let CompressedProofWithPublicInputs {
        proof,
        public_inputs,
    } = compressed_proof_with_pis;

    buffer.write_u32(public_inputs.len() as u32)?;
    buffer.write_field_vec(public_inputs)?;
    buffer.write_compressed_proof(proof)?;

    Ok(buffer)
}

pub(crate) fn compressed_proof_from_bytes(
    bytes: Vec<u8>,
    common_data: &CommonCircuitData<F, D>,
) -> IoResult<CompressedProofWithPublicInputs<F, C, D>> {
    let mut buffer = Buffer::new(&bytes);

    let public_inputs_len = buffer.read_u32()?;
    let public_inputs = buffer.read_field_vec(public_inputs_len as usize)?;
    let proof = buffer.read_compressed_proof(common_data)?;

    Ok(CompressedProofWithPublicInputs {
        proof,
        public_inputs,
    })
}
