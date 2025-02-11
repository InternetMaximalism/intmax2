use intmax2_interfaces::{api::block_builder::interface::Fee, data::encryption::Encryption};
use intmax2_zkp::{
    common::{claim::Claim, transfer::Transfer, withdrawal::Withdrawal},
    ethereum_types::u256::U256,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

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
