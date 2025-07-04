use actix_web::{web::Data, App, HttpServer};
use block_builder::{
    api::{routes::block_builder_scope, state::State},
    EnvVar,
};
use server_common::health_check::{health_check, set_name_and_version};
use uuid::Uuid;

async fn run_builder(env: EnvVar, port: u16) {
    set_name_and_version(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    let mut env = env;
    env.cluster_id = Some(Uuid::new_v4().to_string());
    let state = State::new(&env).await.unwrap();
    state.run();

    let data = Data::new(state);
    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .service(health_check)
            .service(block_builder_scope())
    })
    .bind(format!("0.0.0.0:{port}"))
    .unwrap()
    .run()
    .await
    .unwrap();
}

#[actix_rt::test]
#[ignore]
async fn test_e2e_block_builder() {
    dotenvy::dotenv().ok();

    let env = envy::from_env::<EnvVar>().unwrap();
    for port in 9100..9110 {
        let env = env.clone();
        actix_rt::spawn(async move {
            run_builder(env.clone(), port).await;
        });
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
}
