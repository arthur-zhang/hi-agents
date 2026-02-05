//! Core traits for agent backends

use agent_client_protocol::{NewSessionRequest, PromptRequest, SessionNotification, StopReason};
use async_trait::async_trait;
use futures::Stream;
use std::any::Any;
use std::fmt;
use std::pin::Pin;

/// Agent backend error types
#[derive(Debug)]
pub enum AgentError {
    /// Authentication required
    AuthRequired,
    /// Internal error
    Internal(String),
    /// Process error
    Process(String),
    /// IO error
    Io(std::io::Error),
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentError::AuthRequired => write!(f, "Authentication required"),
            AgentError::Internal(msg) => write!(f, "Internal error: {}", msg),
            AgentError::Process(msg) => write!(f, "Process error: {}", msg),
            AgentError::Io(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for AgentError {}

impl From<std::io::Error> for AgentError {
    fn from(e: std::io::Error) -> Self {
        AgentError::Io(e)
    }
}

pub type AgentResult<T> = Result<T, AgentError>;

/// Turn completion result
#[derive(Debug)]
pub enum TurnCompletion {
    /// Normal completion
    Stop(StopReason),
    /// Error completion
    Error(AgentError),
}

/// ACP notification stream type
pub type NotificationStream = Pin<Box<dyn Stream<Item = SessionNotification> + Send>>;

/// Result of executing a turn
pub struct TurnResult {
    /// Stream of ACP notifications
    pub notifications: NotificationStream,
}

/// Core trait for agent backends
///
/// Each agent implementation is responsible for:
/// 1. Starting and managing its own process
/// 2. Converting native messages to ACP SessionNotification
#[async_trait]
pub trait AgentBackend: Send {
    /// Start an agent session
    async fn start_session(
        &mut self,
        req: &NewSessionRequest,
        session_id: &str,
    ) -> AgentResult<()>;

    /// Execute a turn, returning a stream of ACP notifications
    ///
    /// The returned TurnResult contains:
    /// - A stream of SessionNotification that should be forwarded to the client
    /// - Call `finish_turn` after the stream ends to get completion status
    async fn execute_turn(
        &mut self,
        req: &PromptRequest,
        session_id: &str,
    ) -> AgentResult<TurnResult>;

    /// Get the completion status of the current turn
    ///
    /// Call this after the notification stream ends
    fn take_completion(&mut self) -> TurnCompletion;

    /// Kill the agent process
    async fn kill(&mut self) -> AgentResult<()>;

    /// Get mutable reference as Any for downcasting
    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        None
    }
}

/// Agent type enumeration
#[derive(Debug, Clone, Default)]
pub enum AgentType {
    #[default]
    Claude,
    Codex,
    OpenCode,
}

impl AgentType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "opencode" => Some(Self::OpenCode),
            _ => None,
        }
    }
}
