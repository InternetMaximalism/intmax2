use crate::client::{error::ClientError, sync::error::SyncError};
use intmax2_interfaces::{
    api::store_vault_server::{
        interface::{SaveDataEntry, StoreVaultClientInterface},
        types::{CursorOrder, MetaDataCursor},
    },
    data::{
        encryption::{errors::BlsEncryptionError, BlsEncryption},
        meta_data::MetaData,
        rw_rights::{RWRights, ReadRights, WriteRights},
        topic::topic_from_rights,
        transfer_data::TransferData,
    },
    utils::key::PrivateKey,
};
use intmax2_zkp::ethereum_types::bytes32::Bytes32;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub fn payment_memo_topic(name: &str) -> String {
    topic_from_rights(
        RWRights {
            read_rights: ReadRights::AuthRead,
            write_rights: WriteRights::AuthWrite,
        },
        format!("payment_memo/{name}").as_str(),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", bound(deserialize = ""))]
pub struct PaymentMemo {
    pub meta: MetaData,
    pub transfer_data: TransferData,
    pub memo: String,
}

impl BlsEncryption for PaymentMemo {
    fn from_bytes(bytes: &[u8], version: u8) -> Result<Self, BlsEncryptionError> {
        match version {
            1 | 2 => Ok(bincode::deserialize(bytes)?),
            _ => Err(BlsEncryptionError::UnsupportedVersion(version)),
        }
    }
}

pub async fn save_payment_memo<M: Default + Clone + Serialize + DeserializeOwned>(
    store_vault_server: &dyn StoreVaultClientInterface,
    view_priv: PrivateKey,
    memo_name: &str,
    payment_memo: &PaymentMemo,
) -> Result<Bytes32, ClientError> {
    let topic = payment_memo_topic(memo_name);
    let entry = SaveDataEntry {
        topic,
        pubkey: view_priv.to_public_key().0,
        data: payment_memo.encrypt(view_priv.to_public_key(), Some(view_priv))?,
    };
    let digests = store_vault_server
        .save_data_batch(view_priv, &[entry])
        .await?;
    Ok(digests[0])
}

pub async fn get_all_payment_memos(
    store_vault_server: &dyn StoreVaultClientInterface,
    view_priv: PrivateKey,
    memo_name: &str,
) -> Result<Vec<PaymentMemo>, SyncError> {
    let topic = payment_memo_topic(memo_name);
    let mut encrypted_memos = vec![];
    let mut cursor = None;
    loop {
        let (encrypted_memos_partial, cursor_response) = store_vault_server
            .get_data_sequence(
                view_priv,
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
        // todo: specify sender view pub
        let memo = PaymentMemo::decrypt(view_priv, None, &encrypted_memo.data)?;
        memos.push(memo);
    }

    Ok(memos)
}
