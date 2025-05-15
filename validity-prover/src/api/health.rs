use actix_web::{
    get,
    web::{Data, Json},
    Error,
};
use serde::Serialize;

use crate::{
    api::state::State,
    app::{check_point_store::EventType, observer_common::sync_event_success_key},
};

#[derive(Serialize)]
pub struct HealthCheckResponse {
    pub name: String,
    pub version: String,
}

#[get("/health-check")]
pub async fn health_check(state: Data<State>) -> Result<Json<HealthCheckResponse>, Error> {
    let keys = [
        EventType::Deposited,
        EventType::DepositLeafInserted,
        EventType::BlockPosted,
    ]
    .iter()
    .map(|event_type| sync_event_success_key(*event_type))
    .collect::<Vec<_>>();

    let mut last_timestamps = vec![];
    for key in keys {
        let last_timestamp = state.rate_manager.last_timestamp(&key).await.map_err(|_| {
            actix_web::error::ErrorInternalServerError("Failed to get last timestamp")
        })?;
        last_timestamps.push(last_timestamp);
    }

    todo!()
}
