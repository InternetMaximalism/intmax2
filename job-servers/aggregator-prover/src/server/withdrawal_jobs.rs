use crate::app::{config, interface::WithdrawalProofContent, state::AppState};
use anyhow::Context;
use intmax2_interfaces::utils::circuit_verifiers::CircuitVerifiers;
use intmax2_zkp::{
    common::withdrawal::Withdrawal, ethereum_types::address::Address, utils::conversion::ToU64,
};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};
use redis::{ExistenceCheck, SetExpiry, SetOptions};

const D: usize = 2;
type C = PoseidonGoldilocksConfig;
type F = GoldilocksField;

pub async fn generate_withdrawal_proof_job(
    state: &AppState,
    request_id: String,
    prev_withdrawal_proof: Option<ProofWithPublicInputs<F, C, D>>,
    single_withdrawal_proof: &ProofWithPublicInputs<F, C, D>,
    conn: &mut redis::aio::Connection,
) -> anyhow::Result<()> {
    log::info!("generate_withdrawal_proof_job");
    let single_withdrawal_vd = CircuitVerifiers::load().get_single_withdrawal_vd();
    single_withdrawal_vd
        .verify(single_withdrawal_proof.clone())
        .map_err(|e| anyhow::anyhow!("Invalid single withdrawal proof: {:?}", e))?;

    log::info!("Proving withdrawal chain");
    let withdrawal_proof = state
        .withdrawal_processor
        .prove_chain(single_withdrawal_proof, &prev_withdrawal_proof)
        .map_err(|e| anyhow::anyhow!("Failed to prove withdrawal chain: {}", e))?;

    log::info!("Serializing withdrawal proof");
    let withdrawal_proof = bincode::serialize(&withdrawal_proof)
        .with_context(|| "Failed to serialize withdrawal proof")?;

    let opts = SetOptions::default()
        .conditional_set(ExistenceCheck::NX)
        .get(true)
        .with_expiration(SetExpiry::EX(config::get("proof_expiration")));
    let withdrawal =
        Withdrawal::from_u64_slice(&single_withdrawal_proof.public_inputs.to_u64_vec())?;
    let proof_content = WithdrawalProofContent {
        proof: withdrawal_proof,
        withdrawal,
    };

    let proof_content_json = serde_json::to_string(&proof_content)
        .with_context(|| "Failed to encode withdrawal proof")?;
    let _ = redis::Cmd::set_options(&request_id, proof_content_json, opts)
        .query_async::<_, Option<String>>(conn)
        .await
        .with_context(|| "Failed to set proof")?;

    Ok(())
}

pub async fn generate_withdrawal_wrapper_proof_job(
    state: &AppState,
    request_id: String,
    withdrawal_proof: ProofWithPublicInputs<F, C, D>,
    withdrawal_aggregator: Address,
    conn: &mut redis::aio::Connection,
) -> anyhow::Result<()> {
    log::info!("generate_withdrawal_wrapper_proof_job");
    let wrapped_withdrawal_proof = state
        .withdrawal_processor
        .prove_end(&withdrawal_proof, withdrawal_aggregator)
        .with_context(|| "Failed to prove withdrawal")?;

    let inner_wrap_proof = state
        .withdrawal_inner_wrap_circuit
        .prove(&wrapped_withdrawal_proof)
        .with_context(|| "Failed to prove withdrawal wrapper")?;
    let outer_wrap_proof = state
        .withdrawal_outer_wrap_circuit
        .prove(&inner_wrap_proof)
        .with_context(|| "Failed to prove withdrawal wrapper")?;

    // NOTICE: Not compressing the proof here
    let withdrawal_proof_json = serde_json::to_string(&outer_wrap_proof)
        .with_context(|| "Failed to encode outer withdrawal proof")?;

    let opts = SetOptions::default()
        .conditional_set(ExistenceCheck::NX)
        .get(true)
        .with_expiration(SetExpiry::EX(config::get("proof_expiration")));

    let _ = redis::Cmd::set_options(&request_id, withdrawal_proof_json.clone(), opts)
        .query_async::<_, Option<String>>(conn)
        .await
        .with_context(|| "Failed to set proof")?;

    Ok(())
}
