use crate::{
    app::{
        interface::{
            GenerateProofResponse, WithdrawalWrapperProofRequest, WithdrawalWrapperProofResponse,
        },
        state::AppState,
    },
    server::jobs::generate_withdrawal_wrapper_proof_job,
};
use actix_web::{error, get, post, web, HttpResponse, Responder, Result};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};

type C = PoseidonGoldilocksConfig;
const D: usize = 2;
type F = GoldilocksField;

#[get("/proof/wrapper/{id}")]
async fn get_proof(
    id: web::Path<String>,
    redis: web::Data<redis::Client>,
) -> Result<impl Responder> {
    let mut conn = redis
        .get_async_connection()
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let request_id = get_withdrawal_wrapper_request_id(&id);
    let proof = redis::Cmd::get(&request_id)
        .query_async::<_, Option<String>>(&mut conn)
        .await
        .map_err(error::ErrorInternalServerError)?;
    if proof.is_none() {
        let response = WithdrawalWrapperProofResponse {
            success: false,
            proof: None,
            error_message: None,
        };

        return Ok(HttpResponse::Ok().json(response));
    }

    let response = WithdrawalWrapperProofResponse {
        success: true,
        proof,
        error_message: None,
    };

    Ok(HttpResponse::Ok().json(response))
}

#[post("/proof/wrapper")]
async fn generate_proof(
    req: web::Json<WithdrawalWrapperProofRequest>,
    redis: web::Data<redis::Client>,
    state: web::Data<AppState>,
) -> Result<impl Responder> {
    let mut redis_conn = redis
        .get_async_connection()
        .await
        .map_err(error::ErrorInternalServerError)?;

    let request_id = get_withdrawal_wrapper_request_id(&req.id);
    let old_proof = redis::Cmd::get(&request_id)
        .query_async::<_, Option<String>>(&mut redis_conn)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    if old_proof.is_some() {
        let response = WithdrawalWrapperProofResponse {
            success: true,
            proof: None,
            error_message: Some("withdrawal wrapper proof already exists".to_string()),
        };

        return Ok(HttpResponse::Ok().json(response));
    }

    let withdrawal_circuit_data = state
        .withdrawal_processor
        .cyclic_circuit
        .data
        .verifier_data();

    let withdrawal_proof: ProofWithPublicInputs<F, C, D> =
        bincode::deserialize(&req.withdrawal_proof).map_err(error::ErrorBadRequest)?;
    withdrawal_circuit_data
        .verify(withdrawal_proof.clone())
        .map_err(error::ErrorBadRequest)?;

    // Spawn a new task to generate the proof
    actix_web::rt::spawn(async move {
        let response = generate_withdrawal_wrapper_proof_job(
            &state,
            request_id,
            withdrawal_proof,
            req.withdrawal_aggregator,
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
        message: "withdrawal wrapper proof is generating".to_string(),
    }))
}

fn get_withdrawal_wrapper_request_id(id: &str) -> String {
    format!("withdrawal-wrapper/{}", id)
}
