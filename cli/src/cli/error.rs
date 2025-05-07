use alloy::transports::{RpcError, TransportErrorKind};
use envy::Error as EnvyError;
use intmax2_client_sdk::{
    client::{error::ClientError, sync::error::SyncError},
    external_api::{
        contract::error::BlockchainError, local_backup_store_vault::error::LocalStoreVaultError,
    },
};
use intmax2_interfaces::api::error::ServerError;

use crate::format::FormatTokenInfoError;

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("Envy error:{0}")]
    EnvyError(#[from] EnvyError),

    #[error("RPC error: {0}")]
    RPCError(#[from] RpcError<TransportErrorKind>),

    #[error("Sync error: {0}")]
    SyncError(#[from] SyncError),

    #[error("Client error: {0}")]
    ClientError(#[from] ClientError),

    #[error("Local store vault error: {0}")]
    LocalStoreVaultError(#[from] LocalStoreVaultError),

    #[error("CSV deserialize error: {0}")]
    CSVDeserializeError(#[from] csv::Error),

    #[error("Env error:{0}")]
    EnvError(String),

    #[error("Backup error: {0}")]
    BackupError(String),

    #[error("Too many transfer: {0}")]
    TooManyTransfer(usize),

    #[error("{0}")]
    FormatTokenInfoError(#[from] FormatTokenInfoError),

    #[error("Blockchain error: {0}")]
    BlockchainError(#[from] BlockchainError),

    #[error("InsufficientBalance: {0}")]
    InsufficientBalance(String),

    #[error("Server error: {0}")]
    ServerError(#[from] ServerError),

    #[error("Failed to request tx")]
    FailedToRequestTx,

    #[error("Failed to get proposal")]
    FailedToGetProposal,

    #[error("Unexpected error: {0}")]
    UnexpectedError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Pending tx error")]
    PendingTxError,

    #[error("Tx failed: {0}")]
    TxFailed(String),
}
