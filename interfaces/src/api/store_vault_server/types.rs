use super::interface::{DataType, SaveDataEntry};
use crate::{data::meta_data::MetaData, utils::signature::Signable};
use intmax2_zkp::ethereum_types::bytes32::Bytes32;
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};

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
        bincode::serialize(&(self.data.clone(), self.prev_digest)).unwrap()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUserDataRequest;

impl Signable for GetUserDataRequest {
    fn content(&self) -> Vec<u8> {
        vec![]
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
        bincode::serialize(&self.data).unwrap()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSenderProofSetRequest;

impl Signable for GetSenderProofSetRequest {
    fn content(&self) -> Vec<u8> {
        vec![]
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
        bincode::serialize(&self.data).unwrap()
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
        bincode::serialize(&(self.data_type, self.uuids.clone())).unwrap()
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
        bincode::serialize(&(self.data_type, self.cursor.clone())).unwrap()
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

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub enum CursorOrder {
    #[default]
    Asc,
    Desc,
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
