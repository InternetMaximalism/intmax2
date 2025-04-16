use intmax2_zkp::{
    common::{block_builder::BlockProposal, signature_content::flatten::FlatG2, tx::Tx},
    ethereum_types::u256::U256,
};
use serde::{Deserialize, Serialize};

use super::interface::FeeProof;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxRequestRequest {
    pub is_registration_block: bool,
    pub pubkey: U256,
    pub tx: Tx,
    pub fee_proof: Option<FeeProof>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxRequestResponse {
    pub request_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryProposalRequest {
    pub request_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryProposalResponse {
    pub block_proposal: Option<BlockProposal>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostSignatureRequest {
    pub request_id: String,
    pub pubkey: U256,
    pub signature: FlatG2,
}
