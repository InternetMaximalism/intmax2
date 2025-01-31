use intmax2_zkp::{
    common::{claim::Claim, withdrawal::Withdrawal},
    ethereum_types::address::Address,
};
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};

#[derive(Serialize)]
pub struct HealthCheckResponse {
    pub message: String,
    pub timestamp: u128,
    pub uptime: f64,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawalProofRequest {
    pub id: String,
    #[serde_as(as = "Option<Base64>")]
    pub prev_withdrawal_proof: Option<Vec<u8>>,
    #[serde_as(as = "Base64")]
    pub single_withdrawal_proof: Vec<u8>,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimProofRequest {
    pub id: String,
    #[serde_as(as = "Option<Base64>")]
    pub prev_claim_proof: Option<Vec<u8>>,
    #[serde_as(as = "Base64")]
    pub single_claim_proof: Vec<u8>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawalProofResponse {
    pub success: bool,
    pub proof: Option<WithdrawalProofContent>,
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimProofResponse {
    pub success: bool,
    pub proof: Option<ClaimProofContent>,
    pub error_message: Option<String>,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawalProofContent {
    #[serde_as(as = "Base64")]
    pub proof: Vec<u8>,
    pub withdrawal: Withdrawal,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimProofContent {
    #[serde_as(as = "Base64")]
    pub proof: Vec<u8>,
    pub claim: Claim,
}

#[derive(Serialize)]
pub struct GenerateProofResponse {
    pub success: bool,
    pub message: String,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawalWrapperProofRequest {
    pub id: String,
    #[serde_as(as = "Base64")]
    pub withdrawal_proof: Vec<u8>,
    pub withdrawal_aggregator: Address,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimWrapperProofRequest {
    pub id: String,
    #[serde_as(as = "Base64")]
    pub claim_proof: Vec<u8>,
    pub claim_aggregator: Address,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WrapperProofResponse {
    pub success: bool,
    pub proof: Option<String>, // json string of withdrawal wrap proof
    pub error_message: Option<String>,
}
