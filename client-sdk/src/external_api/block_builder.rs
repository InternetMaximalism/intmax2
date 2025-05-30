use async_trait::async_trait;
use intmax2_interfaces::api::{
    block_builder::{
        interface::{BlockBuilderClientInterface, BlockBuilderFeeInfo, FeeProof},
        types::{
            PostSignatureRequest, QueryProposalRequest, QueryProposalResponse, TxRequestRequest,
            TxRequestResponse,
        },
    },
    error::ServerError,
};
use intmax2_zkp::{
    common::{block_builder::BlockProposal, signature_content::flatten::FlatG2, tx::Tx},
    ethereum_types::u256::U256,
};

use super::utils::query::{get_request, post_request};

pub const DEFAULT_BLOCK_EXPIRY: u64 = 80;

#[derive(Debug, Clone)]
pub struct BlockBuilderClient;

impl BlockBuilderClient {
    pub fn new() -> Self {
        BlockBuilderClient
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
        get_request::<(), BlockBuilderFeeInfo>(block_builder_url, "/block-builder/fee-info", None)
            .await
    }

    async fn send_tx_request(
        &self,
        block_builder_url: &str,
        is_registration_block: bool,
        pubkey: U256,
        tx: Tx,
        fee_proof: Option<FeeProof>,
    ) -> Result<String, ServerError> {
        let request = TxRequestRequest {
            is_registration_block,
            pubkey,
            tx,
            fee_proof,
        };
        let response: TxRequestResponse = post_request(
            block_builder_url,
            "/block-builder/tx-request",
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
            block_builder_url,
            "/block-builder/query-proposal",
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
            block_builder_url,
            "/block-builder/post-signature",
            Some(&request),
        )
        .await
    }
}
