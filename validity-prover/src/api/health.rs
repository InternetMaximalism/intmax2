use std::collections::HashMap;

use crate::{
    api::state::State,
    app::{
        check_point_store::EventType,
        observer_common::sync_event_key,
        validity_prover::{
            ADD_TASKS_KEY, CLEANUP_INACTIVE_TASKS_KEY, GENERATE_VALIDITY_PROOF_KEY,
            SYNC_VALIDITY_WITNESS_KEY,
        },
    },
};
use actix_web::{
    get,
    web::{Data, Json},
    Error,
};
use serde::Serialize;

#[derive(Serialize)]
pub struct HealthCheckResponse {
    pub name: String,
    pub version: String,
}

#[get("/health-check")]
pub async fn health_check(state: Data<State>) -> Result<Json<HealthCheckResponse>, Error> {
    let heartbeat_timeout = state.health_check_config.thread_heartbeat_timeout.as_secs();

    let mut keys = [
        EventType::Deposited,
        EventType::DepositLeafInserted,
        EventType::BlockPosted,
    ]
    .iter()
    .map(|event_type| sync_event_key(*event_type))
    .collect::<Vec<_>>();
    keys.extend([
        SYNC_VALIDITY_WITNESS_KEY.to_string(),
        GENERATE_VALIDITY_PROOF_KEY.to_string(),
        ADD_TASKS_KEY.to_string(),
        CLEANUP_INACTIVE_TASKS_KEY.to_string(),
    ]);
    let mut last_heartbeats = HashMap::new();

    let current_timestamp = chrono::Utc::now().timestamp() as u64;
    for key in keys.iter() {
        let last_timestamp = state.rate_manager.last_timestamp(key).await.map_err(|_| {
            actix_web::error::ErrorInternalServerError("Failed to get last timestamp")
        })?;
        if let Some(last_timestamp) = last_timestamp {
            if last_timestamp + heartbeat_timeout < current_timestamp {
                return Err(actix_web::error::ErrorInternalServerError(format!(
                    "Heartbeat for {} is too old, last heartbeat: {}, current timestamp: {}",
                    key, last_timestamp, current_timestamp
                )));
            }
        };
        let delta =
            last_timestamp.map(|last_timestamp| current_timestamp.saturating_sub(last_timestamp));
        last_heartbeats.insert(key.clone(), delta);
    }
    tracing::info!(
        "Heartbeat check passed. Last heartbeats: {:?}",
        last_heartbeats
    );
    Ok(Json(HealthCheckResponse {
        name: env!("CARGO_PKG_NAME").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }))
}
