use server_common::logger::init_logger;
use validity_prover_worker::{app::worker::Worker, EnvVar};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    init_logger()?;

    let env = envy::from_env::<EnvVar>().unwrap();
    let worker = Worker::new(&env)?;
    worker.run().await;

    Ok(())
}
