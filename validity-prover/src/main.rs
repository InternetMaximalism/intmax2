use actix_cors::Cors;
use actix_web::{middleware::Logger, web::Data, App, HttpServer};
use server_common::{
    health_check::{health_check, set_name_and_version},
    logger::init_logger,
};

use validity_prover::{
    api::{api::validity_prover_scope, state::State, validity_prover::ValidityProver},
    Env,
};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    set_name_and_version(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    init_logger().map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    dotenv::dotenv().ok();
    let env: Env = envy::from_env().map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to parse environment variables: {}", e),
        )
    })?;
    let validity_prover = ValidityProver::new(&env).await.map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to create validity prover: {}", e),
        )
    })?;
    validity_prover.start_sync();

    let inner_state = State::new(validity_prover);
    let state = Data::new(inner_state.clone());

    HttpServer::new(move || {
        let cors = Cors::permissive();
        App::new()
            .wrap(cors)
            .wrap(Logger::new("Request: %r | Status: %s | Duration: %Ts"))
            .app_data(state.clone())
            .service(health_check)
            .service(validity_prover_scope())
    })
    .bind(format!("0.0.0.0:{}", env.port))?
    .run()
    .await
}
