use intmax2_interfaces::{
    api::error::ServerError,
    data::{encryption::errors::BlsEncryptionError, proof_compression::ProofCompressionError},
};
use thiserror::Error;

use crate::external_api::contract::error::BlockchainError;

#[derive(Debug, Error)]
pub enum StrategyError {
    #[error("Server client error: {0}")]
    ServerError(#[from] ServerError),

    #[error("Encryption error: {0}")]
    EncryptionError(#[from] BlsEncryptionError),

    #[error("Proof compression error: {0}")]
    ProofCompressionError(#[from] ProofCompressionError),

    #[error("Blockchain error: {0}")]
    BlockchainError(#[from] BlockchainError),

    #[error("Balance insufficient before sync")]
    BalanceInsufficientBeforeSync,

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Balance insufficient during sync")]
    BalanceInsufficientDuringSync,

    #[error("User data decryption error: {0}")]
    UserDataDecryptionError(String),

    #[error("Pending receives error: {0}")]
    PendingReceivesError(String),

    #[error("Pending tx error: {0}")]
    PendingTxError(String),

    #[error("Sender proof set not found")]
    SenderProofSetNotFound,

    #[error("Unexpected error: {0}")]
    UnexpectedError(String),
}
