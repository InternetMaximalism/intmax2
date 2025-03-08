use crate::api::state::State;
use actix_web::{
    get, post,
    web::{Data, Json},
    Error,
};
use intmax2_interfaces::api::validity_prover::{
    interface::MAX_BATCH_SIZE,
    types::{
        GetAccountInfoBatchRequest, GetAccountInfoBatchResponse, GetAccountInfoQuery,
        GetAccountInfoResponse, GetBlockMerkleProofQuery, GetBlockMerkleProofResponse,
        GetBlockNumberByTxTreeRootBatchRequest, GetBlockNumberByTxTreeRootBatchResponse,
        GetBlockNumberByTxTreeRootQuery, GetBlockNumberByTxTreeRootResponse,
        GetBlockNumberResponse, GetDepositInfoBatchRequest, GetDepositInfoBatchResponse,
        GetDepositInfoQuery, GetDepositInfoResponse, GetDepositMerkleProofQuery,
        GetDepositMerkleProofResponse, GetLatestIncludedDepositIndexResponse,
        GetNextDepositIndexResponse, GetUpdateWitnessQuery, GetUpdateWitnessResponse,
        GetValidityWitnessQuery, GetValidityWitnessResponse,
    },
};
use intmax2_zkp::circuits::validity::validity_pis::ValidityPublicInputs;
use serde_qs::actix::QsQuery;

#[get("/block-number")]
pub async fn get_block_number(state: Data<State>) -> Result<Json<GetBlockNumberResponse>, Error> {
    let block_number = state
        .get_block_number()
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetBlockNumberResponse { block_number }))
}

#[get("/validity-proof-block-number")]
pub async fn get_validity_proof_block_number(
    state: Data<State>,
) -> Result<Json<GetBlockNumberResponse>, Error> {
    let block_number = state
        .get_validity_proof_block_number()
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetBlockNumberResponse { block_number }))
}

#[get("/next-deposit-index")]
pub async fn get_next_deposit_index(
    state: Data<State>,
) -> Result<Json<GetNextDepositIndexResponse>, Error> {
    let deposit_index = state
        .get_next_deposit_index()
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetNextDepositIndexResponse { deposit_index }))
}

#[get("/latest-included-deposit-index")]
pub async fn get_latest_included_deposit_index(
    state: Data<State>,
) -> Result<Json<GetLatestIncludedDepositIndexResponse>, Error> {
    let deposit_index = state
        .get_latest_included_deposit_index()
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetLatestIncludedDepositIndexResponse {
        deposit_index,
    }))
}

#[get("/get-account-info")]
pub async fn get_account_info(
    state: Data<State>,
    query: QsQuery<GetAccountInfoQuery>,
) -> Result<Json<GetAccountInfoResponse>, Error> {
    let query = query.into_inner();
    let response = state
        .get_account_info(query)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(response))
}

#[post("/get-account-info-batch")]
pub async fn get_account_info_batch(
    state: Data<State>,
    request: Json<GetAccountInfoBatchRequest>,
) -> Result<Json<GetAccountInfoBatchResponse>, Error> {
    let request = request.into_inner();
    if request.pubkeys.len() > MAX_BATCH_SIZE {
        return Err(actix_web::error::ErrorBadRequest("Batch size is too large"));
    }
    let response = state
        .get_account_info_batch(&request)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(response))
}

#[get("/get-update-witness")]
pub async fn get_update_witness(
    state: Data<State>,
    query: QsQuery<GetUpdateWitnessQuery>,
) -> Result<Json<GetUpdateWitnessResponse>, Error> {
    let query = query.into_inner();
    let response = state
        .get_update_witness(query)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(response))
}

#[get("/get-validity-witness")]
pub async fn get_validity_witness(
    state: Data<State>,
    query: QsQuery<GetValidityWitnessQuery>,
) -> Result<Json<GetValidityWitnessResponse>, Error> {
    let query = query.into_inner();
    let response = state
        .get_validity_witness(query)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(response))
}

#[get("/get-validity-pis")]
pub async fn get_validity_pis(
    state: Data<State>,
    query: QsQuery<GetValidityWitnessQuery>,
) -> Result<Json<ValidityPublicInputs>, Error> {
    let query = query.into_inner();
    let response = state
        .get_validity_witness(query)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(response.validity_witness.to_validity_pis().unwrap()))
}

#[get("/get-deposit-info")]
pub async fn get_deposit_info(
    state: Data<State>,
    query: QsQuery<GetDepositInfoQuery>,
) -> Result<Json<GetDepositInfoResponse>, Error> {
    let query = query.into_inner();
    let response = state
        .get_deposit_info(query)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(response))
}

#[post("/get-deposit-info-batch")]
pub async fn get_deposit_info_batch(
    state: Data<State>,
    request: Json<GetDepositInfoBatchRequest>,
) -> Result<Json<GetDepositInfoBatchResponse>, Error> {
    let request = request.into_inner();
    if request.deposit_hashes.len() > MAX_BATCH_SIZE {
        return Err(actix_web::error::ErrorBadRequest("Batch size is too large"));
    }
    let response = state
        .get_deposit_info_batch(&request)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(response))
}

#[get("/get-block-number-by-tx-tree-root")]
pub async fn get_block_number_by_tx_tree_root(
    state: Data<State>,
    query: QsQuery<GetBlockNumberByTxTreeRootQuery>,
) -> Result<Json<GetBlockNumberByTxTreeRootResponse>, Error> {
    let query = query.into_inner();
    let response = state
        .get_block_number_by_tx_tree_root(query)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(response))
}

#[post("/get-block-number-by-tx-tree-root-batch")]
pub async fn get_block_number_by_tx_tree_root_batch(
    state: Data<State>,
    request: Json<GetBlockNumberByTxTreeRootBatchRequest>,
) -> Result<Json<GetBlockNumberByTxTreeRootBatchResponse>, Error> {
    let request = request.into_inner();
    if request.tx_tree_roots.len() > MAX_BATCH_SIZE {
        return Err(actix_web::error::ErrorBadRequest("Batch size is too large"));
    }
    let request = state
        .get_block_number_by_tx_tree_root_batch(&request)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(request))
}

#[get("/get-block-merkle-proof")]
pub async fn get_block_merkle_proof(
    state: Data<State>,
    query: QsQuery<GetBlockMerkleProofQuery>,
) -> Result<Json<GetBlockMerkleProofResponse>, Error> {
    let query = query.into_inner();
    let response = state
        .get_block_merkle_proof(query)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(response))
}

#[get("/get-deposit-merkle-proof")]
pub async fn get_deposit_merkle_proof(
    state: Data<State>,
    query: QsQuery<GetDepositMerkleProofQuery>,
) -> Result<Json<GetDepositMerkleProofResponse>, Error> {
    let query = query.into_inner();
    let response = state
        .get_deposit_merkle_proof(&query)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(response))
}

pub fn validity_prover_scope() -> actix_web::Scope {
    actix_web::web::scope("/validity-prover")
        .service(get_block_number)
        .service(get_validity_proof_block_number)
        .service(get_next_deposit_index)
        .service(get_latest_included_deposit_index)
        .service(get_account_info)
        .service(get_account_info_batch)
        .service(get_update_witness)
        .service(get_validity_witness)
        .service(get_validity_pis)
        .service(get_deposit_info)
        .service(get_deposit_info_batch)
        .service(get_block_number_by_tx_tree_root)
        .service(get_block_number_by_tx_tree_root_batch)
        .service(get_block_merkle_proof)
        .service(get_deposit_merkle_proof)
}
