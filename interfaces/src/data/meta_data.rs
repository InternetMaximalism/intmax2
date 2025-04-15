use std::cmp::Ordering;

use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MetaData {
    pub timestamp: u64,
    pub digest: Bytes32,
}

impl PartialOrd for MetaData {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MetaData {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.timestamp == other.timestamp {
            self.digest.to_hex().cmp(&other.digest.to_hex())
        } else {
            self.timestamp.cmp(&other.timestamp)
        }
    }
}

impl MetaData {
    pub fn set_block_number(self, block_number: u32) -> MetaDataWithBlockNumber {
        MetaDataWithBlockNumber {
            meta: self,
            block_number,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaDataWithBlockNumber {
    pub meta: MetaData,
    pub block_number: u32,
}
