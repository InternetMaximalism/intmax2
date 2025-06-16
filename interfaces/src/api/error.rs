#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Env error: {0}")]
    EnvError(String),

    #[error("Invalid auth: {0}")]
    InvalidAuth(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Signing error: {0}")]
    SigningError(String),

    #[error("Response error: {0}")]
    ResponseError(String),

    #[error("Unknown error: {0}")]
    UnknownError(String),

    #[error("Serialization error: {0}")]
    SerializeError(String),

    #[error("Response Deserialization error: {0}")]
    ResponseDeserializationError(String),

    #[error("Proof Decode error: {0}")]
    ProofDecodeError(String),

    #[error("Proof verification error: {0}")]
    ProofVerificationError(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("Malformed URL: {0}")]
    MalformedUrl(String),
}
