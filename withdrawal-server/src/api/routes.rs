use crate::api::state::State;
use actix_web::{
    error::ErrorUnauthorized,
    get, post,
    web::{Data, Json},
    Error, Scope,
};
use intmax2_interfaces::{
    api::withdrawal_server::{
        interface::{ClaimFeeInfo, WithdrawalFeeInfo},
        types::{
            GetClaimInfoRequest, GetClaimInfoResponse, GetWithdrawalInfoByRecipientQuery,
            GetWithdrawalInfoRequest, GetWithdrawalInfoResponse, RequestClaimRequest,
            RequestClaimResponse, RequestWithdrawalRequest, RequestWithdrawalResponse,
        },
    },
    utils::signature::{Signable as _, WithAuth},
};
use serde_qs::actix::QsQuery;

#[get("/withdrawal-fee")]
pub async fn get_withdrawal_fee(state: Data<State>) -> Result<Json<WithdrawalFeeInfo>, Error> {
    let fees = state.withdrawal_server.get_withdrawal_fee();
    Ok(Json(fees))
}

#[get("/claim-fee")]
pub async fn get_claim_fee(state: Data<State>) -> Result<Json<ClaimFeeInfo>, Error> {
    let fees = state.withdrawal_server.get_claim_fee();
    Ok(Json(fees))
}

#[post("/request-withdrawal")]
pub async fn request_withdrawal(
    state: Data<State>,
    request: Json<WithAuth<RequestWithdrawalRequest>>,
) -> Result<Json<RequestWithdrawalResponse>, Error> {
    request
        .inner
        .verify(&request.auth)
        .map_err(ErrorUnauthorized)?;
    let pubkey = request.auth.pubkey;
    let fee_result = state
        .withdrawal_server
        .request_withdrawal(
            pubkey,
            &request.inner.single_withdrawal_proof,
            request.inner.fee_token_index,
            &request.inner.fee_transfer_digests,
        )
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(RequestWithdrawalResponse { fee_result }))
}

#[post("/request-claim")]
pub async fn request_claim(
    state: Data<State>,
    request: Json<WithAuth<RequestClaimRequest>>,
) -> Result<Json<RequestClaimResponse>, Error> {
    request
        .inner
        .verify(&request.auth)
        .map_err(ErrorUnauthorized)?;
    let pubkey = request.auth.pubkey;
    let fee_result = state
        .withdrawal_server
        .request_claim(
            pubkey,
            &request.inner.single_claim_proof,
            request.inner.fee_token_index,
            &request.inner.fee_transfer_digests,
        )
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(RequestClaimResponse { fee_result }))
}

#[post("/get-withdrawal-info")]
pub async fn get_withdrawal_info(
    state: Data<State>,
    request: Json<WithAuth<GetWithdrawalInfoRequest>>,
) -> Result<Json<GetWithdrawalInfoResponse>, Error> {
    request
        .inner
        .verify(&request.auth)
        .map_err(ErrorUnauthorized)?;
    let pubkey = request.auth.pubkey;
    let withdrawal_info = state
        .withdrawal_server
        .get_withdrawal_info(pubkey)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetWithdrawalInfoResponse { withdrawal_info }))
}

#[post("/get-claim-info")]
pub async fn get_claim_info(
    state: Data<State>,
    request: Json<WithAuth<GetClaimInfoRequest>>,
) -> Result<Json<GetClaimInfoResponse>, Error> {
    request
        .inner
        .verify(&request.auth)
        .map_err(ErrorUnauthorized)?;
    let pubkey = request.auth.pubkey;
    let claim_info = state
        .withdrawal_server
        .get_claim_info(pubkey)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetClaimInfoResponse { claim_info }))
}

#[get("/get-withdrawal-info-by-recipient")]
pub async fn get_withdrawal_info_by_recipient(
    state: Data<State>,
    query: QsQuery<GetWithdrawalInfoByRecipientQuery>,
) -> Result<Json<GetWithdrawalInfoResponse>, Error> {
    let withdrawal_info = state
        .withdrawal_server
        .get_withdrawal_info_by_recipient(query.recipient)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetWithdrawalInfoResponse { withdrawal_info }))
}

pub fn withdrawal_server_scope() -> Scope {
    actix_web::web::scope("/withdrawal-server")
        .service(get_withdrawal_fee)
        .service(get_claim_fee)
        .service(request_withdrawal)
        .service(request_claim)
        .service(get_withdrawal_info)
        .service(get_withdrawal_info_by_recipient)
        .service(get_claim_info)
}
