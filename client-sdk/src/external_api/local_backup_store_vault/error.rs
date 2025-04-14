#[derive(Debug, Clone, thiserror::Error)]
pub enum IOError {
    #[error("Failed to create directory: {0}")]
    CreateDirAllError(String),
    #[error("Read error: {0}")]
    ReadError(String),
    #[error("Write error: {0}")]
    WriteError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
}
