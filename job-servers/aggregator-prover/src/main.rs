extern crate actix;
extern crate actix_web;
extern crate config;

use std::env;

use actix_web::{middleware, web, App, HttpServer};
use actix_web_validator::{JsonConfig, PathConfig, QueryConfig};
use app::state::AppState;

use crate::app::error_handler::handle_error;

pub mod app;
pub mod server;

#[actix_rt::main]
async fn main() -> Result<(), std::io::Error> {
    let hostname: String = app::config::get("hostname");

    let log_level = env::var("RUST_LOG").unwrap_or("info".to_string());
    std::env::set_var("RUST_LOG", log_level);
    env_logger::init();

    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let redis_url = env::var("REDIS_URL").expect("REDIS_URL must be set");
    let listen_address = format!("{}:{}", hostname, port);
    let redis = match redis::Client::open(redis_url) {
        Ok(client) => client,
        Err(e) => {
            log::error!("Failed to create Redis client: {}", e);
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to create Redis client",
            ));
        }
    };
    let is_faster_mining = env::var("IS_FASTER_MINING").expect("IS_FASTER_MINING must be set");
    let is_faster_mining = is_faster_mining
        .parse::<bool>()
        .expect("IS_FASTER_MINING must be a boolean");
    if is_faster_mining {
        log::info!("Faster mining is enabled");
    } else {
        log::info!("Faster mining is disabled");
    }
    let state = AppState::new(is_faster_mining);
    log::info!("Listening to requests at {}...", listen_address);
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(redis.clone()))
            .app_data(web::Data::new(state.clone()))
            .app_data(PathConfig::default().error_handler(handle_error))
            .app_data(QueryConfig::default().error_handler(handle_error))
            .app_data(JsonConfig::default().error_handler(handle_error))
            .configure(app::route::setup_routes)
            .wrap(middleware::Logger::default())
    })
    .bind(listen_address)?
    .run()
    .await
}
