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
        meta_data::{MetaData, MetaDataWithBlockNumber},
        tx_data::TxData,
        user_data::ProcessStatus,
    },
};
use intmax2_zkp::{
    common::signature_content::key_set::KeySet,
    ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait as _},
};

#[derive(Debug, Clone)]
pub struct TxInfo {
    pub settled: Vec<(MetaDataWithBlockNumber, TxData)>,
    pub pending: Vec<(MetaData, TxData)>,
    pub timeout: Vec<(MetaData, TxData)>,
}

#[allow(clippy::too_many_arguments)]
pub async fn fetch_tx_info(
    store_vault_server: &dyn StoreVaultClientInterface,
    validity_prover: &dyn ValidityProverClientInterface,
    key: KeySet,
    current_time: u64, // current timestamp for timeout checking
    included_digests: &[Bytes32],
    excluded_digests: &[Bytes32],
    cursor: &MetaDataCursor,
    tx_timeout: u64,
) -> Result<(TxInfo, MetaDataCursorResponse), StrategyError> {
    let mut settled = Vec::new();
    let mut pending = Vec::new();
    let mut timeout = Vec::new();

    let (data_with_meta, cursor_response) = fetch_decrypt_validate::<TxData>(
        store_vault_server,
        key,
        DataType::Tx,
        included_digests,
        excluded_digests,
        cursor,
    )
    .await?;

    // Prepare batch request data
    let tx_tree_roots: Vec<_> = data_with_meta
        .iter()
        .map(|(_, tx_data)| tx_data.tx_tree_root)
        .collect();
    let block_numbers = validity_prover
        .get_block_number_by_tx_tree_root_batch(&tx_tree_roots)
        .await?;

    // Process results and categorize transactions
    for ((meta, tx_data), block_number) in data_with_meta.into_iter().zip(block_numbers) {
        match block_number {
            Some(block_number) => {
                // Transaction is settled
                let meta = MetaDataWithBlockNumber { meta, block_number };
                settled.push((meta, tx_data));
            }
            None if meta.timestamp + tx_timeout < current_time => {
                // Transaction has timed out
                timeout.push((meta, tx_data));
            }
            None => {
                // Transaction is still pending
                log::info!("Tx {} is pending", meta.digest);
                pending.push((meta, tx_data));
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
        TxInfo {
            settled,
            pending,
            timeout,
        },
        cursor_response,
    ))
}

pub async fn fetch_all_unprocessed_tx_info(
    store_vault_server: &dyn StoreVaultClientInterface,
    validity_prover: &dyn ValidityProverClientInterface,
    key: KeySet,
    current_time: u64,
    process_status: &ProcessStatus,
    tx_timeout: u64,
) -> Result<TxInfo, StrategyError> {
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
            TxInfo {
                settled: settled_part,
                pending: pending_part,
                timeout: timeout_part,
            },
            cursor_response,
        ) = fetch_tx_info(
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

    Ok(TxInfo {
        settled,
        pending,
        timeout,
    })
}
