use std::sync::Arc;

use intmax2_zkp::circuits::validity::transition::processor::TransitionProcessor;
use server_common::logger::init_logger;
use validity_prover_worker::{app::worker::Worker, EnvVar};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    init_logger()?;

    let env = envy::from_env::<EnvVar>().unwrap();
    let transition_processor = Arc::new(TransitionProcessor::new());
    log::info!("initialized transition processor");

    let worker = Worker::new(&env, transition_processor.clone())?;
    worker.run().await;

    // keep the main thread alive
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
    }
}
