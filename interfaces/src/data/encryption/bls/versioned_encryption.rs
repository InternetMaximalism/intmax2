use intmax2_zkp::{common::signature::key_set::KeySet, ethereum_types::u256::U256};
use serde::{Deserialize, Serialize};

use crate::data::encryption::bls::v1::singed_encryption::V1SignedEncryption;

#[derive(Debug, thiserror::Error)]
pub enum VersionedBlsEncryptionError {
    #[error("Unsupported version")]
    UnsupportedVersion,

    #[error("Deserialization error: {0}")]
    DeserializeError(#[from] bincode::Error),

    #[error("Encryption error: {0}")]
    EncryptionError(String),

    #[error("Decryption error: {0}")]
    DecryptionError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedBlsEncryption {
    pub version: u8,
    pub data: Vec<u8>,
}

impl VersionedBlsEncryption {
    pub fn encrypt(
        version: u8,
        receiver: U256,
        sender_key: Option<KeySet>,
        data: &[u8],
    ) -> Result<Self, VersionedBlsEncryptionError> {
        match version {
            1 => {
                let encrypted_data = V1SignedEncryption::encrypt(receiver, sender_key, data);
                Ok(Self {
                    version,
                    data: bincode::serialize(&encrypted_data)?,
                })
            }
            _ => Err(VersionedBlsEncryptionError::UnsupportedVersion),
        }
    }

    pub fn decrypt(
        &self,
        receiver_key: KeySet,
        sender: Option<U256>,
    ) -> Result<Vec<u8>, VersionedBlsEncryptionError> {
        match self.version {
            1 => {
                let encrypted_data: V1SignedEncryption = bincode::deserialize(&self.data)?;
                let data = encrypted_data
                    .decrypt(receiver_key, sender)
                    .map_err(|e| VersionedBlsEncryptionError::DecryptionError(e.to_string()))?;
                Ok(data)
            }
            _ => Err(VersionedBlsEncryptionError::UnsupportedVersion),
        }
    }
}
