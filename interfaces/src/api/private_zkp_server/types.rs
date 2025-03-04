use std::fmt::Display;

use intmax2_zkp::ethereum_types::u256::U256;
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};

use crate::data::encryption::{BlsEncryption, RsaEncryption};

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

// -------------- inter types ----------------

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
    Dummy, // for testing
}

impl Display for ProveType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProveType::Spent => write!(f, "spent"),
            ProveType::Send => write!(f, "send"),
            ProveType::Update => write!(f, "update"),
            ProveType::ReceiveTransfer => write!(f, "receiveTransfer"),
            ProveType::ReceiveDeposit => write!(f, "receiveDeposit"),
            ProveType::SingleWithdrawal => write!(f, "singleWithdrawal"),
            ProveType::SingleClaim => write!(f, "singleClaim"),
            ProveType::Dummy => write!(f, "dummy"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProveRequestWithType {
    pub prove_type: ProveType,
    pub pubkey: U256,
    pub request: Vec<u8>,
}

impl RsaEncryption for ProveRequestWithType {}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofResultWithError {
    pub proof: Option<ProofWithPublicInputs<F, C, D>>,
    pub error: Option<String>,
}

impl BlsEncryption for ProofResultWithError {}

// ----------------- api types -------------------

#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProveRequest {
    #[serde_as(as = "Base64")]
    pub encrypted_data: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProofResponse {
    pub request_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofResultQuery {
    pub request_id: String,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofResultResponse {
    pub status: String,
    #[serde_as(as = "Option<Base64>")]
    pub result: Option<Vec<u8>>, // contains encrypted `ProofResultWithError`
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPublicKeyResponse {
    pub public_key: String,
}
