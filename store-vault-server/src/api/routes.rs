use crate::api::state::State;
use actix_web::{
    error::ErrorUnauthorized,
    post,
    web::{Data, Json},
    Error,
};
use intmax2_interfaces::{
    api::{
        s3_store_vault::types::{
            S3GetDataBatchRequest, S3GetDataBatchResponse, S3GetDataSequenceRequest,
            S3GetDataSequenceResponse, S3GetSnapshotRequest, S3GetSnapshotResponse,
            S3PreSaveSnapshotRequest, S3PreSaveSnapshotResponse, S3SaveDataBatchRequest,
            S3SaveDataBatchResponse, S3SaveSnapshotRequest,
        },
        store_vault_server::interface::MAX_BATCH_SIZE,
    },
    data::{rw_rights, topic::extract_rights},
    utils::signature::{Signable, WithAuth},
};

#[post("/pre-save-snapshot")]
pub async fn pre_save_snapshot(
    state: Data<State>,
    request: Json<WithAuth<S3PreSaveSnapshotRequest>>,
) -> Result<Json<S3PreSaveSnapshotResponse>, Error> {
    request
        .inner
        .verify(&request.auth)
        .map_err(ErrorUnauthorized)?;
    let auth_pubkey = request.auth.pubkey;
    let request = &request.inner;

    // validate rights
    validate_topic_length(&request.topic)?;
    let rw_rights = extract_rights(&request.topic)
        .map_err(|e| actix_web::error::ErrorBadRequest(format!("Invalid topic: {e}")))?;
    match rw_rights.write_rights {
        rw_rights::WriteRights::SingleAuthWrite => {
            if auth_pubkey != request.pubkey {
                return Err(actix_web::error::ErrorBadRequest(
                    "Auth pubkey does not match request pubkey",
                ));
            }
        }
        rw_rights::WriteRights::SingleOpenWrite => {}
        rw_rights::WriteRights::AuthWrite => {
            if auth_pubkey != request.pubkey {
                return Err(actix_web::error::ErrorBadRequest(
                    "Auth pubkey does not match request pubkey",
                ));
            }
        }
        rw_rights::WriteRights::OpenWrite => {}
    }

    let presigned_url = state
        .s3_store_vault
        .pre_save_snapshot(&request.topic, request.pubkey, request.digest)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(Json(S3PreSaveSnapshotResponse { presigned_url }))
}

#[post("/save-snapshot")]
pub async fn save_snapshot(
    state: Data<State>,
    request: Json<WithAuth<S3SaveSnapshotRequest>>,
) -> Result<Json<()>, Error> {
    request
        .inner
        .verify(&request.auth)
        .map_err(ErrorUnauthorized)?;
    let auth_pubkey = request.auth.pubkey;
    let request = &request.inner;

    // validate rights
    validate_topic_length(&request.topic)?;
    let rw_rights = extract_rights(&request.topic)
        .map_err(|e| actix_web::error::ErrorBadRequest(format!("Invalid topic: {e}")))?;
    match rw_rights.write_rights {
        rw_rights::WriteRights::SingleAuthWrite => {
            if auth_pubkey != request.pubkey {
                return Err(actix_web::error::ErrorBadRequest(
                    "Auth pubkey does not match request pubkey",
                ));
            }
            if request.prev_digest.is_some() {
                return Err(actix_web::error::ErrorBadRequest(
                    "pre_digest is not allowed in SingleAuthWrite",
                ));
            }
        }
        rw_rights::WriteRights::SingleOpenWrite => {
            if request.prev_digest.is_some() {
                return Err(actix_web::error::ErrorBadRequest(
                    "pre_digest is not allowed in SingleOpenWrite",
                ));
            }
        }
        rw_rights::WriteRights::AuthWrite => {
            if auth_pubkey != request.pubkey {
                return Err(actix_web::error::ErrorBadRequest(
                    "Auth pubkey does not match request pubkey",
                ));
            }
        }
        rw_rights::WriteRights::OpenWrite => {}
    }

    state
        .s3_store_vault
        .save_snapshot(
            &request.topic,
            request.pubkey,
            request.prev_digest,
            request.digest,
        )
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(Json(()))
}

#[post("/get-snapshot")]
pub async fn get_snapshot(
    state: Data<State>,
    request: Json<WithAuth<S3GetSnapshotRequest>>,
) -> Result<Json<S3GetSnapshotResponse>, Error> {
    request
        .inner
        .verify(&request.auth)
        .map_err(ErrorUnauthorized)?;
    let auth_pubkey = request.auth.pubkey;
    let request = &request.inner;

    // validate rights
    validate_topic_length(&request.topic)?;
    let rw_rights = extract_rights(&request.topic)
        .map_err(|e| actix_web::error::ErrorBadRequest(format!("Invalid topic: {e}")))?;
    match rw_rights.read_rights {
        rw_rights::ReadRights::AuthRead => {
            if auth_pubkey != request.pubkey {
                return Err(actix_web::error::ErrorBadRequest(
                    "Auth pubkey does not match request pubkey",
                ));
            }
        }
        rw_rights::ReadRights::OpenRead => {}
    }

    let presigned_url = state
        .s3_store_vault
        .get_snapshot_url(&request.topic, request.pubkey)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(Json(S3GetSnapshotResponse { presigned_url }))
}

#[post("/save-data-batch")]
pub async fn save_data_batch(
    state: Data<State>,
    request: Json<WithAuth<S3SaveDataBatchRequest>>,
) -> Result<Json<S3SaveDataBatchResponse>, Error> {
    request
        .inner
        .verify(&request.auth)
        .map_err(ErrorUnauthorized)?;
    let auth_pubkey = request.auth.pubkey;
    let entries = &request.inner.data;

    if entries.len() > MAX_BATCH_SIZE {
        return Err(actix_web::error::ErrorBadRequest(format!(
            "Batch size exceeds maximum limit of {MAX_BATCH_SIZE}"
        )));
    }

    for entry in entries {
        validate_topic_length(&entry.topic)?;
        let rw_rights = extract_rights(&entry.topic)
            .map_err(|e| actix_web::error::ErrorBadRequest(format!("Invalid topic: {e}")))?;
        match rw_rights.write_rights {
            rw_rights::WriteRights::SingleAuthWrite => {
                return Err(actix_web::error::ErrorBadRequest(
                    "SingleAuthWrite is not allowed in historical data",
                ));
            }
            rw_rights::WriteRights::SingleOpenWrite => {
                return Err(actix_web::error::ErrorBadRequest(
                    "SingleOpenWrite is not allowed in historical data",
                ));
            }
            rw_rights::WriteRights::AuthWrite => {
                if auth_pubkey != entry.pubkey {
                    return Err(actix_web::error::ErrorBadRequest(
                        "Auth pubkey does not match request pubkey",
                    ));
                }
            }
            rw_rights::WriteRights::OpenWrite => {}
        }
    }

    let presigned_urls = state
        .s3_store_vault
        .batch_save_data_url(entries)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(Json(S3SaveDataBatchResponse { presigned_urls }))
}

#[post("/get-data-batch")]
pub async fn get_data_batch(
    state: Data<State>,
    request: Json<WithAuth<S3GetDataBatchRequest>>,
) -> Result<Json<S3GetDataBatchResponse>, Error> {
    request
        .inner
        .verify(&request.auth)
        .map_err(ErrorUnauthorized)?;
    let auth_pubkey = request.auth.pubkey;
    let request = &request.inner;

    if request.digests.len() > MAX_BATCH_SIZE {
        return Err(actix_web::error::ErrorBadRequest(format!(
            "Batch size exceeds maximum limit of {MAX_BATCH_SIZE}"
        )));
    }

    // validate rights
    validate_topic_length(&request.topic)?;
    let rw_rights = extract_rights(&request.topic)
        .map_err(|e| actix_web::error::ErrorBadRequest(format!("Invalid topic: {e}")))?;
    match rw_rights.read_rights {
        rw_rights::ReadRights::AuthRead => {
            if auth_pubkey != request.pubkey {
                return Err(actix_web::error::ErrorBadRequest(
                    "Auth pubkey does not match request pubkey",
                ));
            }
        }
        rw_rights::ReadRights::OpenRead => {}
    }

    let presigned_urls_with_meta = state
        .s3_store_vault
        .get_data_batch(&request.topic, auth_pubkey, &request.digests)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(Json(S3GetDataBatchResponse {
        presigned_urls_with_meta,
    }))
}

#[post("/get-data-sequence")]
pub async fn get_data_sequence(
    state: Data<State>,
    request: Json<WithAuth<S3GetDataSequenceRequest>>,
) -> Result<Json<S3GetDataSequenceResponse>, Error> {
    request
        .inner
        .verify(&request.auth)
        .map_err(ErrorUnauthorized)?;
    let pubkey = request.auth.pubkey;
    let request = &request.inner;

    if let Some(limit) = request.cursor.limit {
        if limit > MAX_BATCH_SIZE as u32 {
            return Err(actix_web::error::ErrorBadRequest(format!(
                "Batch size exceeds maximum limit of {MAX_BATCH_SIZE}"
            )));
        }
    }
    // validate rights
    validate_topic_length(&request.topic)?;
    let rw_rights = extract_rights(&request.topic)
        .map_err(|e| actix_web::error::ErrorBadRequest(format!("Invalid topic: {e}")))?;
    match rw_rights.read_rights {
        rw_rights::ReadRights::AuthRead => {
            if pubkey != request.pubkey {
                return Err(actix_web::error::ErrorBadRequest(
                    "Auth pubkey does not match request pubkey",
                ));
            }
        }
        rw_rights::ReadRights::OpenRead => {}
    }

    let (presigned_urls_with_meta, cursor_response) = state
        .s3_store_vault
        .get_data_sequence_url(&request.topic, pubkey, &request.cursor)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(Json(S3GetDataSequenceResponse {
        presigned_urls_with_meta,
        cursor_response,
    }))
}

pub fn s3_store_vault_scope() -> actix_web::Scope {
    actix_web::web::scope("/s3-store-vault")
        .service(pre_save_snapshot)
        .service(save_snapshot)
        .service(get_snapshot)
        .service(save_data_batch)
        .service(get_data_batch)
        .service(get_data_sequence)
}

fn validate_topic_length(topic: &str) -> Result<(), actix_web::Error> {
    if topic.len() >= 256 {
        return Err(actix_web::error::ErrorBadRequest("Topic too long"));
    }
    Ok(())
}
