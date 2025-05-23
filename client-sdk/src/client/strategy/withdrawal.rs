use super::{common::fetch_decrypt_validate, error::StrategyError};
use intmax2_interfaces::{
    api::{
        store_vault_server::{
            interface::StoreVaultClientInterface,
            types::{CursorOrder, MetaDataCursor, MetaDataCursorResponse},
        },
        validity_prover::interface::ValidityProverClientInterface,
    },
    data::{
        data_type::DataType,
        encryption::BlsEncryption,
        meta_data::{MetaData, MetaDataWithBlockNumber},
        rw_rights::WriteRights,
        sender_proof_set::SenderProofSet,
        transfer_data::TransferData,
        user_data::ProcessStatus,
    },
};
use intmax2_zkp::{
    common::signature_content::key_set::KeySet,
    ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait as _},
};

#[derive(Debug, Clone)]
pub struct WithdrawalInfo {
    pub settled: Vec<(MetaDataWithBlockNumber, TransferData)>,
    pub pending: Vec<(MetaData, TransferData)>,
    pub timeout: Vec<(MetaData, TransferData)>,
}

#[allow(clippy::too_many_arguments)]
pub async fn fetch_withdrawal_info(
    store_vault_server: &dyn StoreVaultClientInterface,
    validity_prover: &dyn ValidityProverClientInterface,
    key: KeySet,
    current_time: u64, // current timestamp for timeout checking
    included_digests: &[Bytes32],
    excluded_digests: &[Bytes32],
    cursor: &MetaDataCursor,
    tx_timeout: u64,
) -> Result<(WithdrawalInfo, MetaDataCursorResponse), StrategyError> {
    let mut settled = Vec::new();
    let mut pending = Vec::new();
    let mut timeout = Vec::new();

    let (data_with_meta, cursor_response) = fetch_decrypt_validate::<TransferData>(
        store_vault_server,
        key,
        DataType::Withdrawal,
        included_digests,
        excluded_digests,
        cursor,
    )
    .await?;

    // First, fetch and decrypt all sender proof sets
    let mut valid_transfers = Vec::new();
    for (meta, mut transfer_data) in data_with_meta {
        let ephemeral_key = KeySet::new(transfer_data.sender_proof_set_ephemeral_key);

        // Fetch encrypted sender proof set
        let encrypted_sender_proof_set = match store_vault_server
            .get_snapshot(ephemeral_key, &DataType::SenderProofSet.to_topic())
            .await?
        {
            Some(data) => data,
            None => {
                log::error!("sender proof set not found for withdrawal {}", meta.digest);
                continue;
            }
        };

        // Decrypt sender proof set
        let enc_sender = match DataType::UserData.rw_rights().write_rights {
            WriteRights::SingleAuthWrite => Some(ephemeral_key.pubkey),
            WriteRights::AuthWrite => Some(ephemeral_key.pubkey),
            WriteRights::SingleOpenWrite => None,
            WriteRights::OpenWrite => None,
        };
        let sender_proof_set =
            match SenderProofSet::decrypt(ephemeral_key, enc_sender, &encrypted_sender_proof_set) {
                Ok(data) => data,
                Err(e) => {
                    log::error!("failed to decrypt sender proof set: {e}");
                    continue;
                }
            };

        transfer_data.set_sender_proof_set(sender_proof_set);
        valid_transfers.push((meta, transfer_data));
    }

    // Batch fetch block numbers for all valid transfers
    let tx_tree_roots: Vec<_> = valid_transfers
        .iter()
        .map(|(_, transfer_data)| transfer_data.tx_tree_root)
        .collect();

    let block_numbers = validity_prover
        .get_block_number_by_tx_tree_root_batch(&tx_tree_roots)
        .await?;

    // Process results and categorize transfers
    for ((meta, transfer_data), block_number) in valid_transfers.into_iter().zip(block_numbers) {
        match block_number {
            Some(block_number) => {
                // Transfer is settled
                let meta = MetaDataWithBlockNumber { meta, block_number };
                settled.push((meta, transfer_data));
            }
            None if meta.timestamp + tx_timeout < current_time => {
                // Transfer has timed out
                timeout.push((meta, transfer_data));
            }
            None => {
                // Transfer is still pending
                log::info!("Withdrawal {} is pending", meta.digest);
                pending.push((meta, transfer_data));
            }
        }
    }

    // sort
    settled.sort_by_key(|(meta, _)| (meta.block_number, meta.meta.digest.to_hex()));
    pending.sort_by_key(|(meta, _)| (meta.timestamp, meta.digest.to_hex()));
    timeout.sort_by_key(|(meta, _)| (meta.timestamp, meta.digest.to_hex()));
    if cursor.order == CursorOrder::Desc {
        settled.reverse();
        pending.reverse();
        timeout.reverse();
    }

    Ok((
        WithdrawalInfo {
            settled,
            pending,
            timeout,
        },
        cursor_response,
    ))
}

pub async fn fetch_all_unprocessed_withdrawal_info(
    store_vault_server: &dyn StoreVaultClientInterface,
    validity_prover: &dyn ValidityProverClientInterface,
    key: KeySet,
    current_time: u64, // current timestamp for timeout checking
    process_status: &ProcessStatus,
    tx_timeout: u64,
) -> Result<WithdrawalInfo, StrategyError> {
    let mut cursor = MetaDataCursor {
        cursor: process_status.last_processed_meta_data.clone(),
        order: CursorOrder::Asc,
        limit: None,
    };
    let mut included_digests = process_status.pending_digests.clone(); // cleared after first fetch

    let mut settled = Vec::new();
    let mut pending = Vec::new();
    let mut timeout = Vec::new();
    loop {
        let (
            WithdrawalInfo {
                settled: settled_part,
                pending: pending_part,
                timeout: timeout_part,
            },
            cursor_response,
        ) = fetch_withdrawal_info(
            store_vault_server,
            validity_prover,
            key,
            current_time,
            &included_digests,
            &process_status.processed_digests,
            &cursor,
            tx_timeout,
        )
        .await?;
        if !included_digests.is_empty() {
            included_digests = Vec::new(); // clear included_digests after first fetch
        }

        settled.extend(settled_part);
        pending.extend(pending_part);
        timeout.extend(timeout_part);
        if !cursor_response.has_more {
            break;
        }
        cursor.cursor = cursor_response.next_cursor;
    }

    Ok(WithdrawalInfo {
        settled,
        pending,
        timeout,
    })
}
