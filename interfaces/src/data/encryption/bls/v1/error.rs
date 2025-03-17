/// An error that occurs while reading or writing to an ECIES stream.
#[derive(Debug, thiserror::Error)]
pub enum ECIESError {
    /// Error when checking the HMAC tag against the tag on the message being decrypted
    #[error("tag check failure in read_header")]
    TagCheckDecryptFailed,
    /// The encrypted data is not large enough for all fields
    #[error("encrypted data is not large enough for all fields")]
    EncryptedDataTooSmall,
}
