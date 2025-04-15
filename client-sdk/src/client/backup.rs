use intmax2_interfaces::{
    api::store_vault_server::types::{CursorOrder, MetaDataCursor},
    data::{
        data_type::DataType, encryption::BlsEncryption, meta_data::MetaData,
        transfer_data::TransferData,
    },
    utils::digest::get_digest,
};
use intmax2_zkp::{common::signature_content::key_set::KeySet, ethereum_types::bytes32::Bytes32};

use crate::external_api::local_backup_store_vault::diff_data_client::{
    make_backup_csv_from_records, DiffRecord,
};

use super::{client::Client, strategy::error::StrategyError};

pub async fn make_history_backup(
    client: &Client,
    key: KeySet,
    from: u64,
    chunk_size: usize,
) -> Result<Vec<String>, StrategyError> {
    let cursor = MetaDataCursor {
        cursor: Some(MetaData {
            timestamp: from,
            digest: Bytes32::default(),
        }),
        order: CursorOrder::Asc,
        limit: None,
    };
    let mut all_records = Vec::new();

    for data_type in [
        DataType::Deposit,
        DataType::Transfer,
        DataType::Tx,
        DataType::Withdrawal,
    ] {
        let records = fetch_records(client, key, &data_type.to_topic(), &cursor).await?;
        all_records.extend(records);
    }

    // decrypt transfer data to fetch sender proof set
    let mut transfer_data = Vec::new();
    for record in all_records.iter() {
        if record.topic == DataType::Transfer.to_topic() {
            let transfer_data_entry = match TransferData::decrypt(key, None, &record.data) {
                Ok(transfer_data_entry) => transfer_data_entry,
                Err(e) => {
                    log::warn!(
                        "failed to decrypt transfer data with digest {}: {}",
                        record.digest,
                        e
                    );
                    continue;
                }
            };
            transfer_data.push(transfer_data_entry);
        }
    }

    // fetch sender proof set
    for transfer_data in transfer_data.iter() {
        let sender_proof_set_key = KeySet::new(transfer_data.sender_proof_set_ephemeral_key);
        let sender_proof_set_data = client
            .store_vault_server
            .get_snapshot(sender_proof_set_key, &DataType::SenderProofSet.to_topic())
            .await?
            .ok_or(StrategyError::SenderProofSetNotFound)?;
        all_records.push(DiffRecord {
            topic: DataType::SenderProofSet.to_topic(),
            pubkey: sender_proof_set_key.pubkey.into(),
            digest: get_digest(&sender_proof_set_data),
            timestamp: chrono::Utc::now().timestamp() as u64, // use current time because we don't have to care about the timestamp for snapshot
            data: sender_proof_set_data,
        });
    }

    // fetch user data
    let user_data = client
        .store_vault_server
        .get_snapshot(key, &DataType::UserData.to_topic())
        .await?;
    if let Some(user_data) = user_data {
        all_records.push(DiffRecord {
            topic: DataType::UserData.to_topic(),
            pubkey: key.pubkey.into(),
            digest: get_digest(&user_data),
            timestamp: chrono::Utc::now().timestamp() as u64, // use current time because we don't have to care about the timestamp for snapshot
            data: user_data,
        });
    }

    let mut backup_csvs = Vec::new();
    for chunks in all_records.chunks(chunk_size) {
        let csv = make_backup_csv_from_records(chunks).map_err(|e| {
            StrategyError::UnexpectedError(format!("failed to make backup csv: {}", e))
        })?;
        backup_csvs.push(csv);
    }
    Ok(backup_csvs)
}

async fn fetch_records(
    client: &Client,
    key: KeySet,
    topic: &str,
    cursor: &MetaDataCursor,
) -> Result<Vec<DiffRecord>, StrategyError> {
    let mut records = Vec::new();
    let mut cursor = cursor.clone();
    loop {
        let (data_with_meta, cursor_response) = client
            .store_vault_server
            .get_data_sequence(key, topic, &cursor)
            .await?;
        records.extend(data_with_meta.into_iter().map(|data_with_meta| DiffRecord {
            topic: topic.to_string(),
            pubkey: key.pubkey.into(),
            digest: data_with_meta.meta.digest,
            timestamp: data_with_meta.meta.timestamp,
            data: data_with_meta.data,
        }));
        if !cursor_response.has_more {
            break;
        }
        cursor.cursor = cursor_response.next_cursor;
    }
    Ok(records)
}
