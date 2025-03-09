use crate::api::state::State;
use actix_web::{
    error::ErrorUnauthorized,
    post,
    web::{Data, Json},
    Error,
};
use intmax2_interfaces::{
    api::store_vault_server::{
        interface::MAX_BATCH_SIZE,
        types::{
            GetDataBatchRequest, GetDataBatchResponse, GetDataSequenceRequest,
            GetDataSequenceResponse, GetSnapshotRequest, GetSnapshotResponse, SaveDataBatchRequest,
            SaveDataBatchResponse, SaveSnapshotRequest,
        },
    },
    utils::signature::{Signable, WithAuth},
};

#[post("/save-snapshot")]
pub async fn save_snapshot(
    state: Data<State>,
    request: Json<WithAuth<SaveSnapshotRequest>>,
) -> Result<Json<()>, Error> {
    request
        .inner
        .verify(&request.auth)
        .map_err(ErrorUnauthorized)?;
    let pubkey = request.auth.pubkey;
    let request = &request.inner;
    state
        .store_vault_server
        .save_snapshot(&request.topic, pubkey, request.prev_digest, &request.data)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(()))
}

#[post("/get-snapshot")]
pub async fn get_snapshot(
    state: Data<State>,
    request: Json<WithAuth<GetSnapshotRequest>>,
) -> Result<Json<GetSnapshotResponse>, Error> {
    request
        .inner
        .verify(&request.auth)
        .map_err(ErrorUnauthorized)?;
    let pubkey = request.auth.pubkey;
    let request = &request.inner;
    let data = state
        .store_vault_server
        .get_snapshot_data(&request.topic, pubkey)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetSnapshotResponse { data }))
}

#[post("/save-data-batch")]
pub async fn save_data_batch(
    state: Data<State>,
    request: Json<WithAuth<SaveDataBatchRequest>>,
) -> Result<Json<SaveDataBatchResponse>, Error> {
    request
        .inner
        .verify(&request.auth)
        .map_err(ErrorUnauthorized)?;
    // let pubkey = request.auth.pubkey;
    let entries = &request.inner.data;

    if entries.len() > MAX_BATCH_SIZE {
        return Err(actix_web::error::ErrorBadRequest(format!(
            "Batch size exceeds maximum limit of {}",
            MAX_BATCH_SIZE
        )));
    }

    // todo: auth check
    // for entry in entries {
    //     if entry.data_type.need_auth() && entry.pubkey != pubkey {
    //         return Err(ErrorUnauthorized(format!(
    //             "Data type {} requires auth but given pubkey is different",
    //             entry.data_type,
    //         )));
    //     }
    // }

    let digests = state
        .store_vault_server
        .batch_save_data(entries)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(SaveDataBatchResponse { digests }))
}

#[post("/get-data-batch")]
pub async fn get_data_batch(
    state: Data<State>,
    request: Json<WithAuth<GetDataBatchRequest>>,
) -> Result<Json<GetDataBatchResponse>, Error> {
    request
        .inner
        .verify(&request.auth)
        .map_err(ErrorUnauthorized)?;
    let pubkey = request.auth.pubkey;
    let request = &request.inner;

    if request.digests.len() > MAX_BATCH_SIZE {
        return Err(actix_web::error::ErrorBadRequest(format!(
            "Batch size exceeds maximum limit of {}",
            MAX_BATCH_SIZE
        )));
    }

    let data = state
        .store_vault_server
        .get_data_batch(&request.topic, pubkey, &request.digests)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetDataBatchResponse { data }))
}

#[post("/get-data-sequence")]
pub async fn get_data_sequence(
    state: Data<State>,
    request: Json<WithAuth<GetDataSequenceRequest>>,
) -> Result<Json<GetDataSequenceResponse>, Error> {
    request
        .inner
        .verify(&request.auth)
        .map_err(ErrorUnauthorized)?;
    let pubkey = request.auth.pubkey;
    let request = &request.inner;

    if let Some(limit) = request.cursor.limit {
        if limit > MAX_BATCH_SIZE as u32 {
            return Err(actix_web::error::ErrorBadRequest(format!(
                "Batch size exceeds maximum limit of {}",
                MAX_BATCH_SIZE
            )));
        }
    }

    let (data, cursor_response) = state
        .store_vault_server
        .get_data_sequence(&request.topic, pubkey, &request.cursor)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let res = GetDataSequenceResponse {
        data,
        cursor_response,
    };
    Ok(Json(res))
}

pub fn store_vault_server_scope() -> actix_web::Scope {
    actix_web::web::scope("/store-vault-server")
        .service(save_snapshot)
        .service(get_snapshot)
        .service(save_data_batch)
        .service(get_data_batch)
        .service(get_data_sequence)
}
