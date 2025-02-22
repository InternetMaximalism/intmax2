use intmax2_interfaces::{
    api::error::ServerError,
    data::{encryption::errors::BlsEncryptionError, proof_compression::ProofCompressionError},
};

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

    #[error("Encryption error: {0}")]
    EncryptionError(#[from] BlsEncryptionError),

    #[error("Payment memo error: {0}")]
    PaymentMemoError(String),

    #[error("Send tx request error: {0}")]
    SendTxRequestError(String),

    #[error("Failed to get proposal: {0}")]
    FailedToGetProposal(String),

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

    #[error("Block builder fee error: {0}")]
    BlockBuilderFeeError(String),

    #[error("Unexpected error: {0}")]
    UnexpectedError(String),
}
