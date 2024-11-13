use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u256::U256};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepositNativeTokenRequest {
    pub pubkey_salt_hash: Bytes32,
    pub amount: U256,
}
