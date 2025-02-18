use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};

use crate::data::encryption::rsa::RsaEncryptedMessage;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProveRequest {
    pub data: RsaEncryptedMessage,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProveResponse {
    #[serde_as(as = "Base64")]
    pub data: Vec<u8>,
}
