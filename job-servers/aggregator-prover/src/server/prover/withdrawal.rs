use crate::{
    app::{
        interface::{
            GenerateProofResponse, WithdrawalProofContent, WithdrawalProofRequest,
            WithdrawalProofResponse,
        },
        state::AppState,
    },
    server::jobs::generate_withdrawal_proof_job,
};
use actix_web::{error, get, post, web, HttpResponse, Responder, Result};
use intmax2_interfaces::data::proof_compression::CompressedSingleWithdrawalProof;
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};

type C = PoseidonGoldilocksConfig;
const D: usize = 2;
type F = GoldilocksField;

#[get("/proof/withdrawal/{id}")]
async fn get_proof(
    id: web::Path<String>,
    redis: web::Data<redis::Client>,
) -> Result<impl Responder> {
    let mut conn = redis
        .get_async_connection()
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let request_id = get_withdrawal_request_id(&id);
    let proof_content_json = redis::Cmd::get(&request_id)
        .query_async::<_, Option<String>>(&mut conn)
        .await
        .map_err(error::ErrorInternalServerError)?;

    if let Some(proof_content_json) = proof_content_json {
        let proof_content: WithdrawalProofContent =
            serde_json::from_str(&proof_content_json).map_err(error::ErrorInternalServerError)?;
        let response = WithdrawalProofResponse {
            success: true,
            proof: Some(proof_content),
            error_message: None,
        };
        return Ok(HttpResponse::Ok().json(response));
    }

    let response = WithdrawalProofResponse {
        success: true,
        proof: None,
        error_message: None,
    };

    Ok(HttpResponse::Ok().json(response))
}

#[post("/proof/withdrawal")]
async fn generate_proof(
    req: web::Json<WithdrawalProofRequest>,
    redis: web::Data<redis::Client>,
    state: web::Data<AppState>,
) -> Result<impl Responder> {
    let mut redis_conn = redis
        .get_async_connection()
        .await
        .map_err(error::ErrorInternalServerError)?;

    let request_id = get_withdrawal_request_id(&req.id);
    let old_proof = redis::Cmd::get(&request_id)
        .query_async::<_, Option<String>>(&mut redis_conn)
        .await
        .map_err(error::ErrorInternalServerError)?;
    if let Some(proof_content_json) = old_proof {
        let proof_content: WithdrawalProofContent =
            serde_json::from_str(&proof_content_json).map_err(error::ErrorInternalServerError)?;
        let response = WithdrawalProofResponse {
            success: true,
            proof: Some(proof_content),
            error_message: Some("withdrawal proof already exists".to_string()),
        };

        return Ok(HttpResponse::Ok().json(response));
    }

    let withdrawal_circuit_data = state
        .withdrawal_processor
        .cyclic_circuit
        .data
        .verifier_data();

    let prev_withdrawal_proof = if let Some(req_prev_withdrawal_proof) = &req.prev_withdrawal_proof
    {
        if req_prev_withdrawal_proof.is_empty() {
            None
        } else {
            let prev_withdrawal_proof: ProofWithPublicInputs<F, C, D> =
                bincode::deserialize::<_>(req_prev_withdrawal_proof)
                    .map_err(error::ErrorBadRequest)?;
            withdrawal_circuit_data
                .verify(prev_withdrawal_proof.clone())
                .map_err(error::ErrorBadRequest)?;
            Some(prev_withdrawal_proof)
        }
    } else {
        None
    };
    let single_withdrawal_proof =
        CompressedSingleWithdrawalProof(req.single_withdrawal_proof.to_vec())
            .decompress()
            .map_err(error::ErrorBadRequest)?;

    // Spawn a new task to generate the proof
    actix_web::rt::spawn(async move {
        let response = generate_withdrawal_proof_job(
            &state,
            request_id,
            prev_withdrawal_proof,
            &single_withdrawal_proof,
            &mut redis_conn,
        )
        .await;

        match response {
            Ok(v) => {
                log::info!("Proof generation completed");
                Ok(v)
            }
            Err(e) => {
                log::error!("Failed to generate proof: {:?}", e);
                Err(e)
            }
        }
    });

    Ok(HttpResponse::Ok().json(GenerateProofResponse {
        success: true,
        message: "withdrawal proof is generating".to_string(),
    }))
}

fn get_withdrawal_request_id(id: &str) -> String {
    format!("withdrawal/{}", id)
}
