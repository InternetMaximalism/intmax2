use serde::{Deserialize, Serialize};

use crate::{
    data::encryption::bls::v1::singed_encryption::V1SignedEncryption,
    utils::key::{PrivateKey, PublicKey},
};

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
        receiver_view_pub: PublicKey,
        sender_view_priv: Option<PrivateKey>,
        data: &[u8],
    ) -> Result<Self, VersionedBlsEncryptionError> {
        match version {
            1 | 2 => {
                let sender_key = sender_view_priv.map(|priv_key| priv_key.to_key_set());
                let encrypted_data =
                    V1SignedEncryption::encrypt(receiver_view_pub.0, sender_key, data);
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
        receiver_view_priv: PrivateKey,
        sender_view_pub: Option<PublicKey>,
    ) -> Result<Vec<u8>, VersionedBlsEncryptionError> {
        match self.version {
            1 | 2 => {
                let sender = sender_view_pub.map(|pub_key| pub_key.0);
                let encrypted_data: V1SignedEncryption = bincode::deserialize(&self.data)?;
                let data = encrypted_data
                    .decrypt(receiver_view_priv.to_key_set(), sender)
                    .map_err(|e| VersionedBlsEncryptionError::DecryptionError(e.to_string()))?;
                Ok(data)
            }
            _ => Err(VersionedBlsEncryptionError::UnsupportedVersion),
        }
    }
}
