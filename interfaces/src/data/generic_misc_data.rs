use serde::{Deserialize, Serialize};

use crate::data::encryption::errors::BlsEncryptionError;

use super::encryption::BlsEncryption;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenericMiscData {
    pub data: Vec<u8>,
}

impl BlsEncryption for GenericMiscData {
    fn from_bytes(bytes: &[u8], version: u8) -> Result<Self, BlsEncryptionError> {
        match version {
            1 | 2 => Ok(bincode::deserialize(bytes)?),
            _ => Err(BlsEncryptionError::UnsupportedVersion(version)),
        }
    }
}
