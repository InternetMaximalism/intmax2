use crate::app::error::FeeError;

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("Failed to add signature: {0}")]
    AddSignatureError(String),

    #[error("Fee error: {0}")]
    FeeError(#[from] FeeError),
    
    #[error("Redis error: {0}")]
    RedisError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Deserialization error: {0}")]
    DeserializationError(String),
}
