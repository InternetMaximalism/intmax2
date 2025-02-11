use intmax2_zkp::{common::transfer::Transfer, ethereum_types::u256::U256};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", bound(deserialize = "M: Deserialize<'de>"))]
pub struct PaymentMemo<M: Clone + Serialize> {
    pub transfer_uuid: String,
    pub sender: U256,
    pub transfer: Transfer,
    pub memo: M,
}
