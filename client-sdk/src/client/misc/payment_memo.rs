use crate::client::{error::ClientError, sync::error::SyncError};
use intmax2_interfaces::{
    api::store_vault_server::{
        interface::{SaveDataEntry, StoreVaultClientInterface},
        types::{CursorOrder, MetaDataCursor},
    },
    data::{
        encryption::BlsEncryption,
        meta_data::MetaData,
        rw_rights::{RWRights, ReadRights, WriteRights},
        topic::topic_from_rights,
        transfer_data::TransferData,
    },
};
use intmax2_zkp::{common::signature_content::key_set::KeySet, ethereum_types::bytes32::Bytes32};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub fn payment_memo_topic(name: &str) -> String {
    topic_from_rights(
        RWRights {
            read_rights: ReadRights::AuthRead,
            write_rights: WriteRights::AuthWrite,
        },
        format!("payment_memo/{}", name).as_str(),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", bound(deserialize = ""))]
pub struct PaymentMemo {
    pub meta: MetaData,
    pub transfer_data: TransferData,
    pub memo: String,
}

impl BlsEncryption for PaymentMemo {}

pub async fn save_payment_memo<M: Default + Clone + Serialize + DeserializeOwned>(
    store_vault_server: &dyn StoreVaultClientInterface,
    key: KeySet,
    memo_name: &str,
    payment_memo: &PaymentMemo,
) -> Result<Bytes32, ClientError> {
    let topic = payment_memo_topic(memo_name);
    let entry = SaveDataEntry {
        topic,
        pubkey: key.pubkey,
        data: payment_memo.encrypt(key.pubkey, Some(key))?,
    };
    let digests = store_vault_server.save_data_batch(key, &[entry]).await?;
    Ok(digests[0])
}

pub async fn get_all_payment_memos(
    store_vault_server: &dyn StoreVaultClientInterface,
    key: KeySet,
    memo_name: &str,
) -> Result<Vec<PaymentMemo>, SyncError> {
    let topic = payment_memo_topic(memo_name);
    let mut encrypted_memos = vec![];
    let mut cursor = None;
    loop {
        let (encrypted_memos_partial, cursor_response) = store_vault_server
            .get_data_sequence(
                key,
                &topic,
                &MetaDataCursor {
                    cursor: cursor.clone(),
                    order: CursorOrder::Asc,
                    limit: None,
                },
            )
            .await?;
        encrypted_memos.extend(encrypted_memos_partial);
        if cursor_response.has_more {
            cursor = cursor_response.next_cursor;
        } else {
            break;
        }
    }

    let mut memos = Vec::new();
    for encrypted_memo in encrypted_memos {
        let memo = PaymentMemo::decrypt(key, None, &encrypted_memo.data)?;
        memos.push(memo);
    }

    Ok(memos)
}
