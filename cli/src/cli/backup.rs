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

fn incorporate_backup_with_env(file_path: &Path, env: &EnvVar) -> Result<(), CliError> {
    let root_path = get_backup_root_path(env)?;
    let local_store_vault = LocalStoreVaultClient::new(root_path);
    local_store_vault.incorporate_diff(file_path)?;
    Ok(())
}

pub fn incorporate_backup(file_path: &Path) -> Result<(), CliError> {
    let env = envy::from_env::<EnvVar>()?;
    incorporate_backup_with_env(file_path, &env)
}

pub async fn make_history_backup(key: KeySet, dir: &Path, from: u64) -> Result<(), CliError> {
    let client = get_client()?;
    let csvs = client
        .make_history_backup(key, from, BACKUP_CHUNK_SIZE)
        .await?;
    for csv_str in csvs.iter() {
        let id = Uuid::new_v4().to_string()[..8].to_string();
        let file_path = dir.join(format!("backup_{id}.csv"));
        tokio::fs::write(&file_path, csv_str).await.map_err(|e| {
            CliError::BackupError(format!("Failed to write backup to {file_path:?}: {e}"))
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::env::EnvType;
    use intmax2_interfaces::api::store_vault_server::types::StoreVaultType;
    use intmax2_zkp::ethereum_types::address::Address;
    use std::{
        fs::{self, File},
        io::Write,
        str::FromStr,
    };
    use tempfile::tempdir;

    #[test]
    fn test_incorporate_backup_success() {
        let env_var = EnvVar {
            env: EnvType::Local,
            is_faster_mining: true,
            indexer_base_url: "https://dev.builder.indexer.intmax.xyz".to_string(),
            store_vault_server_base_url: Some("http://localhost:9000".to_string()),
            store_vault_type: StoreVaultType::Remote,
            balance_prover_base_url: "http://localhost:9001".to_string(),
            use_private_zkp_server: Some(false),
            validity_prover_base_url: "http://localhost:9002".to_string(),
            withdrawal_server_base_url: "http://localhost:9003".to_string(),
            deposit_timeout: 180,
            tx_timeout: 80,
            block_builder_query_wait_time: 5,
            block_builder_query_interval: 5,
            block_builder_query_limit: 20,
            l1_rpc_url: "http://127.0.0.1:8545".to_string(),
            liquidity_contract_address: Address::from_str(
                "0xdc64a140aa3e981100a9beca4e685f962f0cf6c9",
            )
            .unwrap(),
            l2_rpc_url: "http://127.0.0.1:8545".to_string(),
            rollup_contract_address: Address::from_str(
                "0xe7f1725e7734ce288f8367e1bb143e90bb3f0512",
            )
            .unwrap(),
            withdrawal_contract_address: Address::from_str(
                "0x8a791620dd6260079bf849dc5567adc3f2fdc318",
            )
            .unwrap(),

            // Required fields not in your list, you have to provide reasonable defaults or fill them accordingly:
            local_backup_path: None,
            predicate_base_url: None,
            wallet_key_vault_base_url: None,
            block_builder_base_url: Some("http://localhost:9004".to_string()),
            reward_contract_address: None,
            private_zkp_server_max_retires: None,
            private_zkp_server_retry_interval: None,
        };

        let dir = tempdir().unwrap();
        let file_path = dir.path().join("dummy.csv");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "topic,pubkey,digest,timestamp,data").unwrap();
        writeln!(
            file,
            "test_topic,0000000000000000000000000000000000000000000000000000000000000000,\
            1111111111111111111111111111111111111111111111111111111111111111,\
            1717580143,SGVsbG8gd29ybGQ="
        )
        .unwrap();

        fs::create_dir_all("./testdata").unwrap();

        let result = incorporate_backup_with_env(&file_path, &env_var);

        assert!(result.is_ok(), "Got error: {:?}", result.unwrap_err());

        fs::remove_dir_all("./testdata").ok();
    }
}
