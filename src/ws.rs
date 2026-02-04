use agent_client_protocol::{NewSessionRequest, NewSessionResponse};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

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

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(sessions): State<SessionManager>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, sessions))
}

async fn handle_socket(mut socket: WebSocket, sessions: SessionManager) {
    let mut current_session_id: Option<String> = None;

    while let Some(msg) = socket.recv().await {
        let msg = match msg {
            Ok(Message::Text(text)) => text,
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => continue,
        };

        let response = match serde_json::from_str::<ClientMessage>(&msg) {
            Ok(ClientMessage::NewSession(req)) => {
                let session_id = Uuid::new_v4().to_string();
                let state = SessionState {
                    session_id: session_id.clone(),
                    workspace_dir: req.cwd.clone(),
                    created_at: Utc::now(),
                };
                sessions.insert(session_id.clone(), state);
                current_session_id = Some(session_id.clone());

                ServerMessage::NewSession(NewSessionResponse::new(session_id))
            }
            Err(e) => ServerMessage::Error {
                message: format!("Invalid message: {}", e),
            },
        };

        let response_text = serde_json::to_string(&response).unwrap();
        if socket.send(Message::Text(response_text.into())).await.is_err() {
            break;
        }
    }

    // 清理 session
    if let Some(session_id) = current_session_id {
        sessions.remove(&session_id);
        tracing::info!("Session {} cleaned up", session_id);
    }
}
