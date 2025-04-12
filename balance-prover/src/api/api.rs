use actix_web::{
    post,
    web::{scope, Data, Json},
    Error, Scope,
};
use intmax2_interfaces::api::balance_prover::types::{
    ProveReceiveDepositRequest, ProveReceiveTransferRequest, ProveResponse, ProveSendRequest,
    ProveSingleClaimRequest, ProveSingleWithdrawalRequest, ProveSpentRequest, ProveUpdateRequest,
};

use crate::api::balance_prover::BalanceProver;

#[post("/prove-spent")]
pub async fn prove_spent(
    state: Data<BalanceProver>,
    request: Json<ProveSpentRequest>,
) -> Result<Json<ProveResponse>, Error> {
    let proof = state
        .prove_spent(&request.spent_witness)
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(ProveResponse { proof }))
}

#[post("/prove-send")]
pub async fn prove_send(
    state: Data<BalanceProver>,
    request: Json<ProveSendRequest>,
) -> Result<Json<ProveResponse>, Error> {
    let proof = state
        .prove_send(
            request.pubkey,
            &request.tx_witness,
            &request.update_witness,
            &request.spent_proof,
            &request.prev_proof,
        )
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(ProveResponse { proof }))
}

#[post("/prove-update")]
pub async fn prove_update(
    state: Data<BalanceProver>,
    request: Json<ProveUpdateRequest>,
) -> Result<Json<ProveResponse>, Error> {
    let proof = state
        .prove_update(request.pubkey, &request.update_witness, &request.prev_proof)
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(ProveResponse { proof }))
}

#[post("/prove-receive-transfer")]
pub async fn prove_receive_transfer(
    state: Data<BalanceProver>,
    request: Json<ProveReceiveTransferRequest>,
) -> Result<Json<ProveResponse>, Error> {
    let request = request.into_inner();
    let proof = state
        .prove_receive_transfer(
            request.pubkey,
            &request.receive_transfer_witness,
            &request.prev_proof,
        )
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(ProveResponse { proof }))
}

#[post("/prove-receive-deposit")]
pub async fn prove_receive_deposit(
    state: Data<BalanceProver>,
    request: Json<ProveReceiveDepositRequest>,
) -> Result<Json<ProveResponse>, Error> {
    let request = request.into_inner();
    let proof = state
        .prove_receive_deposit(
            request.pubkey,
            &request.receive_deposit_witness,
            &request.prev_proof,
        )
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(ProveResponse { proof }))
}

#[post("/prove-single-withdrawal")]
pub async fn prove_single_withdrawal(
    state: Data<BalanceProver>,
    request: Json<ProveSingleWithdrawalRequest>,
) -> Result<Json<ProveResponse>, Error> {
    let request = request.into_inner();
    let proof = state
        .prove_single_withdrawal(&request.withdrawal_witness)
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(ProveResponse { proof }))
}

#[post("/prove-single-claim")]
pub async fn prove_single_claim(
    state: Data<BalanceProver>,
    request: Json<ProveSingleClaimRequest>,
) -> Result<Json<ProveResponse>, Error> {
    let request = request.into_inner();
    let proof = state
        .prove_single_claim(request.is_faster_mining, &request.claim_witness)
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(ProveResponse { proof }))
}

pub fn balance_prover_scope() -> Scope {
    scope("/balance-prover")
        .service(prove_spent)
        .service(prove_send)
        .service(prove_update)
        .service(prove_receive_transfer)
        .service(prove_receive_deposit)
        .service(prove_single_withdrawal)
        .service(prove_single_claim)
}
