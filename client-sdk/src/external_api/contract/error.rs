use alloy::transports::{RpcError, TransportErrorKind};
use ethers::types::H256;

#[derive(Debug, thiserror::Error)]
pub enum BlockchainError {
    #[error("Contract error: {0}")]
    ContractError(#[from] alloy::contract::Error),

    #[error("Insufficient funds: {0}")]
    InsufficientFunds(String),

    #[error("Transaction failed: {0}")]
    TransactionFailed(String),

    // RPCError(String),
    #[error("RPC error: {0}")]
    RPCError(#[from] RpcError<TransportErrorKind>),

    #[error("Join error: {0}")]
    JoinError(String),

    #[error("Decode call data error: {0}")]
    DecodeCallDataError(String),

    #[error("Token not found")]
    TokenNotFound,

    #[error("Block base fee not found")]
    BlockBaseFeeNotFound,

    #[error("Transaction not found: {0:?}")]
    TxNotFound(H256),

    #[error("Transaction not found in batch")]
    TxNotFoundBatch,

    #[error("Transaction error: {0}")]
    TransactionError(String),

    #[error("Max tx retries reached")]
    MaxTxRetriesReached,

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Env error: {0}")]
    EnvError(String),
}
