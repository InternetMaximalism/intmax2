use actix_web::{
    get, post,
    web::{Data, Json},
    Error,
};
use intmax2_interfaces::api::block_builder::{
    interface::BlockBuilderFeeInfo,
    types::{
        PostSignatureRequest, QueryProposalRequest, QueryProposalResponse, TxRequestRequest,
        TxRequestResponse,
    },
};
use intmax2_zkp::common::block_builder::UserSignature;

use crate::api::state::State;

#[get("/fee-info")]
pub async fn get_fee_info(state: Data<State>) -> Result<Json<BlockBuilderFeeInfo>, Error> {
    state
        .block_builder
        .blockchain_health_check()
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let fee_info = state.block_builder.get_fee_info();
    Ok(Json(fee_info))
}

#[post("/tx-request")]
pub async fn tx_request(
    state: Data<State>,
    request: Json<TxRequestRequest>,
) -> Result<Json<TxRequestResponse>, Error> {
    let request = request.into_inner();
    let request_id = state
        .block_builder
        .send_tx_request(
            request.is_registration_block,
            request.pubkey,
            request.tx,
            &request.fee_proof,
        )
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(TxRequestResponse { request_id }))
}

#[post("/query-proposal")]
pub async fn query_proposal(
    state: Data<State>,
    request: Json<QueryProposalRequest>,
) -> Result<Json<QueryProposalResponse>, Error> {
    let request = request.into_inner();
    let block_proposal = state
        .block_builder
        .query_proposal(&request.request_id)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(QueryProposalResponse { block_proposal }))
}

#[post("/post-signature")]
pub async fn post_signature(
    state: Data<State>,
    request: Json<PostSignatureRequest>,
) -> Result<Json<()>, Error> {
    let request = request.into_inner();
    let user_signature = UserSignature {
        pubkey: request.pubkey,
        signature: request.signature,
    };
    state
        .block_builder
        .post_signature(&request.request_id, user_signature)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(()))
}

pub fn block_builder_scope() -> actix_web::Scope {
    actix_web::web::scope("/block-builder")
        .service(get_fee_info)
        .service(tx_request)
        .service(query_proposal)
        .service(post_signature)
}
