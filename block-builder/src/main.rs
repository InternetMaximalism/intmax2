use actix_cors::Cors;
use actix_web::{web::Data, App, HttpServer};
use block_builder::{
    api::{routes::block_builder_scope, state::State},
    EnvVar,
};
use server_common::{
    health_check::{health_check, set_name_and_version},
    logger,
};
use std::io;
use tracing_actix_web::TracingLogger;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    set_name_and_version(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    logger::init_logger().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    dotenvy::dotenv().ok();
    let env = envy::from_env::<EnvVar>()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("env error: {}", e)))?;
    let state = State::new(&env)
        .await
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("state error: {}", e)))?;
    state.run();

    let data = Data::new(state);
    HttpServer::new(move || {
        let cors = Cors::permissive();
        App::new()
            .wrap(cors)
            .wrap(TracingLogger::<logger::CustomRootSpanBuilder>::new())
            .app_data(data.clone())
            .service(health_check)
            .service(block_builder_scope())
    })
    .bind(format!("0.0.0.0:{}", env.port))?
    .run()
    .await
}
