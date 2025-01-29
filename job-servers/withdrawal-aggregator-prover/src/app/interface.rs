use intmax2_zkp::common::withdrawal::Withdrawal;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct HealthCheckResponse {
    pub message: String,
    pub timestamp: u128,
    pub uptime: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawalProofRequest {
    pub id: String,
    pub prev_withdrawal_proof: Option<String>,
    pub single_withdrawal_proof: String, // base64 encoded
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawalWrapperProofRequest {
    pub id: String,
    pub withdrawal_proof: String,
    pub withdrawal_aggregator: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofContent {
    pub proof: String, // public inputs included
    pub withdrawal: Withdrawal,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawalProofResponse {
    pub success: bool,
    pub proof: Option<ProofContent>,
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofResponse {
    pub success: bool,
    pub proof: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Serialize)]
pub struct GenerateProofResponse {
    pub success: bool,
    pub message: String,
}
