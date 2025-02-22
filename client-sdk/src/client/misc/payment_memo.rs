use intmax2_interfaces::{
    api::store_vault_server::{
        interface::StoreVaultClientInterface,
        types::{CursorOrder, MetaDataCursor},
    },
    data::{encryption::BlsEncryption, meta_data::MetaData, transfer_data::TransferData},
};

use intmax2_zkp::common::signature::key_set::KeySet;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::client::{error::ClientError, sync::error::SyncError};

use super::get_topic;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", bound(deserialize = ""))]
pub struct PaymentMemo {
    pub meta: MetaData,
    pub transfer_data: TransferData,
    pub memo: String,
}

impl BlsEncryption for PaymentMemo {}

pub async fn save_payment_memo<
    S: StoreVaultClientInterface,
    M: Default + Clone + Serialize + DeserializeOwned,
>(
    store_vault_server: &S,
    key: KeySet,
    memo_name: &str,
    payment_memo: &PaymentMemo,
) -> Result<String, ClientError> {
    let topic = get_topic(memo_name);
    let uuid = store_vault_server
        .save_misc(key, topic, &payment_memo.encrypt(key.pubkey))
        .await?;
    Ok(uuid)
}

pub async fn get_all_payment_memos<S: StoreVaultClientInterface>(
    store_vault_server: &S,
    key: KeySet,
    memo_name: &str,
) -> Result<Vec<PaymentMemo>, SyncError> {
    let topic = get_topic(memo_name);

    let mut encrypted_memos = vec![];
    let mut cursor = None;
    loop {
        let (encrypted_memos_partial, cursor_response) = store_vault_server
            .get_misc_sequence(
                key,
                topic,
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
        let memo = PaymentMemo::decrypt(&encrypted_memo.data, key)?;
        memos.push(memo);
    }

    Ok(memos)
}
