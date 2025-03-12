use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedBlsEncryption {
    pub version: u8,
    pub data: Vec<u8>,
}
