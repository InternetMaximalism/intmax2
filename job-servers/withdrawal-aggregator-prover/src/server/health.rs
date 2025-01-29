use crate::app::interface::HealthCheckResponse;
use actix_web::{error, get, web, HttpResponse, Responder, Result};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

async fn is_redis_healthy(redis: &redis::Client) -> bool {
    match redis.get_async_connection().await {
        Ok(mut conn) => {
            let pong: redis::RedisResult<String> = redis::cmd("PING").query_async(&mut conn).await;
            matches!(pong, Ok(response) if response == "PONG")
        }
        _ => false,
    }
}

fn create_health_response(start_time: SystemTime) -> HealthCheckResponse {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis();

    let uptime = SystemTime::now()
        .duration_since(start_time)
        .unwrap_or(Duration::from_secs(0))
        .as_secs_f64();

    HealthCheckResponse {
        message: "OK".to_string(),
        timestamp,
        uptime,
    }
}

#[get("/health")]
async fn health_check(redis: web::Data<redis::Client>) -> Result<impl Responder> {
    let start_time = SystemTime::now();
    if !is_redis_healthy(&redis).await {
        return Err(error::ErrorInternalServerError(
            "Redis connection check failed: Unable to establish connection or receive response"
                .to_string(),
        ));
    }
    Ok(HttpResponse::Ok().json(create_health_response(start_time)))
}
