use actix_cors::Cors;
use actix_web::{
    web::{Data, JsonConfig},
    App, HttpServer,
};
use server_common::{
    health_check::{health_check, set_name_and_version},
    logger,
};
use std::io::{self};
use store_vault_server::{
    api::{routes::store_vault_server_scope, state::State},
    app::store_vault_server::StoreVaultServer,
    // middleware::authorization::authorization_middleware,
    EnvVar,
};
use tracing_actix_web::TracingLogger;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    set_name_and_version(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    logger::init_logger().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    dotenv::dotenv().ok();

    let env: EnvVar = envy::from_env().map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to parse environment variables: {}", e),
        )
    })?;
    let store_vault_server = StoreVaultServer::new(&env).await.map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to initialize store_vault_server: {}", e),
        )
    })?;
    let state = Data::new(State::new(store_vault_server));

    HttpServer::new(move || {
        let cors = Cors::permissive();
        App::new()
            .wrap(cors)
            .wrap(TracingLogger::<logger::CustomRootSpanBuilder>::new())
            // .wrap(from_fn(authorization_middleware))
            .app_data(JsonConfig::default().limit(35_000_000))
            .app_data(state.clone())
            .service(health_check)
            .service(store_vault_server_scope())
    })
    .bind(format!("0.0.0.0:{}", env.port))?
    .run()
    .await
}
