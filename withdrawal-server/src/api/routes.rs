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
    let (withdrawal_info, cursor_response) = state
        .withdrawal_server
        .get_withdrawal_info(pubkey, request.inner.cursor.clone())
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetWithdrawalInfoResponse {
        withdrawal_info,
        cursor_response,
    }))
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
    let (claim_info, cursor_response) = state
        .withdrawal_server
        .get_claim_info(pubkey, request.inner.cursor.clone())
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetClaimInfoResponse {
        claim_info,
        cursor_response,
    }))
}

#[get("/get-withdrawal-info-by-recipient")]
pub async fn get_withdrawal_info_by_recipient(
    state: Data<State>,
    query: QsQuery<GetWithdrawalInfoByRecipientQuery>,
) -> Result<Json<GetWithdrawalInfoResponse>, Error> {
    let (withdrawal_info, cursor_response) = state
        .withdrawal_server
        .get_withdrawal_info_by_recipient(query.recipient, query.cursor.clone())
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetWithdrawalInfoResponse {
        withdrawal_info,
        cursor_response,
    }))
}

pub fn withdrawal_server_scope() -> Scope {
    actix_web::web::scope("")
        .service(get_withdrawal_fee)
        .service(get_claim_fee)
        .service(request_withdrawal)
        .service(request_claim)
        .service(get_withdrawal_info)
        .service(get_withdrawal_info_by_recipient)
        .service(get_claim_info)
}

#[cfg(test)]
mod tests {
    use crate::{
        api::{
            routes::{get_claim_fee, get_withdrawal_fee},
            state::State,
        },
        app::test_helpers::{start_mock_withdrawal_server, stop_withdrawal_docker},
        Env,
    };
    use actix_web::{test, web, App};
    use dotenvy::dotenv;
    use intmax2_client_sdk::client::config::network_from_env;
    use intmax2_interfaces::{
        api::withdrawal_server::interface::{ClaimFeeInfo, WithdrawalFeeInfo},
        utils::address::IntmaxAddress,
    };
    use std::sync::Arc;

    #[tokio::test]
    async fn test_withdrawal_server_actix_web() {
        let cont_name = "test-withdrawal-server-actix-web";
        let server = start_mock_withdrawal_server(cont_name)
            .await
            .expect("WithdrawalServer creation has failed");

        // Get env for further checks
        dotenv().ok();
        let env: Env = envy::from_env().expect("Failed to parse env");

        let state = State {
            withdrawal_server: Arc::new(server),
        };

        // Test get("/withdrawal-fee")
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state.clone()))
                .service(get_withdrawal_fee),
        )
        .await;
        let req = test::TestRequest::get().uri("/withdrawal-fee").to_request();
        let resp: WithdrawalFeeInfo = test::call_and_read_body_json(&app, req).await;
        assert_eq!(
            resp.beneficiary,
            IntmaxAddress::from_viewpair(network_from_env(), &env.withdrawal_beneficiary_view_pair)
        );
        assert_eq!(
            resp.direct_withdrawal_fee,
            env.direct_withdrawal_fee.clone().map(|l| l.0)
        );
        assert_eq!(
            resp.claimable_withdrawal_fee,
            env.claimable_withdrawal_fee.clone().map(|l| l.0)
        );

        // Test get("/claim-fee")
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state.clone()))
                .service(get_claim_fee),
        )
        .await;
        let req = test::TestRequest::get().uri("/claim-fee").to_request();
        let resp: ClaimFeeInfo = test::call_and_read_body_json(&app, req).await;
        assert_eq!(
            resp.beneficiary,
            IntmaxAddress::from_viewpair(network_from_env(), &env.claim_beneficiary_view_pair)
        );
        assert_eq!(resp.fee, env.claim_fee.clone().map(|l| l.0));

        stop_withdrawal_docker(cont_name);
    }
}
