use intmax2_client_sdk::external_api::utils::query::{get_request, post_request};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestConfig {
    concurrent_limit: usize,
    end: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Response {
    message: String,
}

async fn get_config() -> Result<TestConfig, Box<dyn std::error::Error>> {
    let config = get_request::<(), TestConfig>("http://localhost:8080", "/config", None).await?;

    Ok(config)
}

const MAX_CONCURRENT_LIMIT: usize = 800;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Check the current config
    let config = get_config().await?;
    post_request::<TestConfig, Response>("http://localhost:8080", "/config", Some(&config)).await?;

    loop {
        let mut config = get_config().await?;
        if config.end == "true" {
            break;
        }

        if config.concurrent_limit > MAX_CONCURRENT_LIMIT {
            config.concurrent_limit += MAX_CONCURRENT_LIMIT;
        } else {
            config.concurrent_limit *= 2;
        }

        let now = chrono::Local::now();
        let one_hour_later = now + chrono::Duration::hours(1);
        println!(
            "next config: {:?}, time: {}",
            config,
            one_hour_later.format("%Y-%m-%d %H:%M:%S")
        );
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        post_request::<TestConfig, Response>("http://localhost:8080", "/config", Some(&config))
            .await?;
    }

    Ok(())
}
