use thiserror::Error;

#[derive(Error, Debug)]
pub enum MerkleTreeError {
    #[error("Failed to connect to database: {0}")]
    ConnectionError(#[from] sqlx::Error),

    #[error("Failed to serialize/deserialize data: {0}")]
    SerializationError(#[from] bincode::Error),

    #[error("Invalid path length: {0}")]
    WrongPathLength(u32),

    #[error("Node not found for parent hash: {0}")]
    NodeNotFoundError(String),

    #[error("Leaf not found for hash: {0}")]
    LeafNotFoundError(String),

    #[error("Internal error: {0}")]
    InternalError(String),
}
