use super::{common::fetch_decrypt_validate, error::StrategyError};
use intmax2_interfaces::{
    api::{
        store_vault_server::{
            interface::{DataType, StoreVaultClientInterface},
            types::{CursorOrder, MetaDataCursor, MetaDataCursorResponse},
        },
        validity_prover::interface::ValidityProverClientInterface,
    },
    data::{
        meta_data::{MetaData, MetaDataWithBlockNumber},
        tx_data::TxData,
    },
};
use intmax2_zkp::common::signature::key_set::KeySet;

#[derive(Debug, Clone)]
pub struct TxInfo {
    pub settled: Vec<(MetaDataWithBlockNumber, TxData)>,
    pub pending: Vec<(MetaData, TxData)>,
    pub timeout: Vec<(MetaData, TxData)>,
    pub cursor_response: MetaDataCursorResponse,
}

pub async fn fetch_tx_info<S: StoreVaultClientInterface, V: ValidityProverClientInterface>(
    store_vault_server: &S,
    validity_prover: &V,
    key: KeySet,
    included_uuids: &[String],
    excluded_uuids: &[String],
    cursor: &MetaDataCursor,
    tx_timeout: u64,
) -> Result<TxInfo, StrategyError> {
    let mut settled = Vec::new();
    let mut pending = Vec::new();
    let mut timeout = Vec::new();

    let (data_with_meta, cursor_response) = fetch_decrypt_validate::<_, TxData>(
        store_vault_server,
        key,
        DataType::Tx,
        included_uuids,
        excluded_uuids,
        cursor,
    )
    .await?;
    for (meta, tx_data) in data_with_meta {
        let tx_tree_root = tx_data.tx_tree_root;
        let block_number = validity_prover
            .get_block_number_by_tx_tree_root(tx_tree_root)
            .await?;
        if let Some(block_number) = block_number {
            let meta = MetaDataWithBlockNumber { meta, block_number };
            settled.push((meta, tx_data));
        } else if meta.timestamp + tx_timeout < chrono::Utc::now().timestamp() as u64 {
            // timeout
            log::error!("Tx {} is timeout", meta.uuid);
            timeout.push((meta, tx_data));
        } else {
            // pending
            log::info!("Tx {} is pending", meta.uuid);
            pending.push((meta, tx_data));
        }
    }

    // sort
    settled.sort_by_key(|(meta, _)| (meta.block_number, meta.meta.uuid.clone()));
    pending.sort_by_key(|(meta, _)| (meta.timestamp, meta.uuid.clone()));
    timeout.sort_by_key(|(meta, _)| (meta.timestamp, meta.uuid.clone()));
    if cursor.order == CursorOrder::Desc {
        settled.reverse();
        pending.reverse();
        timeout.reverse();
    }

    Ok(TxInfo {
        settled,
        pending,
        timeout,
        cursor_response,
    })
}
