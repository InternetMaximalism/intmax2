use crate::{
    app::{
        encode::decode_plonky2_proof,
        interface::{
            GenerateProofResponse, ProofContent, WithdrawalIdQuery, WithdrawalProofRequest,
            WithdrawalProofResponse, WithdrawalProofValue, WithdrawalProofsResponse,
        },
        state::AppState,
    },
    server::job::generate_withdrawal_proof_job,
};
use actix_web::{error, get, post, web, HttpRequest, HttpResponse, Responder, Result};

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
    let proof_with_withdrawal = redis::Cmd::get(&request_id)
        .query_async::<_, Option<String>>(&mut conn)
        .await
        .map_err(error::ErrorInternalServerError)?;

    if let Some(proof_content_json) = proof_with_withdrawal {
        let proof_content: ProofContent =
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

#[get("/proofs/withdrawal")]
async fn get_proofs(req: HttpRequest, redis: web::Data<redis::Client>) -> Result<impl Responder> {
    let mut conn = redis
        .get_async_connection()
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let query_string = req.query_string();
    let ids_query: WithdrawalIdQuery = serde_qs::from_str(query_string)
        .map_err(|e| error::ErrorBadRequest(format!("Failed to deserialize query: {e:?}")))?;
    let request_ids: Vec<String> = ids_query.ids;

    let mut proofs: Vec<WithdrawalProofValue> = Vec::new();
    for id in &request_ids {
        let request_id = get_withdrawal_request_id(id);
        let some_proof = redis::Cmd::get(&request_id)
            .query_async::<_, Option<String>>(&mut conn)
            .await
            .map_err(error::ErrorInternalServerError)?;
        if let Some(proof_content_json) = some_proof {
            let proof_content: ProofContent = serde_json::from_str(&proof_content_json)
                .map_err(error::ErrorInternalServerError)?;
            proofs.push(WithdrawalProofValue {
                id: (*id).to_string(),
                proof: proof_content,
            });
        }
    }

    let response = WithdrawalProofsResponse {
        success: true,
        proofs,
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
        let proof_content: ProofContent =
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
        .withdrawal_circuit
        .data
        .verifier_data();

    let prev_withdrawal_proof = if let Some(req_prev_withdrawal_proof) = &req.prev_withdrawal_proof
    {
        log::debug!("requested proof size: {}", req_prev_withdrawal_proof.len());
        if req_prev_withdrawal_proof.is_empty() {
            None
        } else {
            let prev_withdrawal_proof =
                decode_plonky2_proof(req_prev_withdrawal_proof, &withdrawal_circuit_data)
                    .map_err(error::ErrorBadRequest)?;
            println!("start withdrawal_circuit_data");
            withdrawal_circuit_data
                .verify(prev_withdrawal_proof.clone())
                .map_err(error::ErrorBadRequest)?;
            println!("end withdrawal_circuit_data");

            Some(prev_withdrawal_proof)
        }
    } else {
        None
    };

    let single_withdrawal_circuit = &state.withdrawal_processor.single_withdrawal_circuit;
    println!("start withdrawal_witness");
    let single_withdrawal_proof = decode_plonky2_proof(
        &req.single_withdrawal_proof,
        &single_withdrawal_circuit.data.verifier_data(),
    )
    .map_err(error::ErrorBadRequest)?;
    println!("end withdrawal_witness");

    // TODO: Validation check of withdrawal_witness

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
