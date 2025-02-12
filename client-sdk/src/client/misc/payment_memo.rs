use intmax2_interfaces::{
    api::{
        block_builder::interface::Fee, store_vault_server::interface::StoreVaultClientInterface,
    },
    data::encryption::Encryption,
};

use intmax2_zkp::common::{claim::Claim, signature::key_set::KeySet, transfer::Transfer};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::client::error::ClientError;

use super::get_topic;

pub const WITHDRAWAL_FEE_MEMO: &str = "withdrawal_fee_memo";
pub const CLAIM_FEE_MEMO: &str = "claim_fee_memo";
pub const USED_OR_INVALID_MEMO: &str = "used_or_invalid_memo";

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", bound(deserialize = ""))]
pub struct PaymentMemo {
    pub transfer_uuid: String,
    pub transfer: Transfer,
    pub memo: String,
}

impl Encryption for PaymentMemo {}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawalFeeMemo {
    pub withdrawal_transfer: Transfer,
    pub fee: Fee,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimFeeMemo {
    pub claim: Claim,
    pub fee: Fee,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsedOrInvalidMemo {
    pub reason: String,
}

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
