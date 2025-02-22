use thiserror::Error;

/// An error that occurs while reading or writing to an ECIES stream.
#[derive(Debug, Error)]
pub enum ECIESError {
    /// Error when checking the HMAC tag against the tag on the message being decrypted
    #[error("tag check failure in read_header")]
    TagCheckDecryptFailed,
    /// The encrypted data is not large enough for all fields
    #[error("encrypted data is not large enough for all fields")]
    EncryptedDataTooSmall,
}

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
