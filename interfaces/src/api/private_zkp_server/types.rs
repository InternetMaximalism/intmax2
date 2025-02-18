use intmax2_zkp::ethereum_types::u256::U256;
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};

use crate::data::encryption::RsaEncryption;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProveType {
    Spent,
    Send,
    Update,
    ReceiveTransfer,
    ReceiveDeposit,
    SingleWithdrawal,
    SingleClaim,
    Echo, // for testing
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProveRequestWithType {
    pub prove_type: ProveType,
    pub pubkey: U256,
    pub request: Vec<u8>,
}

impl RsaEncryption for ProveRequestWithType {}

#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProveRequest {
    #[serde_as(as = "Base64")]
    pub encrypted_data: Vec<u8>,
    pub public_key: String,      // todo: remove this field
    pub transition_type: String, // todo: remove this field
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProofResponse {
    request_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofResultQuery {
    request_id: String,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofResultResponse {
    pub status: String,
    #[serde_as(as = "Option<Base64>")]
    pub result: Option<Vec<u8>>,
    #[serde_as(as = "Option<Base64>")]
    pub error: Option<Vec<u8>>,
}
