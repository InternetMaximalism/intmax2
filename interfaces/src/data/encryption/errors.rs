use thiserror::Error;

#[derive(Debug, Error)]
pub enum BlsEncryptionError {
    #[error("Deserialization error: {0}")]
    DeserializeError(#[from] bincode::Error),

    #[error("Decryption error: {0}")]
    DecryptionError(String),
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
