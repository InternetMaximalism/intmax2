use intmax2_interfaces::{api::error::ServerError, data::encryption::errors::EncryptionError};
use thiserror::Error;

use crate::external_api::contract::error::BlockchainError;

#[derive(Debug, Error)]
pub enum StrategyError {
    #[error("Server client error: {0}")]
    ServerError(#[from] ServerError),

    #[error("Encryption error: {0}")]
    EncryptionError(#[from] EncryptionError),

    #[error("Blockchain error: {0}")]
    BlockchainError(#[from] BlockchainError),

    #[error("Balance insufficient before sync")]
    BalanceInsufficientBeforeSync,

    #[error("Balance insufficient during sync")]
    BalanceInsufficientDuringSync,

    #[error("User data decryption error: {0}")]
    UserDataDecryptionError(String),

    #[error("Pending receives error: {0}")]
    PendingReceivesError(String),

    #[error("Pending tx error: {0}")]
    PendingTxError(String),
}
