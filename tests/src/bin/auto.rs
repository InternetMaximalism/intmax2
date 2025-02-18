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

async fn get_config(
    config_server_base_url: &str,
) -> Result<TestConfig, Box<dyn std::error::Error>> {
    let config = get_request::<(), TestConfig>(config_server_base_url, "/config", None).await?;

    Ok(config)
}

const MAX_CONCURRENT_LIMIT: usize = 800;
const MAX_CONCURRENT_LIMIT2: usize = 10000;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Check the current config
    let config_server_base_url = std::env::var("CONFIG_SERVER_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:8080".to_string());
    println!("config_server_base_url: {}", config_server_base_url);

    let config = get_config(&config_server_base_url).await?;
    post_request::<TestConfig, Response>(&config_server_base_url, "/config", Some(&config)).await?;

    loop {
        let mut config = get_config(&config_server_base_url).await?;
        if config.end == "true" {
            break;
        }

        if config.concurrent_limit > MAX_CONCURRENT_LIMIT2 {
            config.concurrent_limit = MAX_CONCURRENT_LIMIT2;
        } else if config.concurrent_limit > MAX_CONCURRENT_LIMIT {
            config.concurrent_limit += MAX_CONCURRENT_LIMIT;
        } else {
            config.concurrent_limit *= 2;
        }

        let now = chrono::Local::now();
        let one_hour_later = now + chrono::Duration::minutes(30);
        println!(
            "next config: {:?}, time: {}",
            config,
            one_hour_later.format("%Y-%m-%d %H:%M:%S")
        );
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        post_request::<TestConfig, Response>(&config_server_base_url, "/config", Some(&config))
            .await?;
    }

    Ok(())
}
