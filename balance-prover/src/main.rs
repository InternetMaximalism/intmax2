use actix_cors::Cors;
use actix_web::{web::Data, App, HttpServer};
use balance_prover::{
    api::{api::balance_prover_scope, balance_prover::BalanceProver},
    Env,
};
use server_common::{
    health_check::{health_check, set_name_and_version},
    logger,
};
use std::io::{self};
use tracing_actix_web::TracingLogger;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    set_name_and_version(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    logger::init_logger().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    dotenvy::dotenv().ok();
    let env: Env = envy::from_env().map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to parse environment variables: {}", e),
        )
    })?;

    let state = BalanceProver::new().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let state = Data::new(state);
    HttpServer::new(move || {
        let cors = Cors::permissive();
        App::new()
            .wrap(cors)
            .wrap(TracingLogger::<logger::CustomRootSpanBuilder>::new())
            .app_data(state.clone())
            .service(health_check)
            .service(balance_prover_scope())
    })
    .bind(format!("0.0.0.0:{}", env.port))?
    .run()
    .await
}
