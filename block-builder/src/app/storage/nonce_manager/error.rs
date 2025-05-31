#[derive(Debug, thiserror::Error)]
pub enum NonceError {
    #[error("Redis error: {0}")]
    RedisError(#[from] redis::RedisError),

    #[error("Serialization/Deserialization error: {0}")]
    SerdeError(#[from] serde_json::Error),
}
