use crate::app::{config, interface::ClaimProofContent, state::AppState};
use anyhow::Context;
use intmax2_zkp::{
    common::claim::Claim, ethereum_types::address::Address, utils::conversion::ToU64,
};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};
use redis::{ExistenceCheck, SetExpiry, SetOptions};

const D: usize = 2;
type C = PoseidonGoldilocksConfig;
type F = GoldilocksField;

pub async fn generate_claim_proof_job(
    state: &AppState,
    request_id: String,
    prev_claim_proof: Option<ProofWithPublicInputs<F, C, D>>,
    single_claim_proof: &ProofWithPublicInputs<F, C, D>,
    conn: &mut redis::aio::Connection,
) -> anyhow::Result<()> {
    log::info!("generate_claim_proof_job");
    state
        .single_claim_vd
        .verify(single_claim_proof.clone())
        .map_err(|e| anyhow::anyhow!("Invalid single claim proof: {:?}", e))?;

    log::info!("Proving claim chain");
    let claim_proof = state
        .claim_processor
        .prove_chain(single_claim_proof, &prev_claim_proof)
        .map_err(|e| anyhow::anyhow!("Failed to prove claim chain: {}", e))?;

    log::info!("Serializing claim proof");
    let claim_proof =
        bincode::serialize(&claim_proof).with_context(|| "Failed to serialize claim proof")?;

    let opts = SetOptions::default()
        .conditional_set(ExistenceCheck::NX)
        .get(true)
        .with_expiration(SetExpiry::EX(config::get("proof_expiration")));
    let claim = Claim::from_u64_slice(&single_claim_proof.public_inputs.to_u64_vec());
    let proof_content = ClaimProofContent {
        proof: claim_proof,
        claim,
    };

    let proof_content_json =
        serde_json::to_string(&proof_content).with_context(|| "Failed to encode claim proof")?;
    let _ = redis::Cmd::set_options(&request_id, proof_content_json, opts)
        .query_async::<_, Option<String>>(conn)
        .await
        .with_context(|| "Failed to set proof")?;

    Ok(())
}

pub async fn generate_claim_wrapper_proof_job(
    state: &AppState,
    request_id: String,
    claim_proof: ProofWithPublicInputs<F, C, D>,
    claim_aggregator: Address,
    conn: &mut redis::aio::Connection,
) -> anyhow::Result<()> {
    log::info!("generate_claim_wrapper_proof_job");
    let wrapped_claim_proof = state
        .claim_processor
        .prove_end(&claim_proof, claim_aggregator)
        .with_context(|| "Failed to prove claim")?;

    let inner_wrap_proof = state
        .claim_inner_wrap_circuit
        .prove(&wrapped_claim_proof)
        .with_context(|| "Failed to prove claim wrapper")?;
    let outer_wrap_proof = state
        .claim_outer_wrap_circuit
        .prove(&inner_wrap_proof)
        .with_context(|| "Failed to prove claim wrapper")?;

    // NOTICE: Not compressing the proof here
    let claim_proof_json = serde_json::to_string(&outer_wrap_proof)
        .with_context(|| "Failed to encode outer claim proof")?;

    let opts = SetOptions::default()
        .conditional_set(ExistenceCheck::NX)
        .get(true)
        .with_expiration(SetExpiry::EX(config::get("proof_expiration")));

    let _ = redis::Cmd::set_options(&request_id, claim_proof_json.clone(), opts)
        .query_async::<_, Option<String>>(conn)
        .await
        .with_context(|| "Failed to set proof")?;

    Ok(())
}
