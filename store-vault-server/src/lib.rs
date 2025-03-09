use serde::Deserialize;

pub mod api;
pub mod app;

#[derive(Deserialize)]
pub struct EnvVar {
    pub port: u16,
    pub database_url: String,
    pub database_max_connections: u32,
    pub database_timeout: u64,
}
