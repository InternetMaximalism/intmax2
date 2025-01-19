use crate::api::state::State;
use actix_web::{
    error::ErrorUnauthorized,
    post,
    web::{Bytes, BytesMut, Data, Json, Payload},
    Error, HttpResponse,
};
use futures_util::{stream, Stream, StreamExt as _};
use intmax2_interfaces::{
    api::store_vault_server::types::{
        GetDataBatchRequest, GetDataBatchResponse, GetDataSequenceRequest, GetDataSequenceResponse,
        GetSenderProofSetRequest, GetSenderProofSetResponse, GetUserDataRequest,
        GetUserDataResponse, SaveDataBatchRequest, SaveDataBatchResponse,
        SaveSenderProofSetRequest, SaveUserDataRequest,
    },
    utils::signature::{Signable, WithAuth},
};

#[post("/save-user-data")]
pub async fn save_user_data(
    state: Data<State>,
    request: Json<WithAuth<SaveUserDataRequest>>,
) -> Result<Json<()>, Error> {
    request
        .inner
        .verify(&request.auth)
        .map_err(ErrorUnauthorized)?;
    let pubkey = request.auth.pubkey;
    let request = &request.inner;
    state
        .store_vault_server
        .save_user_data(pubkey, request.prev_digest, &request.data)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(()))
}

#[post("/save-user-data-stream")]
pub async fn save_user_data_stream(
    state: Data<State>,
    mut payload: Payload,
) -> Result<Json<()>, Error> {
    let mut bytes = BytesMut::new();
    while let Some(chunk) = payload.next().await {
        let chunk = chunk?;
        bytes.extend_from_slice(&chunk);
    }
    let request: WithAuth<SaveUserDataRequest> = bincode::deserialize(&bytes).map_err(|e| {
        actix_web::error::ErrorBadRequest(format!("Failed to deserialize request: {}", e))
    })?;
    request
        .inner
        .verify(&request.auth)
        .map_err(ErrorUnauthorized)?;
    let pubkey = request.auth.pubkey;
    let request = &request.inner;
    state
        .store_vault_server
        .save_user_data(pubkey, request.prev_digest, &request.data)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(()))
}

#[post("/get-user-data")]
pub async fn get_user_data(
    state: Data<State>,
    request: Json<WithAuth<GetUserDataRequest>>,
) -> Result<Json<GetUserDataResponse>, Error> {
    request
        .inner
        .verify(&request.auth)
        .map_err(ErrorUnauthorized)?;
    let pubkey = request.auth.pubkey;
    let data = state
        .store_vault_server
        .get_user_data(pubkey)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetUserDataResponse { data }))
}

#[post("/get-user-data-stream")]
pub async fn get_user_data_stream(
    state: Data<State>,
    request: Json<WithAuth<GetUserDataRequest>>,
) -> Result<HttpResponse, Error> {
    request
        .inner
        .verify(&request.auth)
        .map_err(ErrorUnauthorized)?;
    let pubkey = request.auth.pubkey;
    let data = state
        .store_vault_server
        .get_user_data(pubkey)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let response_bytes = bincode::serialize(&GetUserDataResponse { data }).unwrap();
    let http_response = HttpResponse::Ok()
        .append_header(("Content-Type", "application/octet-stream"))
        .streaming(create_stream(response_bytes));
    Ok(http_response)
}

fn create_stream(data: Vec<u8>) -> impl Stream<Item = anyhow::Result<Bytes>> {
    const CHUNK_SIZE: usize = 1 << 20; // 1MB
    let chunks: Vec<_> = data
        .chunks(CHUNK_SIZE)
        .map(|chunk| chunk.to_vec())
        .collect();
    stream::iter(chunks).map(|chunk| Ok(Bytes::from(chunk)))
}

#[post("/save-sender-proof-set")]
pub async fn save_sender_proof_set(
    state: Data<State>,
    request: Json<WithAuth<SaveSenderProofSetRequest>>,
) -> Result<Json<()>, Error> {
    request
        .inner
        .verify(&request.auth)
        .map_err(ErrorUnauthorized)?;
    let data = &request.inner.data;
    let pubkey = request.auth.pubkey;
    state
        .store_vault_server
        .save_sender_proof_set(pubkey, data)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(()))
}

#[post("/get-sender-proof-set")]
pub async fn get_sender_proof_set(
    state: Data<State>,
    request: Json<WithAuth<GetSenderProofSetRequest>>,
) -> Result<Json<GetSenderProofSetResponse>, Error> {
    request
        .inner
        .verify(&request.auth)
        .map_err(ErrorUnauthorized)?;
    let pubkey = request.auth.pubkey;
    let data = state
        .store_vault_server
        .get_sender_proof_set(pubkey)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(GetSenderProofSetResponse { data }))
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
    let pubkey = request.auth.pubkey;
    let entries = &request.inner.data;

    const MAX_BATCH_SIZE: usize = 1000;
    if entries.len() > MAX_BATCH_SIZE {
        return Err(actix_web::error::ErrorBadRequest(format!(
            "Batch size exceeds maximum limit of {}",
            MAX_BATCH_SIZE
        )));
    }

    for entry in entries {
        if entry.data_type.need_auth() && entry.pubkey != pubkey {
            return Err(ErrorUnauthorized(format!(
                "Data type {} requires auth but given pubkey is different",
                entry.data_type,
            )));
        }
    }
    let uuids = state
        .store_vault_server
        .batch_save_data(entries)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(SaveDataBatchResponse { uuids }))
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
    let data = state
        .store_vault_server
        .get_data_batch(request.data_type, pubkey, &request.uuids)
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
    let (data, cursor_response) = state
        .store_vault_server
        .get_data_sequence(request.data_type, pubkey, &request.cursor)
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
        .service(save_user_data)
        .service(get_user_data)
        .service(save_user_data_stream)
        .service(get_user_data_stream)
        .service(save_sender_proof_set)
        .service(get_sender_proof_set)
        .service(save_data_batch)
        .service(get_data_batch)
        .service(get_data_sequence)
}
