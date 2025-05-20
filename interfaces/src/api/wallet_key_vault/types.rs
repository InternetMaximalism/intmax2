use alloy::primitives::Address;
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeRequest {
    pub address: Address,
    #[serde(rename = "type")]
    pub request_type: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChallengeResponse {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginRequest {
    pub address: Address,
    pub challenge_signature: String,
    pub security_seed: String,
}

#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponse {
    #[serde_as(as = "Base64")]
    pub hashed_signature: Vec<u8>,
    pub nonce: u32,
    pub encrypted_entropy: Option<String>,
    pub access_token: Option<String>,
}
