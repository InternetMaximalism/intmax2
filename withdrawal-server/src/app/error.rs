use intmax2_client_sdk::client::receive_validation::ReceiveValidationError;

#[derive(Debug, thiserror::Error)]
pub enum WithdrawalServerError {
    #[error("Database error {0}")]
    DBError(#[from] sqlx::Error),

    #[error("Receive validation error {0}")]
    ReceiveValidationError(#[from] ReceiveValidationError),

    #[error("Single withdrawal proof verification error")]
    SingleWithdrawalVerificationError,

    #[error("Single claim proof verification error")]
    SingleClaimVerificationError,

    #[error("Invalid fee: {0}")]
    InvalidFee(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Serialization error {0}")]
    SerializationError(String),
}
