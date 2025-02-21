use std::time::Duration;

use intmax2_zkp::common::signature::key_set::KeySet;
use tests::log_polling_futures;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let mut rnd = rand::thread_rng();

    let num_senders = 10;
    let senders = (0..num_senders)
        .map(|_| KeySet::rand(&mut rnd))
        .collect::<Vec<_>>();

    let timeout_duration = Duration::from_secs(5);

    let mut futures = (0..num_senders)
        .map(|i| {
            let f = sleep(i);
            Box::pin(async move {
                tokio::time::timeout(timeout_duration, f)
                    .await
                    .map_err(|_| format!("Operation {} timed out", i))?
            })
        })
        .collect::<Vec<_>>();

    tokio::time::sleep(Duration::from_secs(5)).await;

    log_polling_futures(&mut futures, &senders).await;

    Ok(())
}

async fn sleep(i: usize) -> Result<(), Box<dyn std::error::Error>> {
    let delay = Duration::from_secs(i as u64);
    tokio::time::sleep(delay).await;
    Ok(())
}
