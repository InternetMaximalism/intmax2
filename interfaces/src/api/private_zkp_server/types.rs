use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};

use crate::data::encryption::{rsa::RsaEncryptedMessage, RsaEncryption};

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
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProveRequestWithType {
    pub prove_type: ProveType,
    pub request: Vec<u8>,
}

impl RsaEncryption for ProveRequestWithType {}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProveRequest {
    pub data: RsaEncryptedMessage,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProveResponse {
    #[serde_as(as = "Option<Base64>")]
    pub data: Option<Vec<u8>>,
    #[serde_as(as = "Option<Base64>")]
    pub error: Option<Vec<u8>>,
}
