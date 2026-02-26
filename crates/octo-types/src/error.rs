use thiserror::Error;

#[derive(Debug, Error)]
pub enum OctoError {
    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Tool execution error: {0}")]
    ToolExecution(String),

    #[error("Sandbox error: {0}")]
    Sandbox(String),

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Max rounds exceeded: {0}")]
    MaxRoundsExceeded(u32),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("WebSocket error: {0}")]
    WebSocket(String),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}
