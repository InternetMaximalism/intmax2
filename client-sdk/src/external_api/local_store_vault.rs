use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use intmax2_interfaces::{
    api::{
        error::ServerError,
        store_vault_server::{
            interface::{SaveDataEntry, StoreVaultClientInterface},
            types::{DataWithMetaData, MetaDataCursor, MetaDataCursorResponse},
        },
    },
    utils::signature::Auth,
};
use intmax2_zkp::{common::signature::key_set::KeySet, ethereum_types::bytes32::Bytes32};

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
        todo!()
    }

    async fn get_snapshot(&self, key: KeySet, topic: &str) -> Result<Option<Vec<u8>>, ServerError> {
        todo!()
    }

    async fn save_data_batch(
        &self,
        key: KeySet,
        entries: &[SaveDataEntry],
    ) -> Result<Vec<Bytes32>, ServerError> {
        todo!()
    }

    async fn get_data_batch(
        &self,
        key: KeySet,
        topic: &str,
        digests: &[Bytes32],
    ) -> Result<Vec<DataWithMetaData>, ServerError> {
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
