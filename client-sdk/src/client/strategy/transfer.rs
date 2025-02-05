use super::{
    common::{fetch_decrypt_validate, fetch_sender_proof_set},
    error::StrategyError,
};
use intmax2_interfaces::{
    api::{
        store_vault_server::interface::{DataType, StoreVaultClientInterface},
        validity_prover::interface::ValidityProverClientInterface,
    },
    data::{
        meta_data::{MetaData, MetaDataWithBlockNumber},
        transfer_data::TransferData,
        user_data::ProcessStatus,
    },
};
use intmax2_zkp::common::signature::key_set::KeySet;

#[derive(Debug, Clone)]
pub struct TransferInfo {
    pub settled: Vec<(MetaDataWithBlockNumber, TransferData)>,
    pub pending: Vec<(MetaData, TransferData)>,
    pub timeout: Vec<(MetaData, TransferData)>,
}

pub async fn fetch_transfer_info<S: StoreVaultClientInterface, V: ValidityProverClientInterface>(
    store_vault_server: &S,
    validity_prover: &V,
    key: KeySet,
    transfer_status: &ProcessStatus,
    tx_timeout: u64,
) -> Result<TransferInfo, StrategyError> {
    let mut settled = Vec::new();
    let mut pending = Vec::new();
    let mut timeout = Vec::new();
    let data_with_meta = fetch_decrypt_validate::<_, TransferData>(
        store_vault_server,
        key,
        DataType::Transfer,
        transfer_status,
    )
    .await?;
    for (meta, transfer_data) in data_with_meta {
        let sender_proof_set = match fetch_sender_proof_set(
            store_vault_server,
            transfer_data.sender_proof_set_ephemeral_key,
        )
        .await
        {
            Ok(sender_proof_set) => sender_proof_set,
            // ignore encryption error
            Err(StrategyError::EncryptionError(e)) => {
                log::error!("failed to decrypt sender proof set: {}", e);
                continue;
            }
            // return other errors
            Err(e) => return Err(e),
        };
        let mut transfer_data = transfer_data;
        transfer_data.set_sender_proof_set(sender_proof_set);

        let tx_tree_root = transfer_data.tx_tree_root;
        let block_number = validity_prover
            .get_block_number_by_tx_tree_root(tx_tree_root)
            .await?;
        if let Some(block_number) = block_number {
            // set block number
            let meta = MetaDataWithBlockNumber { meta, block_number };
            settled.push((meta, transfer_data));
        } else if meta.timestamp + tx_timeout < chrono::Utc::now().timestamp() as u64 {
            // timeout
            log::error!("Transfer {} is timeout", meta.uuid);
            timeout.push((meta, transfer_data));
        } else {
            // pending
            log::info!("Transfer {} is pending", meta.uuid);
            pending.push((meta, transfer_data));
        }
    }

    // sort by block number
    settled.sort_by_key(|(meta, _)| meta.block_number);

    // sort by timestamp
    pending.sort_by_key(|(meta, _)| meta.timestamp);

    Ok(TransferInfo {
        settled,
        pending,
        timeout,
    })
}
