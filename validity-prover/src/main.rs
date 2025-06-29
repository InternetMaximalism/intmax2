use std::io::{self};

use actix_cors::Cors;
use actix_web::{web::Data, App, HttpServer};
use server_common::{logger, version_check::VersionCheck};
use tracing_actix_web::TracingLogger;
use validity_prover::{
    api::{health::health_check, state::State, validity_prover::validity_prover_scope},
    EnvVar,
};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    logger::init_logger().map_err(io::Error::other)?;

    dotenvy::dotenv().ok();
    let env: EnvVar = envy::from_env()
        .map_err(|e| io::Error::other(format!("Failed to parse environment variables: {e}")))?;
    let state = State::new(&env)
        .await
        .map_err(|e| io::Error::other(format!("Failed to create validity prover: {e}")))?;

    let data = Data::new(state);

    HttpServer::new(move || {
        let cors = Cors::permissive();
        App::new()
            .wrap(cors)
            .wrap(TracingLogger::<logger::CustomRootSpanBuilder>::new())
            .wrap(VersionCheck::from_env())
            .app_data(data.clone())
            .service(health_check)
            .service(validity_prover_scope())
    })
    .bind(format!("0.0.0.0:{}", env.port))?
    .run()
    .await
}
