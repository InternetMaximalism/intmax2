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
    utils::signature::Auth,
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
