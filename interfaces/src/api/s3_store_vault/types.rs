use crate::{
    api::store_vault_server::types::{MetaDataCursor, MetaDataCursorResponse},
    data::meta_data::MetaData,
    utils::signature::Signable,
};
use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u256::U256};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct S3SaveDataEntry {
    pub topic: String,
    pub pubkey: U256,
    pub digest: Bytes32,
}

// a prefix to make the content unique
fn content_prefix(path: &str) -> Vec<u8> {
    format!("intmax2/v1/s3-store-vault/{}", path)
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
pub struct GetS3SnapshotRequest {
    pub pubkey: U256,
    pub topic: String,
}

impl Signable for GetS3SnapshotRequest {
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
pub struct GetS3SnapshotResponse {
    pub presigned_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveS3DataBatchRequest {
    pub data: Vec<S3SaveDataEntry>,
}

impl Signable for SaveS3DataBatchRequest {
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
pub struct SaveS3DataBatchResponse {
    pub presigned_urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetS3DataBatchRequest {
    pub topic: String,
    pub pubkey: U256,
    pub digests: Vec<Bytes32>,
}

impl Signable for GetS3DataBatchRequest {
    fn content(&self) -> Vec<u8> {
        // to reuse the signature, we exclude data_type and uuids from the content intentionally
        content_prefix("get_data_batch")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetS3DataBatchResponse {
    pub presigned_urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetS3DataSequenceRequest {
    pub topic: String,
    pub pubkey: U256,
    pub cursor: MetaDataCursor,
}

impl Signable for GetS3DataSequenceRequest {
    fn content(&self) -> Vec<u8> {
        // to reuse the signature, we exclude data_type and cursor from the content intentionally
        content_prefix("get_data_sequence")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetS3DataSequenceResponse {
    pub data: Vec<PresignedUrlWithMetaData>,
    pub cursor_response: MetaDataCursorResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresignedUrlWithMetaData {
    pub meta: MetaData,
    pub presigned_url: String,
}
