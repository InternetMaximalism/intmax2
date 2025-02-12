use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
};

use super::interface::{DataType, SaveDataEntry};
use crate::{data::meta_data::MetaData, utils::signature::Signable};
use intmax2_zkp::ethereum_types::bytes32::Bytes32;
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};

// a prefix to make the content unique
fn content_prefix(path: &str) -> Vec<u8> {
    format!("intmax2/v1/store-vault-server/{}", path)
        .as_bytes()
        .to_vec()
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveUserDataRequest {
    #[serde_as(as = "Base64")]
    pub data: Vec<u8>,
    pub prev_digest: Option<Bytes32>,
}

impl Signable for SaveUserDataRequest {
    fn content(&self) -> Vec<u8> {
        [
            content_prefix("save_user_data"),
            bincode::serialize(&(self.data.clone(), self.prev_digest)).unwrap(),
        ]
        .concat()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUserDataRequest;

impl Signable for GetUserDataRequest {
    fn content(&self) -> Vec<u8> {
        content_prefix("get_user_data")
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUserDataResponse {
    #[serde_as(as = "Option<Base64>")]
    pub data: Option<Vec<u8>>,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSenderProofSetRequest {
    #[serde_as(as = "Base64")]
    pub data: Vec<u8>,
}

impl Signable for SaveSenderProofSetRequest {
    fn content(&self) -> Vec<u8> {
        [
            content_prefix("save_sender_proof_set"),
            bincode::serialize(&self.data).unwrap(),
        ]
        .concat()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSenderProofSetRequest;

impl Signable for GetSenderProofSetRequest {
    fn content(&self) -> Vec<u8> {
        content_prefix("get_sender_proof_set")
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSenderProofSetResponse {
    #[serde_as(as = "Base64")]
    pub data: Vec<u8>,
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
    pub uuids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDataBatchRequest {
    pub data_type: DataType,
    pub uuids: Vec<String>,
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
    pub data: Vec<DataWithMetaData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDataSequenceRequest {
    pub data_type: DataType,
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
pub struct SaveMiscRequest {
    #[serde_as(as = "Base64")]
    pub data: Vec<u8>,
    pub topic: Bytes32,
}

impl Signable for SaveMiscRequest {
    fn content(&self) -> Vec<u8> {
        [
            content_prefix("save_misc"),
            bincode::serialize(&self.data).unwrap(),
        ]
        .concat()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveMiscResponse {
    pub uuid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMiscSequenceRequest {
    pub topic: Bytes32,
    pub cursor: MetaDataCursor,
}

impl Signable for GetMiscSequenceRequest {
    fn content(&self) -> Vec<u8> {
        // to reuse the signature, we exclude cursor from the content intentionally
        [
            content_prefix("get_misc_sequence"),
            bincode::serialize(&self.topic.clone()).unwrap(),
        ]
        .concat()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMiscSequenceResponse {
    pub data: Vec<DataWithMetaData>,
    pub cursor_response: MetaDataCursorResponse,
}
