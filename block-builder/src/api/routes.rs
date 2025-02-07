use actix_web::{
    get, post,
    web::{Data, Json},
    Error,
};
use intmax2_interfaces::api::block_builder::types::{
    GetBlockBuilderStatusQuery, GetBlockBuilderStatusResponse, PostSignatureRequest,
    QueryProposalRequest, QueryProposalResponse, TxRequestRequest,
};
use intmax2_zkp::common::block_builder::UserSignature;
use serde_qs::actix::QsQuery;

use crate::api::state::State;

#[get("/status")]
pub async fn get_status(
    state: Data<State>,
    query: QsQuery<GetBlockBuilderStatusQuery>,
) -> Result<Json<GetBlockBuilderStatusResponse>, Error> {
    let status = state
        .block_builder
        .get_status(query.is_registration_block)
        .await;
    Ok(Json(GetBlockBuilderStatusResponse { status }))
}

#[post("/tx-request")]
pub async fn tx_request(
    state: Data<State>,
    request: Json<TxRequestRequest>,
) -> Result<Json<()>, Error> {
    let request = request.into_inner();
    state
        .block_builder
        .send_tx_request(
            request.is_registration_block,
            request.pubkey,
            request.tx,
            &request.fee_proof,
        )
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(()))
}

#[post("/query-proposal")]
pub async fn query_proposal(
    state: Data<State>,
    request: Json<QueryProposalRequest>,
) -> Result<Json<QueryProposalResponse>, Error> {
    let request = request.into_inner();
    let block_proposal = state
        .block_builder
        .query_proposal(request.is_registration_block, request.pubkey, request.tx)
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
        .post_signature(request.is_registration_block, request.tx, user_signature)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(()))
}

pub fn block_builder_scope() -> actix_web::Scope {
    actix_web::web::scope("/block-builder")
        .service(get_status)
        .service(tx_request)
        .service(query_proposal)
        .service(post_signature)
}
