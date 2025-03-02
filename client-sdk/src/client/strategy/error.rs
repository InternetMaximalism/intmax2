use intmax2_interfaces::api::error::ServerError;
use thiserror::Error;

use crate::external_api::contract::error::BlockchainError;

#[derive(Debug, Error)]
pub enum StrategyError {
    #[error("Server client error: {0}")]
    ServerError(#[from] ServerError),

    #[error("Blockchain error: {0}")]
    BlockchainError(#[from] BlockchainError),

    #[error("Balance insufficient before sync")]
    BalanceInsufficientBeforeSync,

    #[error("Balance insufficient during sync")]
    BalanceInsufficientDuringSync,

    #[error("User data decryption error: {0}")]
    UserDataDecryptionError(String),

    #[error("Unexpected error: {0}")]
    UnexpectedError(String),
}
