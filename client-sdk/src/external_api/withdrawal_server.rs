use super::utils::query::{get_request, post_request};
use async_trait::async_trait;
use intmax2_interfaces::{
    api::{
        block_builder::interface::Fee,
        error::ServerError,
        withdrawal_server::{
            interface::{ClaimInfo, WithdrawalInfo, WithdrawalServerClientInterface},
            types::{
                GetClaimInfoRequest, GetClaimInfoResponse, GetFeeResponse,
                GetWithdrawalInfoByRecipientQuery, GetWithdrawalInfoRequest,
                GetWithdrawalInfoResponse, RequestClaimRequest, RequestWithdrawalRequest,
            },
        },
    },
    utils::signature::Signable,
};
use intmax2_zkp::{common::signature::key_set::KeySet, ethereum_types::address::Address};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};

const TIME_TO_EXPIRY: u64 = 60; // 1 minute

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

#[derive(Debug, Clone)]
pub struct WithdrawalServerClient {
    base_url: String,
}

impl WithdrawalServerClient {
    pub fn new(base_url: &str) -> Self {
        WithdrawalServerClient {
            base_url: base_url.to_string(),
        }
    }
}

#[async_trait(?Send)]
impl WithdrawalServerClientInterface for WithdrawalServerClient {
    async fn get_withdrawal_fee(&self) -> Result<Option<Vec<Fee>>, ServerError> {
        let response: GetFeeResponse =
            get_request::<(), _>(&self.base_url, "/withdrawal-server/withdrawal-fee", None).await?;
        Ok(response.fees)
    }

    async fn get_claim_fee(&self) -> Result<Option<Vec<Fee>>, ServerError> {
        let response: GetFeeResponse =
            get_request::<(), _>(&self.base_url, "/withdrawal-server/claim-fee", None).await?;
        Ok(response.fees)
    }

    async fn request_withdrawal(
        &self,
        key: KeySet,
        single_withdrawal_proof: &ProofWithPublicInputs<F, C, D>,
        fee_transfer_uuids: &[String],
    ) -> Result<(), ServerError> {
        let request = RequestWithdrawalRequest {
            single_withdrawal_proof: single_withdrawal_proof.clone(),
            fee_transfer_uuids: fee_transfer_uuids.to_vec(),
        };
        let request_with_auth = request.sign(key, TIME_TO_EXPIRY);
        post_request::<_, ()>(
            &self.base_url,
            "/withdrawal-server/request-withdrawal",
            Some(&request_with_auth),
        )
        .await
    }

    async fn request_claim(
        &self,
        key: KeySet,
        single_claim_proof: &ProofWithPublicInputs<F, C, D>,
        fee_transfer_uuids: &[String],
    ) -> Result<(), ServerError> {
        let request = RequestClaimRequest {
            single_claim_proof: single_claim_proof.clone(),
            fee_transfer_uuids: fee_transfer_uuids.to_vec(),
        };
        let request_with_auth = request.sign(key, TIME_TO_EXPIRY);
        post_request::<_, ()>(
            &self.base_url,
            "/withdrawal-server/request-claim",
            Some(&request_with_auth),
        )
        .await
    }

    async fn get_withdrawal_info(&self, key: KeySet) -> Result<Vec<WithdrawalInfo>, ServerError> {
        let request = GetWithdrawalInfoRequest;
        let request_with_auth = request.sign(key, TIME_TO_EXPIRY);
        let response: GetWithdrawalInfoResponse = post_request(
            &self.base_url,
            "/withdrawal-server/get-withdrawal-info",
            Some(&request_with_auth),
        )
        .await?;
        Ok(response.withdrawal_info)
    }

    async fn get_withdrawal_info_by_recipient(
        &self,
        recipient: Address,
    ) -> Result<Vec<WithdrawalInfo>, ServerError> {
        let query = GetWithdrawalInfoByRecipientQuery { recipient };
        let response: GetWithdrawalInfoResponse = get_request(
            &self.base_url,
            "/withdrawal-server/get-withdrawal-info-by-recipient",
            Some(query),
        )
        .await?;
        Ok(response.withdrawal_info)
    }

    async fn get_claim_info(&self, key: KeySet) -> Result<Vec<ClaimInfo>, ServerError> {
        let request = GetClaimInfoRequest;
        let request_with_auth = request.sign(key, TIME_TO_EXPIRY);
        let response: GetClaimInfoResponse = post_request(
            &self.base_url,
            "/withdrawal-server/get-claim-info",
            Some(&request_with_auth),
        )
        .await?;
        Ok(response.claim_info)
    }
}
