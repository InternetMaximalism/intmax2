use intmax2_interfaces::{
    api::store_vault_server::interface::StoreVaultClientInterface, data::encryption::Encryption,
};

use intmax2_zkp::common::{signature::key_set::KeySet, transfer::Transfer};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::client::error::ClientError;

use super::get_topic;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", bound(deserialize = ""))]
pub struct PaymentMemo {
    pub transfer_uuid: String,
    pub transfer: Transfer,
    pub memo: String,
}

impl Encryption for PaymentMemo {}

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

pub async fn get_payment_memos<S: StoreVaultClientInterface>(
    store_vault_server: &S,
    key: KeySet,
    memo_name: &str,
) -> Result<Vec<PaymentMemo>, ClientError> {
    let topic = get_topic(memo_name);
    let encrypted_memos = store_vault_server
        .get_misc_sequence(key, topic, &None)
        .await?;
    let mut memos = Vec::new();
    for encrypted_memo in encrypted_memos {
        let memo = PaymentMemo::decrypt(&encrypted_memo.data, key)?;
        memos.push(memo);
    }
    Ok(memos)
}
