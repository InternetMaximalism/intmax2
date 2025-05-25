use async_trait::async_trait;
use intmax2_interfaces::api::{
    error::ServerError,
    indexer::interface::{BlockBuilderInfo, IndexerClientInterface},
};

use super::utils::query::get_request;

#[derive(Debug, Clone)]
pub struct IndexerClient {
    base_url: String,
}

impl IndexerClient {
    pub fn new(base_url: &str) -> Self {
        IndexerClient {
            base_url: base_url.to_string(),
        }
    }
}

#[async_trait(?Send)]
impl IndexerClientInterface for IndexerClient {
    async fn get_block_builder_info(&self) -> Result<BlockBuilderInfo, ServerError> {
        let block_builders: Vec<BlockBuilderInfo> =
            get_request::<(), _>(&self.base_url, "/v1/indexer/builders", None).await?;
        if block_builders.is_empty() {
            return Err(ServerError::InvalidResponse(
                "No block builders found".to_string(),
            ));
        }
        let client = reqwest::Client::new();
        for block_builder in &block_builders {
            if block_builder.url.parse::<reqwest::Url>().is_err() {
                log::warn!("Invalid URL for block builder: {}", block_builder.url);
                continue; // Skip invalid URLs
            }
            let fee_info_url = format!("{}/block-builder/fee-info", block_builder.url);
            // Query fee info without retry
            let response = client.get(&fee_info_url).send().await;
            match response {
                Ok(resp) if resp.status().is_success() => {
                    // Successfully retrieved fee info
                    return Ok(block_builder.clone());
                }
                Ok(resp) => {
                    log::warn!(
                        "Failed to get fee info from {}: {}",
                        block_builder.url,
                        resp.status()
                    );
                }
                Err(e) => {
                    log::warn!("Error querying fee info from {}: {}", block_builder.url, e);
                }
            }
        }
        Err(ServerError::InvalidResponse(
            "No valid block builders found".to_string(),
        ))
    }
}
