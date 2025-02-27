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
        GetDepositMerkleProofResponse, GetNextDepositIndexResponse, GetUpdateWitnessQuery,
        GetUpdateWitnessResponse, GetValidityWitnessQuery, GetValidityWitnessResponse,
    },
};
use intmax2_zkp::circuits::validity::validity_pis::ValidityPublicInputs;
use serde_qs::actix::QsQuery;

#[get("/block-number")]
pub async fn get_block_number(state: Data<State>) -> Result<Json<GetBlockNumberResponse>, Error> {
    let block_number = state
        .validity_prover
        .get_last_block_number()
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetBlockNumberResponse { block_number }))
}

#[get("/validity-proof-block-number")]
pub async fn get_validity_proof_block_number(
    state: Data<State>,
) -> Result<Json<GetBlockNumberResponse>, Error> {
    let block_number = state
        .validity_prover
        .get_latest_validity_proof_block_number()
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetBlockNumberResponse { block_number }))
}

#[get("/next-deposit-index")]
pub async fn get_next_deposit_index(
    state: Data<State>,
) -> Result<Json<GetNextDepositIndexResponse>, Error> {
    let deposit_index = state
        .validity_prover
        .get_next_deposit_index()
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetNextDepositIndexResponse { deposit_index }))
}

#[get("/get-account-info")]
pub async fn get_account_info(
    state: Data<State>,
    query: QsQuery<GetAccountInfoQuery>,
) -> Result<Json<GetAccountInfoResponse>, Error> {
    let query = query.into_inner();
    let account_info = state
        .validity_prover
        .get_account_info(query.pubkey)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetAccountInfoResponse { account_info }))
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
    let account_info = state
        .validity_prover
        .get_account_info_batch(&request.pubkeys)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetAccountInfoBatchResponse { account_info }))
}

#[get("/get-update-witness")]
pub async fn get_update_witness(
    state: Data<State>,
    query: QsQuery<GetUpdateWitnessQuery>,
) -> Result<Json<GetUpdateWitnessResponse>, Error> {
    let query = query.into_inner();
    let update_witness = state
        .validity_prover
        .get_update_witness(
            query.pubkey,
            query.root_block_number,
            query.leaf_block_number,
            query.is_prev_account_tree,
        )
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetUpdateWitnessResponse { update_witness }))
}

#[get("/get-validity-witness")]
pub async fn get_validity_witness(
    state: Data<State>,
    query: QsQuery<GetValidityWitnessQuery>,
) -> Result<Json<GetValidityWitnessResponse>, Error> {
    let query = query.into_inner();
    let validity_witness = state
        .validity_prover
        .get_validity_witness(query.block_number)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetValidityWitnessResponse { validity_witness }))
}

#[get("/get-validity-pis")]
pub async fn get_validity_pis(
    state: Data<State>,
    query: QsQuery<GetValidityWitnessQuery>,
) -> Result<Json<ValidityPublicInputs>, Error> {
    let query = query.into_inner();
    let validity_witness = state
        .validity_prover
        .get_validity_witness(query.block_number)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(validity_witness.to_validity_pis().unwrap()))
}

#[get("/get-deposit-info")]
pub async fn get_deposit_info(
    state: Data<State>,
    query: QsQuery<GetDepositInfoQuery>,
) -> Result<Json<GetDepositInfoResponse>, Error> {
    let query = query.into_inner();
    let deposit_info = state
        .validity_prover
        .get_deposit_info(query.deposit_hash)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetDepositInfoResponse { deposit_info }))
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
    let deposit_info = state
        .validity_prover
        .get_deposit_info_batch(&request.deposit_hashes)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetDepositInfoBatchResponse { deposit_info }))
}

#[get("/get-block-number-by-tx-tree-root")]
pub async fn get_block_number_by_tx_tree_root(
    state: Data<State>,
    query: QsQuery<GetBlockNumberByTxTreeRootQuery>,
) -> Result<Json<GetBlockNumberByTxTreeRootResponse>, Error> {
    let query = query.into_inner();
    let block_number = state
        .validity_prover
        .get_block_number_by_tx_tree_root(query.tx_tree_root)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetBlockNumberByTxTreeRootResponse { block_number }))
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
    let block_numbers = state
        .validity_prover
        .get_block_number_by_tx_tree_root_batch(&request.tx_tree_roots)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetBlockNumberByTxTreeRootBatchResponse {
        block_numbers,
    }))
}

#[get("/get-block-merkle-proof")]
pub async fn get_block_merkle_proof(
    state: Data<State>,
    query: QsQuery<GetBlockMerkleProofQuery>,
) -> Result<Json<GetBlockMerkleProofResponse>, Error> {
    let query = query.into_inner();
    let block_merkle_proof = state
        .validity_prover
        .get_block_merkle_proof(query.root_block_number, query.leaf_block_number)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetBlockMerkleProofResponse { block_merkle_proof }))
}

#[get("/get-deposit-merkle-proof")]
pub async fn get_deposit_merkle_proof(
    state: Data<State>,
    query: QsQuery<GetDepositMerkleProofQuery>,
) -> Result<Json<GetDepositMerkleProofResponse>, Error> {
    let query = query.into_inner();
    let deposit_merkle_proof = state
        .validity_prover
        .get_deposit_merkle_proof(query.block_number, query.deposit_index)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetDepositMerkleProofResponse {
        deposit_merkle_proof,
    }))
}

pub fn validity_prover_scope() -> actix_web::Scope {
    actix_web::web::scope("/validity-prover")
        .service(get_block_number)
        .service(get_validity_proof_block_number)
        .service(get_next_deposit_index)
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
