use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use intmax2_interfaces::{
    api::{
        error::ServerError,
        store_vault_server::{
            interface::{SaveDataEntry, StoreVaultClientInterface},
            types::{DataWithMetaData, MetaDataCursor, MetaDataCursorResponse},
        },
    },
    data::meta_data::MetaData,
    utils::{digest::get_digest, signature::Auth},
};
use intmax2_zkp::{common::signature_content::key_set::KeySet, ethereum_types::bytes32::Bytes32};
use local_store_vault::LocalStoreVaultClient;

pub mod diff_data_client;
pub mod error;
pub mod local_data_client;
pub mod local_store_vault;
pub mod metadata_client;

#[derive(Clone)]
pub struct LocalBackupStoreVaultClient {
    pub store_vault: Arc<Box<dyn StoreVaultClientInterface>>,
    pub local_store_vault: LocalStoreVaultClient,
}

impl LocalBackupStoreVaultClient {
    pub fn new(store_vault: Arc<Box<dyn StoreVaultClientInterface>>, root_path: PathBuf) -> Self {
        LocalBackupStoreVaultClient {
            store_vault,
            local_store_vault: LocalStoreVaultClient::new(root_path),
        }
    }
}

#[async_trait(?Send)]
impl StoreVaultClientInterface for LocalBackupStoreVaultClient {
    async fn save_snapshot(
        &self,
        key: KeySet,
        topic: &str,
        prev_digest: Option<Bytes32>,
        data: &[u8],
    ) -> Result<(), ServerError> {
        log::info!("save_snapshot");
        self.store_vault
            .save_snapshot(key, topic, prev_digest, data)
            .await?;
        let meta = MetaData {
            timestamp: chrono::Utc::now().timestamp() as u64,
            digest: get_digest(data),
        };
        self.local_store_vault
            .local_save_snapshot(key.pubkey, topic, data, &meta)?;
        Ok(())
    }

    async fn get_snapshot(&self, key: KeySet, topic: &str) -> Result<Option<Vec<u8>>, ServerError> {
        let data = self.store_vault.get_snapshot(key, topic).await?;
        if let Some(data) = &data {
            let digest = get_digest(data);
            let local_prev_digest = self
                .local_store_vault
                .local_get_prev_snapshot_digest(key, topic)?;
            if local_prev_digest != Some(digest) {
                // save the data to local store vault
                let meta = MetaData {
                    timestamp: chrono::Utc::now().timestamp() as u64,
                    digest,
                };
                self.local_store_vault
                    .local_save_snapshot(key.pubkey, topic, data, &meta)?;
            }
        }
        Ok(data)
    }

    async fn save_data_batch(
        &self,
        key: KeySet,
        entries: &[SaveDataEntry],
    ) -> Result<Vec<Bytes32>, ServerError> {
        let digests = self.store_vault.save_data_batch(key, entries).await?;
        let mut entries_with_meta = Vec::new();
        for (digest, entries) in digests.iter().zip(entries.iter()) {
            let meta = MetaData {
                timestamp: chrono::Utc::now().timestamp() as u64,
                digest: *digest,
            };
            entries_with_meta.push((entries.clone(), meta));
        }
        self.local_store_vault
            .local_save_data_batch(&entries_with_meta)?;
        Ok(digests)
    }

    async fn get_data_batch(
        &self,
        key: KeySet,
        topic: &str,
        digests: &[Bytes32],
    ) -> Result<Vec<DataWithMetaData>, ServerError> {
        let data_with_meta = self.store_vault.get_data_batch(key, topic, digests).await?;
        let mut entries_with_meta = Vec::new();
        for DataWithMetaData { meta, data } in &data_with_meta {
            entries_with_meta.push((
                SaveDataEntry {
                    topic: topic.to_string(),
                    pubkey: key.pubkey,
                    data: data.clone(),
                },
                MetaData {
                    timestamp: meta.timestamp,
                    digest: meta.digest,
                },
            ));
        }
        self.local_store_vault
            .local_save_data_batch(&entries_with_meta)?;
        Ok(data_with_meta)
    }

    async fn get_data_sequence(
        &self,
        key: KeySet,
        topic: &str,
        cursor: &MetaDataCursor,
    ) -> Result<(Vec<DataWithMetaData>, MetaDataCursorResponse), ServerError> {
        let (data_with_meta, next_cursor) = self
            .store_vault
            .get_data_sequence(key, topic, cursor)
            .await?;
        let mut entries_with_meta = Vec::new();
        for DataWithMetaData { meta, data } in &data_with_meta {
            entries_with_meta.push((
                SaveDataEntry {
                    topic: topic.to_string(),
                    pubkey: key.pubkey,
                    data: data.clone(),
                },
                MetaData {
                    timestamp: meta.timestamp,
                    digest: meta.digest,
                },
            ));
        }
        self.local_store_vault
            .local_save_data_batch(&entries_with_meta)?;
        Ok((data_with_meta, next_cursor))
    }

    async fn get_data_sequence_with_auth(
        &self,
        topic: &str,
        cursor: &MetaDataCursor,
        auth: &Auth,
    ) -> Result<(Vec<DataWithMetaData>, MetaDataCursorResponse), ServerError> {
        let (data_with_meta, next_cursor) = self
            .store_vault
            .get_data_sequence_with_auth(topic, cursor, auth)
            .await?;
        let mut entries_with_meta = Vec::new();
        for DataWithMetaData { meta, data } in &data_with_meta {
            entries_with_meta.push((
                SaveDataEntry {
                    topic: topic.to_string(),
                    pubkey: auth.pubkey,
                    data: data.clone(),
                },
                MetaData {
                    timestamp: meta.timestamp,
                    digest: meta.digest,
                },
            ));
        }
        self.local_store_vault
            .local_save_data_batch(&entries_with_meta)?;
        Ok((data_with_meta, next_cursor))
    }
}
