use intmax2_zkp::ethereum_types::bytes32::Bytes32;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct DiffRecord {
    pub topic: String,
    pub pubkey: Bytes32,
    pub digest: Bytes32,
    pub timestamp: u64,
    pub data: Vec<u8>,
}
