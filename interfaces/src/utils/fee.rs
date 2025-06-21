use intmax2_zkp::ethereum_types::u256::U256;
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fee {
    pub token_index: u32,
    pub amount: U256,
}
