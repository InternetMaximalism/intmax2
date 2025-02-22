use serde::{Deserialize, Serialize};

use super::encryption::BlsEncryption;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenericMiscData {
    pub data: Vec<u8>,
}

impl BlsEncryption for GenericMiscData {}
