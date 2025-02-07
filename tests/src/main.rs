use actix_web::{get, post, web, App, HttpResponse, HttpServer, Responder};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct LoadConfig {
    num_senders: u32,

    tps: u32,
    concurrency: u32,
    duration: u32,
}

#[derive(Debug)]
struct AppState {
    config: Mutex<LoadConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct EnvVar {
    #[serde(default = "default_url")]
    server_url: String,

    num_senders: u32,
    tps: Option<u32>,
    concurrency: Option<u32>,
    duration_seconds: Option<u32>,
}

fn default_url() -> String {
    "0.0.0.0:8080".to_string()
}

#[get("/config")]
async fn get_config(data: web::Data<AppState>) -> impl Responder {
    let config = data.config.lock().unwrap().clone();
    HttpResponse::Ok().json(config)
}

#[post("/config")]
async fn update_config(
    data: web::Data<AppState>,
    new_config: web::Json<LoadConfig>,
) -> impl Responder {
    let mut config = data.config.lock().unwrap();
    *config = new_config.into_inner();
    log::info!("Config updated: {:?}", *config);
    HttpResponse::Ok().body("Config updated")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().expect("Failed to read .env file");
    let config = envy::from_env::<EnvVar>().unwrap();
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let initial_config = LoadConfig {
        num_senders: config.num_senders,
        tps: config.tps.unwrap_or(10),
        concurrency: config.concurrency.unwrap_or(5),
        duration: config.duration_seconds.unwrap_or(30),
    };

    let app_state = web::Data::new(AppState {
        config: Mutex::new(initial_config),
    });

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .service(get_config)
            .service(update_config)
    })
    .bind(&config.server_url)?
    .run()
    .await
}
