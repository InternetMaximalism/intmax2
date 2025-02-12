use std::io;

use actix_cors::Cors;
use actix_web::{web::Data, App, HttpServer};
use server_common::{
    health_check::{health_check, set_name_and_version},
    logger,
};
use tracing_actix_web::TracingLogger;
use withdrawal_server::{
    api::{routes::withdrawal_server_scope, state::State},
    Env,
};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    set_name_and_version(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    logger::init_logger().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    dotenv::dotenv().ok();

    let env = envy::from_env::<Env>()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("env error: {}", e)))?;
    let state = State::new(&env)
        .await
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("state error: {}", e)))?;
    let state = Data::new(state);
    HttpServer::new(move || {
        let cors = Cors::permissive();
        App::new()
            .wrap(cors)
            .wrap(TracingLogger::<logger::CustomRootSpanBuilder>::new())
            .app_data(state.clone())
            .service(health_check)
            .service(withdrawal_server_scope())
    })
    .bind(format!("0.0.0.0:{}", env.port))?
    .run()
    .await
}
