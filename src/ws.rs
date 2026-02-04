use agent_client_protocol::{NewSessionRequest, NewSessionResponse};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    NewSession(NewSessionRequest),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    NewSession(NewSessionResponse),
    Error { message: String },
}

#[derive(Debug, Clone)]
pub struct SessionState {
    pub session_id: String,
    pub workspace_dir: PathBuf,
    pub created_at: DateTime<Utc>,
}

pub type SessionManager = Arc<DashMap<String, SessionState>>;
