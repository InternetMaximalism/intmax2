use intmax2_interfaces::{
    api::{
        block_builder::interface::Fee, store_vault_server::interface::StoreVaultClientInterface,
    },
    data::encryption::Encryption,
};

use intmax2_zkp::{
    common::{
        claim::Claim, signature::key_set::KeySet, transfer::Transfer, withdrawal::Withdrawal,
    },
    ethereum_types::{bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait},
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::Digest as _;

use crate::client::error::ClientError;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", bound(deserialize = ""))]
pub struct PaymentMemo<M: Clone + Serialize + DeserializeOwned> {
    pub transfer_uuid: String,
    pub sender: U256,
    pub transfer: Transfer,
    pub memo: M,
}

impl<M: Clone + Serialize + DeserializeOwned> Encryption for PaymentMemo<M> {}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawalFeeMemo {
    pub withdrawal: Withdrawal,
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
    payment_memo: &PaymentMemo<M>,
) -> Result<String, ClientError> {
    let topic = get_topic(payment_memo);
    let uuid = store_vault_server
        .save_misc(key, topic, &payment_memo.encrypt(key.pubkey))
        .await?;
    Ok(uuid)
}

pub async fn get_payment_memos<
    S: StoreVaultClientInterface,
    M: Default + Clone + Serialize + DeserializeOwned,
>(
    store_vault_server: &S,
    key: KeySet,
) -> Result<Vec<PaymentMemo<M>>, ClientError> {
    let topic = get_topic::<M>(&PaymentMemo::<M>::default());
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

fn get_topic<M: Clone + Serialize + DeserializeOwned>(_payment_memo: &PaymentMemo<M>) -> Bytes32 {
    let path_independent_type_name = match std::any::type_name::<M>().rfind(':') {
        Some(index) => &std::any::type_name::<M>()[index + 1..],
        None => std::any::type_name::<M>(),
    };
    let topic_str = format!("PaymentMemo<{}>", path_independent_type_name);
    dbg!(&topic_str);
    let digest: [u8; 32] = sha2::Sha256::digest(topic_str).into();
    Bytes32::from_bytes_be(&digest)
}

#[cfg(test)]
mod tests {
    use intmax2_interfaces::api::block_builder::interface::Fee;
    use intmax2_zkp::{common::withdrawal::Withdrawal, ethereum_types::u256::U256};

    use crate::client::misc::payment_memo::WithdrawalFeeMemo;

    use super::*;

    #[test]
    fn test_get_topic() {
        let payment_memo = PaymentMemo {
            transfer_uuid: "uuid".to_string(),
            sender: U256::from(0),
            transfer: Default::default(),
            memo: WithdrawalFeeMemo {
                withdrawal: Withdrawal::rand(&mut rand::thread_rng()),
                fee: Fee {
                    token_index: 0,
                    amount: 0.into(),
                },
            },
        };
        let _topic = get_topic(&payment_memo);
    }
}
