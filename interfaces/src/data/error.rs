use super::proof_compression::ProofCompressionError;
use intmax2_zkp::circuits::balance::error::BalanceError;

#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error("Proof compression error: {0}")]
    ProofCompressionError(#[from] ProofCompressionError),

    #[error("Decryption error: {0}")]
    DecryptionError(String),

    #[error("Deserialization error: {0}")]
    DeserializeError(#[from] bincode::Error),

    #[error("Balance error: {0}")]
    BalanceError(#[from] BalanceError),
}
