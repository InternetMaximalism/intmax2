use crate::env_var::EnvVar;
use intmax2_client_sdk::external_api::local_backup_store_vault::local_store_vault::LocalStoreVaultClient;
use std::path::Path;

use super::{client::get_backup_root_path, error::CliError};

pub fn incorporate_backup(file_path: &Path) -> Result<(), CliError> {
    let env = envy::from_env::<EnvVar>()?;
    let root_path = get_backup_root_path(&env)?;
    let local_store_vault = LocalStoreVaultClient::new(root_path);
    local_store_vault.incorporate_diff(file_path)?;
    Ok(())
}
