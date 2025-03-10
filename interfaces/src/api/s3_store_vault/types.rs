use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
};

use crate::{data::meta_data::MetaData, utils::signature::Signable};
use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u256::U256};
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct S3SaveDataEntry {
    pub topic: String,
    pub pubkey: U256,
    pub digest: Bytes32,
}

// a prefix to make the content unique
fn content_prefix(path: &str) -> Vec<u8> {
    format!("intmax2/v1/store-vault-server/{}", path)
        .as_bytes()
        .to_vec()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct S3SaveSnapshotRequest {
    pub topic: String,
    pub pubkey: U256,
    pub prev_digest: Option<Bytes32>,
    pub digest: Bytes32,
}

impl Signable for S3SaveSnapshotRequest {
    fn content(&self) -> Vec<u8> {
        [
            content_prefix("save_snapshot"),
            bincode::serialize(&(self.pubkey, self.digest, self.prev_digest)).unwrap(),
        ]
        .concat()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct S3SaveSnapshotResponse {
    pub presigned_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSnapshotRequest {
    pub pubkey: U256,
    pub topic: String,
}

impl Signable for GetSnapshotRequest {
    fn content(&self) -> Vec<u8> {
        [
            content_prefix("get_snapshot"),
            bincode::serialize(&(&self.topic, self.pubkey)).unwrap(),
        ]
        .concat()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSnapshotResponse {
    pub presigned_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveDataBatchRequest {
    pub data: Vec<S3SaveDataEntry>,
}

impl Signable for SaveDataBatchRequest {
    fn content(&self) -> Vec<u8> {
        [
            content_prefix("save_data_batch"),
            bincode::serialize(&self.data).unwrap(),
        ]
        .concat()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveDataBatchResponse {
    pub presigned_urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDataBatchRequest {
    pub topic: String,
    pub pubkey: U256,
    pub digests: Vec<Bytes32>,
}

impl Signable for GetDataBatchRequest {
    fn content(&self) -> Vec<u8> {
        // to reuse the signature, we exclude data_type and uuids from the content intentionally
        content_prefix("get_data_batch")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDataBatchResponse {
    pub presigned_urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDataSequenceRequest {
    pub topic: String,
    pub pubkey: U256,
    pub cursor: MetaDataCursor,
}

impl Signable for GetDataSequenceRequest {
    fn content(&self) -> Vec<u8> {
        // to reuse the signature, we exclude data_type and cursor from the content intentionally
        content_prefix("get_data_sequence")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDataSequenceResponse {
    pub data: Vec<DataWithMetaData>,
    pub cursor_response: MetaDataCursorResponse,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaDataCursor {
    pub cursor: Option<MetaData>,
    pub order: CursorOrder,
    pub limit: Option<u32>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub enum CursorOrder {
    #[default]
    Asc,
    Desc,
}

impl Display for CursorOrder {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            CursorOrder::Asc => write!(f, "asc"),
            CursorOrder::Desc => write!(f, "desc"),
        }
    }
}

impl FromStr for CursorOrder {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "asc" => Ok(CursorOrder::Asc),
            "desc" => Ok(CursorOrder::Desc),
            _ => Err(format!("Invalid CursorOrder: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaDataCursorResponse {
    pub next_cursor: Option<MetaData>,
    pub has_more: bool,
    pub total_count: u32,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataWithMetaData {
    pub meta: MetaData,
    #[serde_as(as = "Base64")]
    pub data: Vec<u8>,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataWithTimestamp {
    pub timestamp: u64,
    #[serde_as(as = "Base64")]
    pub data: Vec<u8>,
}
