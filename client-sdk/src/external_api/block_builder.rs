use async_trait::async_trait;
use intmax2_interfaces::api::{
    block_builder::{
        interface::{BlockBuilderClientInterface, BlockBuilderStatus, FeeProof},
        types::{
            GetBlockBuilderStatusQuery, GetBlockBuilderStatusResponse, PostSignatureRequest,
            QueryProposalRequest, QueryProposalResponse, TxRequestRequest,
        },
    },
    error::ServerError,
};
use intmax2_zkp::{
    common::{block_builder::BlockProposal, signature::flatten::FlatG2, tx::Tx},
    ethereum_types::u256::U256,
};

use super::utils::query::{get_request, post_request};

#[derive(Debug, Clone)]
pub struct BlockBuilderClient;

impl BlockBuilderClient {
    pub fn new() -> Self {
        BlockBuilderClient
    }
}

#[async_trait(?Send)]
impl BlockBuilderClientInterface for BlockBuilderClient {
    async fn get_status(
        &self,
        block_builder_url: &str,
        is_registration_block: bool,
    ) -> Result<BlockBuilderStatus, ServerError> {
        let query = GetBlockBuilderStatusQuery {
            is_registration_block,
        };
        let response = get_request::<GetBlockBuilderStatusQuery, GetBlockBuilderStatusResponse>(
            block_builder_url,
            "/block-builder/status",
            Some(query),
            None,
        )
        .await?;
        Ok(response.status)
    }

    async fn send_tx_request(
        &self,
        block_builder_url: &str,
        is_registration_block: bool,
        pubkey: U256,
        tx: Tx,
        fee_proof: Option<FeeProof>,
    ) -> Result<(), ServerError> {
        let request = TxRequestRequest {
            is_registration_block,
            pubkey,
            tx,
            fee_proof,
        };
        post_request::<_, ()>(
            block_builder_url,
            "/block-builder/tx-request",
            &request,
            None,
        )
        .await
    }

    async fn query_proposal(
        &self,
        block_builder_url: &str,
        is_registration_block: bool,
        pubkey: U256,
        tx: Tx,
    ) -> Result<Option<BlockProposal>, ServerError> {
        let request = QueryProposalRequest {
            is_registration_block,
            pubkey,
            tx,
        };
        let response: QueryProposalResponse = post_request(
            block_builder_url,
            "/block-builder/query-proposal",
            &request,
            None,
        )
        .await?;
        Ok(response.block_proposal)
    }

    async fn post_signature(
        &self,
        block_builder_url: &str,
        is_registration_block: bool,
        pubkey: U256,
        tx: Tx,
        signature: FlatG2,
    ) -> Result<(), ServerError> {
        let request = PostSignatureRequest {
            is_registration_block,
            pubkey,
            tx,
            signature,
        };
        post_request::<_, ()>(
            block_builder_url,
            "/block-builder/post-signature",
            &request,
            None,
        )
        .await
    }
}
