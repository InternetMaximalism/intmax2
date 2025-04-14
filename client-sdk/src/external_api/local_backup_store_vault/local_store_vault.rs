use std::path::PathBuf;

use intmax2_interfaces::{
    api::store_vault_server::{
        interface::SaveDataEntry,
        types::{CursorOrder, DataWithMetaData, MetaDataCursor, MetaDataCursorResponse},
    },
    data::meta_data::MetaData,
    utils::digest::get_digest,
};
use intmax2_zkp::{common::signature_content::key_set::KeySet, ethereum_types::bytes32::Bytes32};

use super::{
    error::LocalStoreVaultError, local_data_client::LocalDataClient,
    metadata_client::MetaDataClient,
};

#[derive(Clone, Debug)]
pub struct LocalStoreVaultClient {
    pub data_client: LocalDataClient,
    pub metadata_client: MetaDataClient,
}

impl LocalStoreVaultClient {
    pub fn new(root_path: PathBuf) -> Self {
        LocalStoreVaultClient {
            data_client: LocalDataClient::new(root_path.clone()),
            metadata_client: MetaDataClient::new(root_path),
        }
    }
}

impl LocalStoreVaultClient {
    pub async fn save_snapshot(
        &self,
        key: KeySet,
        topic: &str,
        data: &[u8],
    ) -> Result<(), LocalStoreVaultError> {
        let digest = get_digest(data);
        self.data_client.write(topic, key.pubkey, digest, data)?;
        self.metadata_client.write(
            topic,
            key.pubkey,
            &[MetaData {
                timestamp: chrono::Utc::now().timestamp() as u64,
                digest,
            }],
        )?;
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
        if meta.len() > 1 {
            return Err(LocalStoreVaultError::DataInconsistencyError(
                "Multiple snapshots found".to_string(),
            ));
        }
        let digest = meta[0].digest;
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
        let _meta_result = match cursor.order {
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

        todo!()
    }
}
