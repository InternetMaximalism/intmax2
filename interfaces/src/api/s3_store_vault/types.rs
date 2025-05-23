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
    format!("intmax2/v1/s3-store-vault/{path}",)
        .as_bytes()
        .to_vec()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct S3PreSaveSnapshotRequest {
    pub topic: String,
    pub pubkey: U256,
    pub digest: Bytes32,
}

impl Signable for S3PreSaveSnapshotRequest {
    fn content(&self) -> Vec<u8> {
        [
            content_prefix("pre_save_snapshot"),
            bincode::serialize(&(&self.topic, self.pubkey, self.digest)).unwrap(),
        ]
        .concat()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct S3PreSaveSnapshotResponse {
    pub presigned_url: String,
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
            bincode::serialize(&(&self.topic, self.pubkey, self.digest, self.prev_digest)).unwrap(),
        ]
        .concat()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct S3GetSnapshotRequest {
    pub pubkey: U256,
    pub topic: String,
}

impl Signable for S3GetSnapshotRequest {
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
pub struct S3GetSnapshotResponse {
    pub presigned_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct S3SaveDataBatchRequest {
    pub data: Vec<S3SaveDataEntry>,
}

impl Signable for S3SaveDataBatchRequest {
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
pub struct S3SaveDataBatchResponse {
    pub presigned_urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct S3GetDataBatchRequest {
    pub topic: String,
    pub pubkey: U256,
    pub digests: Vec<Bytes32>,
}

impl Signable for S3GetDataBatchRequest {
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
pub struct S3GetDataBatchResponse {
    pub presigned_urls_with_meta: Vec<PresignedUrlWithMetaData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct S3GetDataSequenceRequest {
    pub topic: String,
    pub pubkey: U256,
    pub cursor: MetaDataCursor,
}

impl Signable for S3GetDataSequenceRequest {
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
pub struct S3GetDataSequenceResponse {
    pub presigned_urls_with_meta: Vec<PresignedUrlWithMetaData>,
    pub cursor_response: MetaDataCursorResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresignedUrlWithMetaData {
    pub meta: MetaData,
    pub presigned_url: String,
}
