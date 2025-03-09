use async_trait::async_trait;
use intmax2_interfaces::{
    api::{
        error::ServerError,
        store_vault_server::{
            interface::{SaveDataEntry, StoreVaultClientInterface, MAX_BATCH_SIZE},
            types::{
                CursorOrder, DataWithMetaData, GetDataBatchRequest, GetDataBatchResponse,
                GetDataSequenceRequest, GetDataSequenceResponse, GetSnapshotRequest,
                GetSnapshotResponse, MetaDataCursor, MetaDataCursorResponse, SaveDataBatchRequest,
                SaveDataBatchResponse, SaveSnapshotRequest,
            },
        },
    },
    utils::signature::{Auth, Signable, WithAuth},
};
use intmax2_zkp::{common::signature::key_set::KeySet, ethereum_types::bytes32::Bytes32};

use super::utils::query::post_request;

const TIME_TO_EXPIRY: u64 = 60; // 1 minute for normal requests
const TIME_TO_EXPIRY_READONLY: u64 = 60 * 60 * 24; // 24 hours for readonly

#[derive(Debug, Clone)]
pub struct StoreVaultServerClient {
    base_url: String,
}

impl StoreVaultServerClient {
    pub fn new(base_url: &str) -> Self {
        StoreVaultServerClient {
            base_url: base_url.to_string(),
        }
    }
}

#[async_trait(?Send)]
impl StoreVaultClientInterface for StoreVaultServerClient {
    async fn save_snapshot(
        &self,
        key: KeySet,
        topic: &str,
        prev_digest: Option<Bytes32>,
        data: &[u8],
    ) -> Result<(), ServerError> {
        let request = SaveSnapshotRequest {
            data: data.to_vec(),
            pubkey: key.pubkey,
            topic: topic.to_string(),
            prev_digest,
        };
        let request_with_auth = request.sign(key, TIME_TO_EXPIRY);
        post_request::<_, ()>(
            &self.base_url,
            "/store-vault-server/save-snapshot",
            Some(&request_with_auth),
        )
        .await?;
        Ok(())
    }

    async fn get_snapshot(&self, key: KeySet, topic: &str) -> Result<Option<Vec<u8>>, ServerError> {
        let request = GetSnapshotRequest {
            topic: topic.to_string(),
            pubkey: key.pubkey,
        };
        let request_with_auth = request.sign(key, TIME_TO_EXPIRY);
        let response: GetSnapshotResponse = post_request(
            &self.base_url,
            "/store-vault-server/get-snapshot",
            Some(&request_with_auth),
        )
        .await?;
        Ok(response.data)
    }

    async fn save_data_batch(
        &self,
        key: KeySet,
        entries: &[SaveDataEntry],
    ) -> Result<Vec<Bytes32>, ServerError> {
        let mut all_digests = vec![];

        for chunk in entries.chunks(MAX_BATCH_SIZE) {
            let request = SaveDataBatchRequest {
                data: chunk.to_vec(),
            };
            let request_with_auth = request.sign(key, TIME_TO_EXPIRY);
            let response: SaveDataBatchResponse = post_request(
                &self.base_url,
                "/store-vault-server/save-data-batch",
                Some(&request_with_auth),
            )
            .await?;
            all_digests.extend(response.digests);
        }
        Ok(all_digests)
    }

    async fn get_data_batch(
        &self,
        key: KeySet,
        topic: &str,
        digests: &[Bytes32],
    ) -> Result<Vec<DataWithMetaData>, ServerError> {
        let mut all_data = vec![];
        for chunk in digests.chunks(MAX_BATCH_SIZE) {
            let request = GetDataBatchRequest {
                topic: topic.to_string(),
                digests: chunk.to_vec(),
                pubkey: key.pubkey,
            };
            let request_with_auth = request.sign(key, TIME_TO_EXPIRY);
            let response: GetDataBatchResponse = post_request(
                &self.base_url,
                "/store-vault-server/get-data-batch",
                Some(&request_with_auth),
            )
            .await?;
            all_data.extend(response.data);
        }
        Ok(all_data)
    }

    async fn get_data_sequence(
        &self,
        key: KeySet,
        topic: &str,
        cursor: &MetaDataCursor,
    ) -> Result<(Vec<DataWithMetaData>, MetaDataCursorResponse), ServerError> {
        let auth = generate_auth_for_get_data_sequence(key);
        let (data, cursor) = self
            .get_data_sequence_with_auth(topic, cursor, &auth)
            .await?;
        Ok((data, cursor))
    }

    async fn get_data_sequence_with_auth(
        &self,
        topic: &str,
        cursor: &MetaDataCursor,
        auth: &Auth,
    ) -> Result<(Vec<DataWithMetaData>, MetaDataCursorResponse), ServerError> {
        if let Some(limit) = cursor.limit {
            if limit > MAX_BATCH_SIZE as u32 {
                return Err(ServerError::InvalidRequest(
                    "Limit exceeds max batch size".to_string(),
                ));
            }
        }
        self.verify_auth_for_get_data_sequence(auth)
            .map_err(|e| ServerError::InvalidAuth(e.to_string()))?;
        let request_with_auth = WithAuth {
            inner: GetDataSequenceRequest {
                topic: topic.to_string(),
                pubkey: auth.pubkey,
                cursor: cursor.clone(),
            },
            auth: auth.clone(),
        };
        let response: GetDataSequenceResponse = post_request(
            &self.base_url,
            "/store-vault-server/get-data-sequence",
            Some(&request_with_auth),
        )
        .await?;
        Ok((response.data, response.cursor_response))
    }
}

impl StoreVaultServerClient {
    fn verify_auth_for_get_data_sequence(&self, auth: &Auth) -> anyhow::Result<()> {
        let dummy_request = GetDataSequenceRequest {
            topic: "dummy".to_string(),
            pubkey: auth.pubkey,
            cursor: MetaDataCursor {
                cursor: None,
                order: CursorOrder::Asc,
                limit: None,
            },
        };
        dummy_request.verify(auth)
    }
}

pub fn generate_auth_for_get_data_sequence(key: KeySet) -> Auth {
    // because auth is not dependent on the datatype and cursor, we can use a dummy request
    let dummy_request = GetDataSequenceRequest {
        topic: "dummy".to_string(),
        pubkey: key.pubkey,
        cursor: MetaDataCursor {
            cursor: None,
            order: CursorOrder::Asc,
            limit: None,
        },
    };
    let dummy_request_with_auth = dummy_request.sign(key, TIME_TO_EXPIRY_READONLY);
    dummy_request_with_auth.auth
}
