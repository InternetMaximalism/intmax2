use intmax2_zkp::ethereum_types::{address::Address, bytes32::Bytes32};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};
use serde::{Deserialize, Serialize};

use crate::utils::signature::Signable;

use super::interface::{ClaimInfo, FeeResult, WithdrawalInfo};

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

// a prefix to make the content unique
fn content_prefix(path: &str) -> Vec<u8> {
    format!("intmax2/v1/withdrawal-server/{}", path)
        .as_bytes()
        .to_vec()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestWithdrawalRequest {
    pub single_withdrawal_proof: ProofWithPublicInputs<F, C, D>,
    pub fee_token_index: Option<u32>,
    pub fee_transfer_digests: Vec<Bytes32>,
}

impl Signable for RequestWithdrawalRequest {
    fn content(&self) -> Vec<u8> {
        [
            content_prefix("request_withdrawal"),
            bincode::serialize(&(
                self.single_withdrawal_proof.clone(),
                self.fee_token_index,
                self.fee_transfer_digests.clone(),
            ))
            .unwrap(),
        ]
        .concat()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestWithdrawalResponse {
    pub fee_result: FeeResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestClaimRequest {
    pub single_claim_proof: ProofWithPublicInputs<F, C, D>,
    pub fee_token_index: Option<u32>,
    pub fee_transfer_digests: Vec<Bytes32>,
}

impl Signable for RequestClaimRequest {
    fn content(&self) -> Vec<u8> {
        [
            content_prefix("request_claim"),
            bincode::serialize(&(
                self.single_claim_proof.clone(),
                self.fee_token_index,
                self.fee_transfer_digests.clone(),
            ))
            .unwrap(),
        ]
        .concat()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestClaimResponse {
    pub fee_result: FeeResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetWithdrawalInfoRequest;

impl Signable for GetWithdrawalInfoRequest {
    fn content(&self) -> Vec<u8> {
        content_prefix("get_withdrawal_info")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetWithdrawalInfoResponse {
    pub withdrawal_info: Vec<WithdrawalInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetClaimInfoRequest;

impl Signable for GetClaimInfoRequest {
    fn content(&self) -> Vec<u8> {
        content_prefix("get_claim_info")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetClaimInfoResponse {
    pub claim_info: Vec<ClaimInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetWithdrawalInfoByRecipientQuery {
    pub recipient: Address,
}
