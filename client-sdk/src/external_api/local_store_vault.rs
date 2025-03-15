use std::{fs, path::PathBuf, sync::Arc};

use async_trait::async_trait;
use intmax2_interfaces::{
    api::{
        error::ServerError,
        store_vault_server::{
            interface::{SaveDataEntry, StoreVaultClientInterface},
            types::{DataWithMetaData, MetaDataCursor, MetaDataCursorResponse},
        },
    },
    utils::{digest::get_digest, signature::Auth},
};
use intmax2_zkp::{
    common::signature::key_set::KeySet,
    ethereum_types::{bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait as _},
};

// get path for local object
pub fn get_path(topic: &str, pubkey: U256, digest: Bytes32) -> String {
    format!("{}/{}/{}", topic, pubkey.to_hex(), digest.to_hex())
}

#[derive(Clone)]
pub struct LocalStoreVaultClient {
    pub root_path: PathBuf,
    pub external_client: Option<Arc<Box<dyn StoreVaultClientInterface>>>,
}

impl LocalStoreVaultClient {
    pub fn new(
        root_path: PathBuf,
        external_client: Option<Arc<Box<dyn StoreVaultClientInterface>>>,
    ) -> Self {
        LocalStoreVaultClient {
            root_path,
            external_client,
        }
    }

    fn external_client(&self) -> Option<&Arc<Box<dyn StoreVaultClientInterface>>> {
        self.external_client.as_ref()
    }

    fn write(&self, topic: &str, data: &[u8]) -> Result<(), ServerError> {
        let file_path = self.root_path.join(topic);
        if !file_path.exists() {
            fs::create_dir_all(&file_path).map_err(|e| {
                ServerError::InternalError(format!("failed to create directory: {}", e))
            })?;
        }
        fs::write(file_path, data)
            .map_err(|e| ServerError::InternalError(format!("failed to write file: {}", e)))?;
        Ok(())
    }

    fn read(&self, topic: &str) -> Result<Option<Vec<u8>>, ServerError> {
        let file_path = self.root_path.join(topic);
        if !file_path.exists() {
            return Ok(None);
        }
        let data = fs::read(file_path)
            .map_err(|e| ServerError::InternalError(format!("failed to read file: {}", e)))?;
        Ok(Some(data))
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
        if let Some(client) = self.external_client() {
            return client.save_snapshot(key, topic, prev_digest, data).await;
        }
        self.write(topic, data)?;
        Ok(())
    }

    async fn get_snapshot(&self, key: KeySet, topic: &str) -> Result<Option<Vec<u8>>, ServerError> {
        if let Some(client) = self.external_client() {
            let data = client.get_snapshot(key, topic).await?;
            if let Some(data) = &data {
                // save the data to local store
                self.write(topic, data)?;
            }
            return Ok(data);
        } else {
            return self.read(topic);
        }
    }

    async fn save_data_batch(
        &self,
        key: KeySet,
        entries: &[SaveDataEntry],
    ) -> Result<Vec<Bytes32>, ServerError> {
        if let Some(client) = self.external_client() {
            return client.save_data_batch(key, entries).await;
        }
        let mut digests = Vec::new();
        for entry in entries {
            let digest = get_digest(&entry.data);
            let path = get_path(&entry.topic, key.pubkey, digest);
            self.write(&path, &entry.data)?;
            digests.push(digest);
        }
        Ok(digests)
    }

    async fn get_data_batch(
        &self,
        key: KeySet,
        topic: &str,
        digests: &[Bytes32],
    ) -> Result<Vec<DataWithMetaData>, ServerError> {
        if let Some(client) = self.external_client() {
            let data_with_meta = client.get_data_batch(key, topic, digests).await?;
            for data in &data_with_meta {
                let path = get_path(topic, key.pubkey, data.meta.digest);
                self.write(&path, &data.data)?;
            }
            return Ok(data_with_meta);
        }
        let mut data_with_meta = Vec::new();
        for digest in digests {
            let path = get_path(topic, key.pubkey, *digest);
            if let Some(data) = self.read(&path)? {
                data_with_meta.push(DataWithMetaData {
                    data,
                    meta: Default::default(),
                });
            }
        }
        todo!()
    }

    async fn get_data_sequence(
        &self,
        key: KeySet,
        topic: &str,
        cursor: &MetaDataCursor,
    ) -> Result<(Vec<DataWithMetaData>, MetaDataCursorResponse), ServerError> {
        todo!()
    }

    async fn get_data_sequence_with_auth(
        &self,
        topic: &str,
        cursor: &MetaDataCursor,
        auth: &Auth,
    ) -> Result<(Vec<DataWithMetaData>, MetaDataCursorResponse), ServerError> {
        todo!()
    }
}
