use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
};

use super::interface::SaveDataEntry;
use crate::{data::meta_data::MetaData, utils::signature::Signable};
use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u256::U256};
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StoreVaultType {
    Local,
    LegacyRemote,
    Remote,
    RemoteWithBackup,
    LegacyRemoteWithBackup,
}

// a prefix to make the content unique
fn content_prefix(path: &str) -> Vec<u8> {
    format!("intmax2/v1/store-vault-server/{path}",)
        .as_bytes()
        .to_vec()
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSnapshotRequest {
    pub topic: String,
    pub pubkey: U256,
    pub prev_digest: Option<Bytes32>,
    #[serde_as(as = "Base64")]
    pub data: Vec<u8>,
}

impl Signable for SaveSnapshotRequest {
    fn content(&self) -> Vec<u8> {
        [
            content_prefix("save_snapshot"),
            bincode::serialize(&(self.data.clone(), self.pubkey, self.prev_digest)).unwrap(),
        ]
        .concat()
    }
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

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSnapshotResponse {
    #[serde_as(as = "Option<Base64>")]
    pub data: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveDataBatchRequest {
    pub data: Vec<SaveDataEntry>,
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
    pub digests: Vec<Bytes32>,
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
        // to reuse the signature, we exclude topic and digests from the content intentionally
        [
            content_prefix("get_data_batch"),
            bincode::serialize(&self.pubkey).unwrap(),
        ]
        .concat()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDataBatchResponse {
    pub data: Vec<DataWithMetaData>,
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
        // to reuse the signature, we exclude topic and cursor from the content intentionally
        [
            content_prefix("get_data_sequence"),
            bincode::serialize(&self.pubkey).unwrap(),
        ]
        .concat()
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
            _ => Err(format!("Invalid CursorOrder: {s}",)),
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
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
