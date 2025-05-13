use crate::app::the_graph::types::{BlockPostedsData, GraphQLResponse};

use super::types::BlockPostedEntry;
use intmax2_client_sdk::external_api::utils::query::post_request_with_bearer_token;
use intmax2_interfaces::api::error::ServerError;
use serde_json::json;

#[derive(Clone, Debug)]
pub struct TheGraphClient {
    pub l1_url: String,
    pub l1_bearer_token: Option<String>,
    pub l2_url: String,
    pub l2_bearer_token: Option<String>,
}

impl TheGraphClient {
    pub fn new(
        l1_url: String,
        l2_url: String,
        l1_bearer_token: Option<String>,
        l2_bearer_token: Option<String>,
    ) -> Self {
        Self {
            l1_url,
            l1_bearer_token,
            l2_url,
            l2_bearer_token,
        }
    }
    pub async fn fetch_block_posteds(
        &self,
        next_block_number: u32,
        limit: usize,
    ) -> Result<Vec<BlockPostedEntry>, ServerError> {
        let query = r#"
        query GetBlocksAfterNumber($blockNumber: BigInt!, $limit: Int!) {
        blockPosteds(
            first: $limit,
            where: { rollupBlockNumber_gte: $blockNumber }
            orderBy: rollupBlockNumber
        ) {
            id
            prevBlockHash
            blockBuilder
            depositTreeRoot
            rollupBlockNumber
            timestamp
            transactionHash
        }
        }
        "#;
        let request = json!({
            "query": query,
            "variables": {
                "blockNumber": next_block_number - 1,
                "limit": limit,
            }
        });

        let response: GraphQLResponse<BlockPostedsData> = post_request_with_bearer_token(
            &self.l2_url,
            "",
            self.l2_bearer_token.clone(),
            Some(&request),
        )
        .await?;
        Ok(response.data.block_posteds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_block_posteds() {
        let client = TheGraphClient::new(
            "http://localhost:8000/subgraphs/name/liquidity-subgraph".to_string(),
            "http://localhost:8000/subgraphs/name/rollup-subgraph".to_string(),
            None,
            None,
        );
        let result = client.fetch_block_posteds(1, 1).await.unwrap();
        dbg!(result);
    }
}
