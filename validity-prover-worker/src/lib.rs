use serde::Deserialize;

pub mod app;

#[derive(Deserialize)]
pub struct EnvVar {
    pub redis_url: String,
    pub heartbeat_interval: u64,
    pub num_process: u32,
}
