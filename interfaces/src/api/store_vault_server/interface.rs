use async_trait::async_trait;
use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u256::U256};
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};

use crate::{
    api::error::ServerError,
    utils::{key::PrivateKey, signature::Auth},
};

use super::types::{DataWithMetaData, MetaDataCursor, MetaDataCursorResponse};

pub const MAX_BATCH_SIZE: usize = 256;

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveDataEntry {
    pub topic: String,
    pub pubkey: U256,
    #[serde_as(as = "Base64")]
    pub data: Vec<u8>,
}

#[async_trait(?Send)]
pub trait StoreVaultClientInterface: Sync + Send {
    async fn save_snapshot(
        &self,
        view_key: PrivateKey,
        topic: &str,
        prev_digest: Option<Bytes32>,
        data: &[u8],
    ) -> Result<(), ServerError>;

    async fn get_snapshot(
        &self,
        view_key: PrivateKey,
        topic: &str,
    ) -> Result<Option<Vec<u8>>, ServerError>;

    async fn save_data_batch(
        &self,
        view_key: PrivateKey,
        entries: &[SaveDataEntry],
    ) -> Result<Vec<Bytes32>, ServerError>;

    async fn get_data_batch(
        &self,
        view_key: PrivateKey,
        topic: &str,
        digests: &[Bytes32],
    ) -> Result<Vec<DataWithMetaData>, ServerError>;

    async fn get_data_sequence(
        &self,
        view_key: PrivateKey,
        topic: &str,
        cursor: &MetaDataCursor,
    ) -> Result<(Vec<DataWithMetaData>, MetaDataCursorResponse), ServerError>;

    async fn get_data_sequence_with_auth(
        &self,
        topic: &str,
        cursor: &MetaDataCursor,
        auth: &Auth,
    ) -> Result<(Vec<DataWithMetaData>, MetaDataCursorResponse), ServerError>;
}
