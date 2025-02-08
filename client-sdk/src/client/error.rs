use intmax2_interfaces::{api::error::ServerError, data::proof_compression::ProofCompressionError};

use crate::external_api::contract::error::BlockchainError;

use super::{strategy::error::StrategyError, sync::error::SyncError};

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("Server client error: {0}")]
    ServerError(#[from] ServerError),

    #[error("Blockchain error: {0}")]
    BlockchainError(#[from] BlockchainError),

    #[error("Strategy error: {0}")]
    StrategyError(#[from] StrategyError),

    #[error("Sync error: {0}")]
    SyncError(#[from] SyncError),

    #[error("Proof compression error: {0}")]
    ProofCompressionError(#[from] ProofCompressionError),

    #[error("Send tx request error: {0}")]
    SendTxRequestError(String),

    #[error("Balance error: {0}")]
    BalanceError(String),

    #[error("Invalid transfer len: {0}")]
    TransferLenError(String),

    #[error("Cannot send tx by zero balance account")]
    CannotSendTxByZeroBalanceAccount,

    #[error("Invalid block proposal: {0}")]
    InvalidBlockProposal(String),

    #[error("Invalid mining deposit criteria")]
    InvalidMiningDepositCriteria,

    #[error("Fee token {0} is not supported")]
    FeeTokenIsNotSupported(u32),

    #[error("Unexpected error: {0}")]
    UnexpectedError(String),
}
