use actix_cors::Cors;
use actix_web::{
    web::{Data, JsonConfig},
    App, HttpServer,
};
use server_common::{
    health_check::{health_check, set_name_and_version},
    logger,
    version_check::VersionCheck,
};
use std::io::{self};
use store_vault_server::{
    api::{routes::s3_store_vault_scope, state::State},
    app::s3_store_vault::S3StoreVault,
    EnvVar,
};
use tracing_actix_web::TracingLogger;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    set_name_and_version(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    logger::init_logger().map_err(io::Error::other)?;

    dotenvy::dotenv().ok();

    let env: EnvVar = envy::from_env()
        .map_err(|e| io::Error::other(format!("Failed to parse environment variables: {e}")))?;
    let s3_store_vault = S3StoreVault::new(&env)
        .await
        .map_err(|e| io::Error::other(format!("Failed to initialize s3_store_vault: {e}")))?;

    // start tasks
    s3_store_vault.run();

    let state = Data::new(State::new(s3_store_vault));
    build_server(state, &env).await
}

async fn build_server(state: Data<State>, env: &EnvVar) -> std::io::Result<()> {
    HttpServer::new(move || {
        let cors = Cors::permissive();
        App::new()
            .wrap(cors)
            .wrap(TracingLogger::<logger::CustomRootSpanBuilder>::new())
            .wrap(VersionCheck::from_env())
            .app_data(JsonConfig::default().limit(35_000_000))
            .app_data(state.clone())
            .service(health_check)
            .service(s3_store_vault_scope())
    })
    .bind(format!("0.0.0.0:{}", env.port))?
    .run()
    .await
}
