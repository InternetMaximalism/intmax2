use super::{common::fetch_decrypt_validate, error::StrategyError};
use intmax2_interfaces::{
    api::{
        store_vault_server::interface::{DataType, StoreVaultClientInterface},
        validity_prover::interface::ValidityProverClientInterface,
    },
    data::{
        encryption::BlsEncryption,
        meta_data::{MetaData, MetaDataWithBlockNumber},
        sender_proof_set::SenderProofSet,
        transfer_data::TransferData,
        user_data::ProcessStatus,
    },
};
use intmax2_zkp::common::signature::key_set::KeySet;
use num_bigint::BigUint;

#[derive(Debug, Clone)]
pub struct WithdrawalInfo {
    pub settled: Vec<(MetaDataWithBlockNumber, TransferData)>,
    pub pending: Vec<(MetaData, TransferData)>,
    pub timeout: Vec<(MetaData, TransferData)>,
}

pub async fn fetch_withdrawal_info<
    S: StoreVaultClientInterface,
    V: ValidityProverClientInterface,
>(
    store_vault_server: &S,
    validity_prover: &V,
    key: KeySet,
    withdrawal_status: &ProcessStatus,
    tx_timeout: u64,
) -> Result<WithdrawalInfo, StrategyError> {
    let mut settled = Vec::new();
    let mut pending = Vec::new();
    let mut timeout = Vec::new();

    let data_with_meta = fetch_decrypt_validate::<_, TransferData>(
        store_vault_server,
        key,
        DataType::Withdrawal,
        withdrawal_status,
    )
    .await?;
    for (meta, transfer_data) in data_with_meta {
        let ephemeral_key =
            KeySet::new(BigUint::from(transfer_data.sender_proof_set_ephemeral_key).into());
        let encrypted_sender_proof_set = store_vault_server
            .get_sender_proof_set(ephemeral_key)
            .await?;
        let sender_proof_set =
            match SenderProofSet::decrypt(&encrypted_sender_proof_set, ephemeral_key) {
                Ok(data) => data,
                Err(e) => {
                    log::error!("failed to decrypt sender proof set: {}", e);
                    continue;
                }
            };
        let mut transfer_data = transfer_data;
        transfer_data.set_sender_proof_set(sender_proof_set);
        let tx_tree_root = transfer_data.tx_tree_root;
        let block_number = validity_prover
            .get_block_number_by_tx_tree_root(tx_tree_root)
            .await?;
        if let Some(block_number) = block_number {
            let meta = MetaDataWithBlockNumber { meta, block_number };
            settled.push((meta, transfer_data));
        } else if meta.timestamp + tx_timeout < chrono::Utc::now().timestamp() as u64 {
            // timeout
            log::error!("Withdrawal {} is timeout", meta.uuid);
            timeout.push((meta, transfer_data));
        } else {
            // pending
            log::info!("Withdrawal {} is pending", meta.uuid);
            pending.push((meta, transfer_data));
        }
    }
    Ok(WithdrawalInfo {
        settled,
        pending,
        timeout,
    })
}
