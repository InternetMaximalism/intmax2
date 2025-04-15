use crate::env_var::EnvVar;
use intmax2_client_sdk::external_api::local_backup_store_vault::local_store_vault::LocalStoreVaultClient;
use intmax2_zkp::common::signature_content::key_set::KeySet;
use std::path::Path;
use uuid::Uuid;

use super::{
    client::{get_backup_root_path, get_client},
    error::CliError,
};

const BACKUP_CHUNK_SIZE: usize = 1000;

pub fn incorporate_backup(file_path: &Path) -> Result<(), CliError> {
    let env = envy::from_env::<EnvVar>()?;
    let root_path = get_backup_root_path(&env)?;
    let local_store_vault = LocalStoreVaultClient::new(root_path);
    local_store_vault.incorporate_diff(file_path)?;
    Ok(())
}

pub async fn make_history_backup(key: KeySet, dir: &Path, from: u64) -> Result<(), CliError> {
    let client = get_client()?;
    let csvs = client
        .make_history_backup(key, from, BACKUP_CHUNK_SIZE)
        .await?;
    for csv_str in csvs.iter() {
        let id = Uuid::new_v4().to_string()[..8].to_string();
        let file_path = dir.join(format!("backup_{}.csv", id));
        std::fs::write(file_path, csv_str)
            .map_err(|e| CliError::BackupError(format!("Failed to write file: {}", e)))?;
    }
    Ok(())
}
