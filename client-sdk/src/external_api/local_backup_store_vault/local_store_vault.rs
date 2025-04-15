use super::{
    diff_data_client::DiffDataClient, error::LocalStoreVaultError,
    local_data_client::LocalDataClient, metadata_client::MetaDataClient,
};
use intmax2_interfaces::{
    api::store_vault_server::{
        interface::SaveDataEntry,
        types::{CursorOrder, DataWithMetaData, MetaDataCursor, MetaDataCursorResponse},
    },
    data::meta_data::MetaData,
};
use intmax2_zkp::{
    common::signature_content::key_set::KeySet,
    ethereum_types::{bytes32::Bytes32, u256::U256},
};
use std::path::PathBuf;

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
    pub async fn save_snapshot(
        &self,
        key: KeySet,
        topic: &str,
        data: &[u8],
        meta: &MetaData,
    ) -> Result<(), LocalStoreVaultError> {
        self.data_client
            .write(topic, key.pubkey, meta.digest, data)?;
        self.metadata_client
            .append(topic, key.pubkey, &[meta.clone()])?;
        Ok(())
    }

    pub async fn get_snapshot(
        &self,
        key: KeySet,
        topic: &str,
    ) -> Result<Option<Vec<u8>>, LocalStoreVaultError> {
        let meta = self.metadata_client.read(topic, key.pubkey)?;
        if meta.is_empty() {
            return Ok(None);
        }
        // get the latest metadata
        let meta = meta.iter().max_by_key(|m| m.timestamp).unwrap();
        let digest = meta.digest;
        let data = self.data_client.read(topic, key.pubkey, digest)?;
        Ok(data)
    }

    pub async fn save_data_batch(
        &self,
        key: KeySet,
        entries_with_meta: &[(SaveDataEntry, MetaData)],
    ) -> Result<(), LocalStoreVaultError> {
        for (entry, meta) in entries_with_meta {
            self.data_client
                .write(&entry.topic, key.pubkey, meta.digest, &entry.data)?;
            self.metadata_client
                .append(&entry.topic, key.pubkey, &[meta.clone()])?;
        }
        Ok(())
    }

    pub async fn get_data_batch(
        &self,
        key: KeySet,
        topic: &str,
        digests: &[Bytes32],
    ) -> Result<Vec<DataWithMetaData>, LocalStoreVaultError> {
        let mut data_with_meta = Vec::new();
        for digest in digests {
            let data = self
                .data_client
                .read(topic, key.pubkey, *digest)?
                .ok_or_else(|| {
                    LocalStoreVaultError::DataNotFoundError(format!(
                        "Data not found for topic: {}, pubkey: {}, digest: {}",
                        topic, key.pubkey, digest
                    ))
                })?;
            let meta = self
                .metadata_client
                .read(topic, key.pubkey)?
                .into_iter()
                .find(|m| m.digest == *digest)
                .ok_or_else(|| {
                    LocalStoreVaultError::DataNotFoundError(format!(
                        "MetaData not found for topic: {}, pubkey: {}, digest: {}",
                        topic, key.pubkey, digest
                    ))
                })?;
            data_with_meta.push(DataWithMetaData { data, meta });
        }
        Ok(data_with_meta)
    }

    pub async fn get_data_sequence(
        &self,
        key: KeySet,
        topic: &str,
        cursor: &MetaDataCursor,
    ) -> Result<(Vec<DataWithMetaData>, MetaDataCursorResponse), LocalStoreVaultError> {
        // get metadata list
        let meta = self.metadata_client.read(topic, key.pubkey)?;
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
                .read(topic, key.pubkey, meta.digest)?
                .ok_or_else(|| {
                    LocalStoreVaultError::DataNotFoundError(format!(
                        "Data not found for topic: {}, pubkey: {}, digest: {}",
                        topic, key.pubkey, meta.digest
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

    pub fn load_diff(&self, diff_file_path: PathBuf) -> Result<(), LocalStoreVaultError> {
        let records = self.diff_data_client.read(diff_file_path)?;
        for record in records {
            let topic = record.topic.clone();
            let pubkey = record.pubkey;
            let digest = record.digest;
            let data = record.data;
            self.data_client
                .write(&topic, pubkey.into(), digest, &data)?;
        }
        Ok(())
    }

    pub fn delete_all(&self, topic: &str, pubkey: U256) -> Result<(), LocalStoreVaultError> {
        // metadata is also deleted because the directory is the same
        self.data_client.delete_all(topic, pubkey)?;
        Ok(())
    }
}
