use intmax2_zkp::ethereum_types::{address::Address, bytes32::Bytes32};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};
use serde::{Deserialize, Serialize};

use crate::{api::store_vault_server::types::CursorOrder, utils::signature::Signable};

use super::interface::{ClaimInfo, FeeResult, WithdrawalInfo};

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

// a prefix to make the content unique
fn content_prefix(path: &str) -> Vec<u8> {
    format!("intmax2/v1/withdrawal-server/{path}",)
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
pub struct GetWithdrawalInfoRequest {
    pub cursor: TimestampCursor,
}

impl Signable for GetWithdrawalInfoRequest {
    fn content(&self) -> Vec<u8> {
        content_prefix("get_withdrawal_info")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetWithdrawalInfoResponse {
    pub withdrawal_info: Vec<WithdrawalInfo>,
    pub cursor_response: TimestampCursorResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetClaimInfoRequest {
    pub cursor: TimestampCursor,
}

impl Signable for GetClaimInfoRequest {
    fn content(&self) -> Vec<u8> {
        content_prefix("get_claim_info")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetClaimInfoResponse {
    pub claim_info: Vec<ClaimInfo>,
    pub cursor_response: TimestampCursorResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetWithdrawalInfoByRecipientQuery {
    pub recipient: Address,
    pub cursor: TimestampCursor,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimestampCursor {
    pub cursor: Option<u64>, // Optional timestamp in seconds
    pub order: CursorOrder,
    pub limit: Option<u32>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimestampCursorResponse {
    pub next_cursor: Option<u64>,
    pub has_more: bool,
    pub total_count: u32,
}
