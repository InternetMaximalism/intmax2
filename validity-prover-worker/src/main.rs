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

    let mut handles = vec![];
    for _ in 0..env.num_process {
        let worker = Worker::new(&env, transition_processor.clone())?;
        handles.extend(worker.run().await);
    }

    let result = futures::future::join_all(handles).await;
    for res in result {
        res?;
    }

    Ok(())
}
