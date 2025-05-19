use super::types::{
    BlockPostedEntry, BlockPostedsData, DepositLeafInsertedData, DepositLeafInsertedEntry,
    DepositedData, DepositedEntry, GraphQLResponse,
};
use intmax2_client_sdk::external_api::utils::query::post_request_with_bearer_token;
use intmax2_interfaces::api::error::ServerError;
use serde_json::json;

#[derive(Clone, Debug)]
pub struct TheGraphQueryClient {
    pub l1_url: String,
    pub l1_bearer_token: Option<String>,
    pub l2_url: String,
    pub l2_bearer_token: Option<String>,
}

impl TheGraphQueryClient {
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
        query GetBlocks($nextBlockNumber: BigInt!, $limit: Int!) {
        blockPosteds(
            first: $limit,
            where: { rollupBlockNumber_gte: $nextBlockNumber }
            orderBy: rollupBlockNumber
        ) {
            prevBlockHash
            blockBuilder
            depositTreeRoot
            rollupBlockNumber
            blockTimestamp
            transactionHash
        }
        }
        "#;
        let request = json!({
            "query": query,
            "variables": {
                "nextBlockNumber": next_block_number,
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

    pub async fn fetch_deposit_leaves(
        &self,
        next_deposit_index: u32,
        limit: usize,
    ) -> Result<Vec<DepositLeafInsertedEntry>, ServerError> {
        let query = r#"
        query GetDepositLeaves($nextDepositIndex: BigInt!, $limit: Int!) {
        depositLeafInserteds(
            first: $limit,
            where: { depositIndex_gte: $nextDepositIndex }
            orderBy: depositIndex
        ) {
            depositHash
            depositIndex
            transactionHash
        }
        }
        "#;
        let request = json!({
            "query": query,
            "variables": {
                "nextDepositIndex": next_deposit_index,
                "limit": limit,
            }
        });
        let response: GraphQLResponse<DepositLeafInsertedData> = post_request_with_bearer_token(
            &self.l2_url,
            "",
            self.l2_bearer_token.clone(),
            Some(&request),
        )
        .await?;
        Ok(response.data.deposit_leaf_inserteds)
    }

    pub async fn fetch_deposited(
        &self,
        next_deposit_id: u64,
        limit: usize,
    ) -> Result<Vec<DepositedEntry>, ServerError> {
        let query = r#"
        query GetDeposited($nextDepositId: BigInt!, $limit: Int!) {
        depositeds(
            first: $limit,
            where: { depositId_gte: $nextDepositId }
            orderBy: depositId
        ) {
            depositId
            sender
            tokenIndex
            amount
            recipientSaltHash
            isEligible
            depositedAt
            transactionHash
        }
        }
        "#;
        let request = json!({
            "query": query,
            "variables": {
                "nextDepositId": next_deposit_id,
                "limit": limit,
            }
        });
        let response: GraphQLResponse<DepositedData> = post_request_with_bearer_token(
            &self.l1_url,
            "",
            self.l1_bearer_token.clone(),
            Some(&request),
        )
        .await?;
        Ok(response.data.depositeds)
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;

    fn create_client() -> TheGraphQueryClient {
        dotenvy::dotenv().ok();
        let l1_url = env::var("THE_GRAPH_L1_URL").unwrap_or_else(|_| {
            "http://localhost:8000/subgraphs/name/liquidity-subgraph".to_string()
        });
        let l2_url = env::var("THE_GRAPH_L2_URL")
            .unwrap_or_else(|_| "http://localhost:8000/subgraphs/name/rollup-subgraph".to_string());
        let l1_bearer_token = env::var("THE_GRAPH_L1_BEARER").ok();
        let l2_bearer_token = env::var("THE_GRAPH_L2_BEARER").ok();
        TheGraphQueryClient::new(l1_url, l2_url, l1_bearer_token, l2_bearer_token)
    }

    #[tokio::test]
    #[ignore]
    async fn test_fetch_block_posteds() {
        let client = create_client();

        let next_block_number = 1;
        let limit = 1;
        let result = client
            .fetch_block_posteds(next_block_number, limit)
            .await
            .unwrap();
        assert_eq!(result.len(), limit);
        assert_eq!(result[0].rollup_block_number, next_block_number);
    }

    #[tokio::test]
    #[ignore]
    async fn test_fetch_deposit_leaf_inserteds() {
        let client = create_client();

        let next_deposit_index = 1;
        let limit = 1;
        let result = client
            .fetch_deposit_leaves(next_deposit_index, limit)
            .await
            .unwrap();
        assert_eq!(result.len(), limit);
        assert_eq!(result[0].deposit_index, next_deposit_index);
    }

    #[tokio::test]
    #[ignore]
    async fn test_fetch_deposited() {
        let client = create_client();

        let next_deposit_id = 1;
        let limit = 1;
        let result = client
            .fetch_deposited(next_deposit_id, limit)
            .await
            .unwrap();
        assert_eq!(result.len(), limit);
        assert_eq!(result[0].deposit_id, next_deposit_id);
    }
}
