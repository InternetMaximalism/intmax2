use super::{
    diff_data_client::DiffDataClient, error::LocalStoreVaultError,
    local_data_client::LocalDataClient, metadata_client::MetaDataClient,
};
use async_trait::async_trait;
use intmax2_interfaces::{
    api::{
        error::ServerError,
        store_vault_server::{
            interface::{SaveDataEntry, StoreVaultClientInterface},
            types::{CursorOrder, DataWithMetaData, MetaDataCursor, MetaDataCursorResponse},
        },
    },
    data::meta_data::MetaData,
    utils::{digest::get_digest, signature::Auth},
};
use intmax2_zkp::{
    common::signature_content::key_set::KeySet,
    ethereum_types::{bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait},
};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct LocalStoreVaultClient {
    pub data_client: LocalDataClient,
    pub metadata_client: MetaDataClient,
    pub diff_data_client: DiffDataClient,
}

impl LocalStoreVaultClient {
    pub fn new(root_path: PathBuf) -> Self {
        LocalStoreVaultClient {
            data_client: LocalDataClient::new(root_path.clone()),
            metadata_client: MetaDataClient::new(root_path),
            diff_data_client: DiffDataClient,
        }
    }
}

impl LocalStoreVaultClient {
    pub fn local_get_prev_snapshot_digest(
        &self,
        key: KeySet,
        topic: &str,
    ) -> Result<Option<Bytes32>, LocalStoreVaultError> {
        let meta = self.metadata_client.read(topic, key.pubkey)?;
        if meta.is_empty() {
            return Ok(None);
        }
        // get the latest metadata
        let meta = meta.iter().max_by_key(|m| m.timestamp).unwrap();
        Ok(Some(meta.digest))
    }

    pub fn local_save_snapshot(
        &self,
        pubkey: U256,
        topic: &str,
        data: &[u8],
        meta: &MetaData,
    ) -> Result<(), LocalStoreVaultError> {
        log::info!(
            "local_save_snapshot: topic: {}, pubkey: {}, digest: {}",
            topic,
            pubkey.to_hex(),
            meta.digest
        );
        self.data_client.write(topic, pubkey, meta.digest, data)?;
        self.metadata_client
            .append(topic, pubkey, &[meta.clone()])?;
        Ok(())
    }

    pub fn local_get_snapshot(
        &self,
        pubkey: U256,
        topic: &str,
    ) -> Result<Option<Vec<u8>>, LocalStoreVaultError> {
        let meta = self.metadata_client.read(topic, pubkey)?;
        if meta.is_empty() {
            return Ok(None);
        }
        // get the latest metadata
        let meta = meta.iter().max().unwrap();
        let digest = meta.digest;
        let data = self.data_client.read(topic, pubkey, digest)?;
        Ok(data)
    }

    pub fn local_save_data_batch(
        &self,
        entries_with_meta: &[(SaveDataEntry, MetaData)],
    ) -> Result<(), LocalStoreVaultError> {
        for (entry, meta) in entries_with_meta {
            log::info!(
                "local_save_data_batch: topic: {}, pubkey: {}, digest: {}",
                entry.topic,
                entry.pubkey.to_hex(),
                meta.digest
            );
            self.data_client
                .write(&entry.topic, entry.pubkey, meta.digest, &entry.data)?;
            self.metadata_client
                .append(&entry.topic, entry.pubkey, &[meta.clone()])?;
        }
        Ok(())
    }

    pub fn local_get_data_batch(
        &self,
        pubkey: U256,
        topic: &str,
        digests: &[Bytes32],
    ) -> Result<Vec<DataWithMetaData>, LocalStoreVaultError> {
        let mut data_with_meta = Vec::new();
        for digest in digests {
            let data = self
                .data_client
                .read(topic, pubkey, *digest)?
                .ok_or_else(|| {
                    LocalStoreVaultError::DataNotFoundError(format!(
                        "Data not found for topic: {}, pubkey: {}, digest: {}",
                        topic,
                        pubkey.to_hex(),
                        digest
                    ))
                })?;
            let meta = self
                .metadata_client
                .read(topic, pubkey)?
                .into_iter()
                .find(|m| m.digest == *digest)
                .ok_or_else(|| {
                    LocalStoreVaultError::DataNotFoundError(format!(
                        "MetaData not found for topic: {}, pubkey: {}, digest: {}",
                        topic,
                        pubkey.to_hex(),
                        digest
                    ))
                })?;
            data_with_meta.push(DataWithMetaData { data, meta });
        }
        Ok(data_with_meta)
    }

    pub fn local_get_data_sequence(
        &self,
        pubkey: U256,
        topic: &str,
        cursor: &MetaDataCursor,
    ) -> Result<(Vec<DataWithMetaData>, MetaDataCursorResponse), LocalStoreVaultError> {
        // get metadata list
        let meta = self.metadata_client.read(topic, pubkey)?;
        if meta.is_empty() {
            return Ok((Vec::new(), MetaDataCursorResponse::default()));
        }
        let mut metadata = match cursor.order {
            CursorOrder::Asc => {
                let cursor_meta = cursor.cursor.clone().unwrap_or_default();
                meta.iter()
                    .filter(|m| m > &&cursor_meta)
                    .cloned()
                    .collect::<Vec<_>>()
            }
            CursorOrder::Desc => {
                let cursor_meta = cursor.cursor.clone().unwrap_or(MetaData {
                    timestamp: i64::MAX as u64,
                    digest: Bytes32::default(),
                });
                meta.iter()
                    .filter(|m| m < &&cursor_meta)
                    .cloned()
                    .collect::<Vec<_>>()
            }
        };

        // sort metadata
        let metadata = match cursor.order {
            CursorOrder::Asc => {
                metadata.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
                metadata
            }
            CursorOrder::Desc => {
                metadata.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
                metadata
            }
        };

        let mut data_with_meta = Vec::new();
        for meta in &metadata {
            let data = self
                .data_client
                .read(topic, pubkey, meta.digest)?
                .ok_or_else(|| {
                    LocalStoreVaultError::DataNotFoundError(format!(
                        "Data not found for topic: {}, pubkey: {}, digest: {}",
                        topic, pubkey, meta.digest
                    ))
                })?;
            data_with_meta.push(DataWithMetaData {
                data,
                meta: meta.clone(),
            });
        }
        let next_cursor = MetaDataCursorResponse {
            next_cursor: None,
            has_more: false,
            total_count: metadata.len() as u32,
        };
        Ok((data_with_meta, next_cursor))
    }

    pub fn incorporate_diff(&self, diff_file_path: &Path) -> Result<(), LocalStoreVaultError> {
        let records = self.diff_data_client.read(diff_file_path)?;
        log::info!(
            "Incorporating diff file: {} with {} records",
            diff_file_path.display(),
            records.len()
        );
        for record in records {
            self.data_client.write(
                &record.topic,
                record.pubkey.into(),
                record.digest,
                &record.data,
            )?;
            self.metadata_client.append(
                &record.topic,
                record.pubkey.into(),
                &[MetaData {
                    timestamp: record.timestamp,
                    digest: record.digest,
                }],
            )?;
        }
        Ok(())
    }

    pub fn delete_all(&self, topic: &str, pubkey: U256) -> Result<(), LocalStoreVaultError> {
        // metadata is also deleted because the directory is the same
        self.data_client.delete_all(topic, pubkey)?;
        Ok(())
    }
}

impl From<LocalStoreVaultError> for ServerError {
    fn from(error: LocalStoreVaultError) -> Self {
        ServerError::InternalError(format!("LocalStoreVaultClient error: {}", error))
    }
}

#[async_trait(?Send)]
impl StoreVaultClientInterface for LocalStoreVaultClient {
    async fn save_snapshot(
        &self,
        key: KeySet,
        topic: &str,
        prev_digest: Option<Bytes32>,
        data: &[u8],
    ) -> Result<(), ServerError> {
        let stored_prev_digest = self.local_get_prev_snapshot_digest(key, topic)?;
        if stored_prev_digest != prev_digest {
            return Err(LocalStoreVaultError::LockError(format!(
                "prev_digest mismatch with stored digest: {:?}",
                stored_prev_digest
            ))
            .into());
        }
        let meta = MetaData {
            timestamp: chrono::Utc::now().timestamp() as u64,
            digest: get_digest(data),
        };
        self.local_save_snapshot(key.pubkey, topic, data, &meta)?;
        Ok(())
    }

    async fn get_snapshot(&self, key: KeySet, topic: &str) -> Result<Option<Vec<u8>>, ServerError> {
        let data = self.local_get_snapshot(key.pubkey, topic)?;
        Ok(data)
    }

    async fn save_data_batch(
        &self,
        _key: KeySet,
        entries: &[SaveDataEntry],
    ) -> Result<Vec<Bytes32>, ServerError> {
        let mut entries_with_meta = Vec::new();
        for entry in entries {
            let meta = MetaData {
                timestamp: chrono::Utc::now().timestamp() as u64,
                digest: get_digest(entry.data.as_slice()),
            };
            entries_with_meta.push((entry.clone(), meta));
        }
        self.local_save_data_batch(&entries_with_meta)?;
        Ok(entries_with_meta
            .iter()
            .map(|(_, meta)| meta.digest)
            .collect())
    }

    async fn get_data_batch(
        &self,
        key: KeySet,
        topic: &str,
        digests: &[Bytes32],
    ) -> Result<Vec<DataWithMetaData>, ServerError> {
        let data_with_meta = self.local_get_data_batch(key.pubkey, topic, digests)?;
        Ok(data_with_meta)
    }

    async fn get_data_sequence(
        &self,
        key: KeySet,
        topic: &str,
        cursor: &MetaDataCursor,
    ) -> Result<(Vec<DataWithMetaData>, MetaDataCursorResponse), ServerError> {
        let (data_with_meta, cursor_response) =
            self.local_get_data_sequence(key.pubkey, topic, cursor)?;
        Ok((data_with_meta, cursor_response))
    }

    async fn get_data_sequence_with_auth(
        &self,
        topic: &str,
        cursor: &MetaDataCursor,
        auth: &Auth,
    ) -> Result<(Vec<DataWithMetaData>, MetaDataCursorResponse), ServerError> {
        let (data_with_meta, cursor_response) =
            self.local_get_data_sequence(auth.pubkey, topic, cursor)?;
        Ok((data_with_meta, cursor_response))
    }
}
