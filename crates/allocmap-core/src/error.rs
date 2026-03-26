use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("Invalid .amr file: {0}")]
    InvalidRecording(String),

    #[error("Unsupported .amr version: expected {expected}, got {got}")]
    UnsupportedVersion { expected: u32, got: u32 },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),
}
