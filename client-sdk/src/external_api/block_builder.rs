use async_trait::async_trait;
use intmax2_interfaces::{
    api::{
        block_builder::{
            interface::{BlockBuilderClientInterface, BlockBuilderFeeInfo, FeeProof},
            types::{
                PostSignatureRequest, QueryProposalRequest, QueryProposalResponse,
                TxRequestRequest, TxRequestResponse,
            },
        },
        error::ServerError,
    },
    utils::address::IntmaxAddress,
};
use intmax2_zkp::{
    common::{block_builder::BlockProposal, signature_content::flatten::FlatG2, tx::Tx},
    ethereum_types::u256::U256,
};
use reqwest::Client;

use crate::external_api::utils::query::build_client;

use super::utils::query::{get_request, post_request};

pub const DEFAULT_BLOCK_EXPIRY: u64 = 80;

#[derive(Debug, Clone)]
pub struct BlockBuilderClient {
    client: Client,
}

impl BlockBuilderClient {
    pub fn new() -> Self {
        BlockBuilderClient {
            client: build_client(),
        }
    }
}

impl Default for BlockBuilderClient {
    fn default() -> Self {
        BlockBuilderClient::new()
    }
}

#[async_trait(?Send)]
impl BlockBuilderClientInterface for BlockBuilderClient {
    async fn get_fee_info(
        &self,
        block_builder_url: &str,
    ) -> Result<BlockBuilderFeeInfo, ServerError> {
        get_request::<(), BlockBuilderFeeInfo>(&self.client, block_builder_url, "/fee-info", None)
            .await
    }

    async fn send_tx_request(
        &self,
        block_builder_url: &str,
        is_registration_block: bool,
        sender: IntmaxAddress,
        tx: Tx,
        fee_proof: Option<FeeProof>,
    ) -> Result<String, ServerError> {
        let request = TxRequestRequest {
            is_registration_block,
            sender,
            tx,
            fee_proof,
        };
        let response: TxRequestResponse = post_request(
            &self.client,
            block_builder_url,
            "/tx-request",
            Some(&request),
        )
        .await?;
        Ok(response.request_id)
    }

    async fn query_proposal(
        &self,
        block_builder_url: &str,
        request_id: &str,
    ) -> Result<Option<BlockProposal>, ServerError> {
        let request = QueryProposalRequest {
            request_id: request_id.to_string(),
        };
        let response: QueryProposalResponse = post_request(
            &self.client,
            block_builder_url,
            "/query-proposal",
            Some(&request),
        )
        .await?;
        Ok(response.block_proposal)
    }

    async fn post_signature(
        &self,
        block_builder_url: &str,
        request_id: &str,
        pubkey: U256,
        signature: FlatG2,
    ) -> Result<(), ServerError> {
        let request = PostSignatureRequest {
            request_id: request_id.to_string(),
            pubkey,
            signature,
        };
        post_request::<_, ()>(
            &self.client,
            block_builder_url,
            "/post-signature",
            Some(&request),
        )
        .await
    }
}
