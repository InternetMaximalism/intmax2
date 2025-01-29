use crate::{
    app::{
        encode::decode_plonky2_proof,
        interface::{
            GenerateProofResponse, ProofResponse, ProofValue, ProofsResponse,
            WithdrawalWrapperIdQuery, WithdrawalWrapperProofRequest,
        },
        state::AppState,
    },
    proof::generate_withdrawal_wrapper_proof_job,
};
use actix_web::{error, get, post, web, HttpRequest, HttpResponse, Responder, Result};
use intmax2_zkp::ethereum_types::{address::Address, u32limb_trait::U32LimbTrait};

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
        let response = ProofResponse {
            success: false,
            proof: None,
            error_message: None,
        };

        return Ok(HttpResponse::Ok().json(response));
    }

    let response = ProofResponse {
        success: true,
        proof,
        error_message: None,
    };

    Ok(HttpResponse::Ok().json(response))
}

#[get("/proofs/wrapper")]
async fn get_proofs(req: HttpRequest, redis: web::Data<redis::Client>) -> Result<impl Responder> {
    let mut conn = redis
        .get_async_connection()
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let query_string = req.query_string();
    let ids_query: WithdrawalWrapperIdQuery = serde_qs::from_str(query_string)
        .map_err(|e| error::ErrorBadRequest(format!("Failed to deserialize query: {e:?}")))?;
    let request_ids: Vec<String> = ids_query.ids;

    let mut proofs: Vec<ProofValue> = Vec::new();
    for id in &request_ids {
        let request_id = get_withdrawal_wrapper_request_id(id);
        let some_proof = redis::Cmd::get(&request_id)
            .query_async::<_, Option<String>>(&mut conn)
            .await
            .map_err(error::ErrorInternalServerError)?;
        if let Some(proof) = some_proof {
            proofs.push(ProofValue {
                id: (*id).to_string(),
                proof,
            });
        }
    }

    let response = ProofsResponse {
        success: true,
        proofs,
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
        let response = ProofResponse {
            success: true,
            proof: None,
            error_message: Some("withdrawal wrapper proof already exists".to_string()),
        };

        return Ok(HttpResponse::Ok().json(response));
    }

    let withdrawal_circuit_data = state
        .withdrawal_processor
        .withdrawal_circuit
        .data
        .verifier_data();

    log::debug!("requested proof size: {}", req.withdrawal_proof.len());
    let withdrawal_proof = decode_plonky2_proof(&req.withdrawal_proof, &withdrawal_circuit_data)
        .map_err(error::ErrorBadRequest)?;
    withdrawal_circuit_data
        .verify(withdrawal_proof.clone())
        .map_err(error::ErrorBadRequest)?;

    let withdrawal_aggregator =
        Address::from_hex(&req.withdrawal_aggregator).map_err(error::ErrorBadRequest)?;

    // TODO: Validation check of withdrawal_witness

    // Spawn a new task to generate the proof
    actix_web::rt::spawn(async move {
        let response = generate_withdrawal_wrapper_proof_job(
            request_id,
            withdrawal_proof,
            withdrawal_aggregator,
            &state.withdrawal_processor,
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
