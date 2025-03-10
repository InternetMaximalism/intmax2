use async_trait::async_trait;
use intmax2_interfaces::{
    api::{
        error::ServerError,
        s3_store_vault::types::{
            S3GetDataBatchRequest, S3GetDataBatchResponse, S3GetDataSequenceRequest,
            S3GetDataSequenceResponse, S3GetSnapshotRequest, S3GetSnapshotResponse,
            S3SaveDataBatchRequest, S3SaveDataBatchResponse, S3SaveDataEntry,
            S3SaveSnapshotRequest, S3SaveSnapshotResponse,
        },
        store_vault_server::{
            interface::{SaveDataEntry, StoreVaultClientInterface, MAX_BATCH_SIZE},
            types::{CursorOrder, DataWithMetaData, MetaDataCursor, MetaDataCursorResponse},
        },
    },
    utils::{
        digest::get_digest,
        signature::{Auth, Signable, WithAuth},
    },
};
use intmax2_zkp::{common::signature::key_set::KeySet, ethereum_types::bytes32::Bytes32};

use super::utils::{query::post_request, retry::with_retry};

const TIME_TO_EXPIRY: u64 = 60; // 1 minute for normal requests
const TIME_TO_EXPIRY_READONLY: u64 = 60 * 60 * 24; // 24 hours for readonly

#[derive(Debug, Clone)]
pub struct S3StoreVaultClient {
    base_url: String,
}

impl S3StoreVaultClient {
    pub fn new(base_url: &str) -> Self {
        S3StoreVaultClient {
            base_url: base_url.to_string(),
        }
    }
}

#[async_trait(?Send)]
impl StoreVaultClientInterface for S3StoreVaultClient {
    async fn save_snapshot(
        &self,
        key: KeySet,
        topic: &str,
        prev_digest: Option<Bytes32>,
        data: &[u8],
    ) -> Result<(), ServerError> {
        let digest = get_digest(data);
        let request = S3SaveSnapshotRequest {
            pubkey: key.pubkey,
            topic: topic.to_string(),
            prev_digest,
            digest,
        };
        let request_with_auth = request.sign(key, TIME_TO_EXPIRY);
        let response: S3SaveSnapshotResponse = post_request(
            &self.base_url,
            "/s3-store-vault/save-snapshot",
            Some(&request_with_auth),
        )
        .await?;

        // upload data to s3
        self.upload(&response.presigned_url, data).await?;

        Ok(())
    }

    async fn get_snapshot(&self, key: KeySet, topic: &str) -> Result<Option<Vec<u8>>, ServerError> {
        let request = S3GetSnapshotRequest {
            topic: topic.to_string(),
            pubkey: key.pubkey,
        };
        let request_with_auth = request.sign(key, TIME_TO_EXPIRY);
        let response: S3GetSnapshotResponse = post_request(
            &self.base_url,
            "/s3-store-vault/get-snapshot",
            Some(&request_with_auth),
        )
        .await?;

        match response.presigned_url {
            Some(url) => {
                let data = self.download(&url).await?;
                Ok(Some(data))
            }
            None => Ok(None),
        }
    }

    async fn save_data_batch(
        &self,
        key: KeySet,
        entries: &[SaveDataEntry],
    ) -> Result<Vec<Bytes32>, ServerError> {
        let mut all_digests = vec![];

        for chunk in entries.chunks(MAX_BATCH_SIZE) {
            let data = chunk
                .iter()
                .map(|entry| S3SaveDataEntry {
                    topic: entry.topic.clone(),
                    pubkey: entry.pubkey,
                    digest: get_digest(&entry.data),
                })
                .collect::<Vec<_>>();
            let digests = data.iter().map(|entry| entry.digest).collect::<Vec<_>>();
            let request = S3SaveDataBatchRequest { data };
            let request_with_auth = request.sign(key, TIME_TO_EXPIRY);
            let response: S3SaveDataBatchResponse = post_request(
                &self.base_url,
                "/s3-store-vault/save-data-batch",
                Some(&request_with_auth),
            )
            .await?;

            for (url, entry) in response.presigned_urls.iter().zip(chunk.iter()) {
                self.upload(url, &entry.data).await?;
            }

            all_digests.extend(digests);
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
            let request = S3GetDataBatchRequest {
                topic: topic.to_string(),
                digests: chunk.to_vec(),
                pubkey: key.pubkey,
            };
            let request_with_auth = request.sign(key, TIME_TO_EXPIRY);
            let response: S3GetDataBatchResponse = post_request(
                &self.base_url,
                "/s3-store-vault/get-data-batch",
                Some(&request_with_auth),
            )
            .await?;
            let mut data_with_meta = Vec::new();
            for url_with_meta in response.presigned_urls_with_meta.iter() {
                let data = self.download(&url_with_meta.presigned_url).await?;
                data_with_meta.push(DataWithMetaData {
                    data,
                    meta: url_with_meta.meta.clone(),
                });
            }
            all_data.extend(data_with_meta);
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
            inner: S3GetDataSequenceRequest {
                topic: topic.to_string(),
                pubkey: auth.pubkey,
                cursor: cursor.clone(),
            },
            auth: auth.clone(),
        };
        let response: S3GetDataSequenceResponse = post_request(
            &self.base_url,
            "/s3-store-vault/get-data-sequence",
            Some(&request_with_auth),
        )
        .await?;

        let mut data_with_meta = Vec::new();
        for url_with_meta in response.presigned_urls_with_meta.iter() {
            let data = self.download(&url_with_meta.presigned_url).await?;
            data_with_meta.push(DataWithMetaData {
                data,
                meta: url_with_meta.meta.clone(),
            });
        }

        Ok((data_with_meta, response.cursor_response))
    }
}

impl S3StoreVaultClient {
    fn verify_auth_for_get_data_sequence(&self, auth: &Auth) -> anyhow::Result<()> {
        let dummy_request = S3GetDataSequenceRequest {
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

    async fn upload(&self, url: &str, data: &[u8]) -> Result<(), ServerError> {
        let client = reqwest::Client::new();
        let response = with_retry(|| async { client.put(url).body(data.to_vec()).send().await })
            .await
            .map_err(|e| ServerError::NetworkError(e.to_string()))?;
        if !response.status().is_success() {
            return Err(ServerError::InvalidResponse(format!(
                "Failed to upload data: {:?}",
                response.text().await
            )));
        }
        Ok(())
    }

    async fn download(&self, url: &str) -> Result<Vec<u8>, ServerError> {
        let client = reqwest::Client::new();
        let response = with_retry(|| async { client.get(url).send().await })
            .await
            .map_err(|e| ServerError::NetworkError(e.to_string()))?;
        if !response.status().is_success() {
            return Err(ServerError::InvalidResponse(format!(
                "Failed to download data: {:?}",
                response.text().await
            )));
        }
        let response = response.bytes().await.map_err(|e| {
            ServerError::InvalidResponse(format!("Failed to read response: {:?}", e))
        })?;
        Ok(response.to_vec())
    }
}

pub fn generate_auth_for_get_data_sequence(key: KeySet) -> Auth {
    // because auth is not dependent on the datatype and cursor, we can use a dummy request
    let dummy_request = S3GetDataSequenceRequest {
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
