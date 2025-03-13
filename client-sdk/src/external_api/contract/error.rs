use ethers::types::H256;

#[derive(Debug, thiserror::Error)]
pub enum BlockchainError {
    #[error("Insufficient funds: {0}")]
    InsufficientFunds(String),

    #[error("Transaction failed: {0}")]
    TransactionFailed(String),

    #[error("RPC error: {0}")]
    RPCError(String),

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

    #[error("Transaction error: {0}")]
    TransactionError(String),

    #[error("Max tx retries reached")]
    MaxTxRetriesReached,

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Env error: {0}")]
    EnvError(String),
}
