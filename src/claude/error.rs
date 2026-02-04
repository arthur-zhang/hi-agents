//! Error types for Claude Agent SDK.

use thiserror::Error;
use tokio_util::codec::LinesCodecError;

/// Main error type for the Claude Agent SDK.
#[derive(Error, Debug)]
pub enum Error {
    /// Serialization or deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Permission denied error.
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Invalid configuration error.
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// Hook execution failed.
    #[error("Hook execution failed: {0}")]
    HookFailed(String),

    /// Tool execution failed.
    #[error("Tool execution failed: {0}")]
    ToolFailed(String),

    /// SDK control protocol error.
    #[error("SDK control protocol error: {0}")]
    ControlProtocol(String),

    /// Claude CLI not found.
    #[error("Claude CLI not found: {0}")]
    CLINotFound(String),

    /// CLI connection error.
    #[error("CLI connection error: {0}")]
    CLIConnection(String),

    /// Message parse error.
    #[error("Message parse error: {0}")]
    MessageParse(String),

    /// Process error.
    #[error("Process error: {0}")]
    Process(String),

    /// Timeout error.
    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("{0}")]
    Codec(#[from] LinesCodecError),

    /// Unknown error.
    #[error("Unknown error: {0}")]
    Unknown(String),
}

/// Result type alias for the Claude Agent SDK.
pub type Result<T> = std::result::Result<T, Error>;
