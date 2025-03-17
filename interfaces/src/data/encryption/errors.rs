use thiserror::Error;

use super::bls::versioned_encryption::VersionedBlsEncryptionError;

#[derive(Debug, Error)]
pub enum BlsEncryptionError {
    #[error("{0}")]
    VersionedBlsEncryptionError(#[from] VersionedBlsEncryptionError),

    #[error("Deserialization error: {0}")]
    DeserializeError(#[from] bincode::Error),
}

#[derive(Debug, Error)]
pub enum RsaEncryptionError {
    #[error("Deserialization error: {0}")]
    DeserializeError(#[from] bincode::Error),

    #[error("RSA error: {0}")]
    RsaError(#[from] rsa::errors::Error),

    #[error("Decryption error: {0}")]
    DecryptionError(String),
}
