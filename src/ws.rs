use agent_client_protocol::{ContentBlock, NewSessionRequest, NewSessionResponse, PromptRequest};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::mpsc;
use tokio_util::codec::{FramedRead, LinesCodec};
use uuid::Uuid;

use crate::claude::process_handle::ProcessHandle;
use crate::claude::transport::transport2::SubprocessCLITransport;
use crate::claude::types::{ClaudeAgentOptions, InputMessage};

// ============================================================================
// Message Types
// ============================================================================

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    NewSession(NewSessionRequest),
    Prompt(PromptRequest),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    NewSession(NewSessionResponse),
    ClaudeEvent { data: serde_json::Value },
    Error { message: String },
}

// ============================================================================
// Session State
// ============================================================================

#[derive(Debug, Clone)]
pub struct SessionState {
    pub session_id: String,
    pub workspace_dir: PathBuf,
    pub created_at: DateTime<Utc>,
}

pub type SessionManager = Arc<DashMap<String, SessionState>>;

// ============================================================================
// WebSocket Handler
// ============================================================================

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(sessions): State<SessionManager>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, sessions))
}

async fn handle_socket(socket: WebSocket, sessions: SessionManager) {
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let mut current_session_id: Option<String> = None;
    let mut claude_stdin: Option<ChildStdin> = None;
    let mut process_handle: Option<ProcessHandle> = None;

    // Channel for sending messages to WebSocket
    let (tx, mut rx) = mpsc::channel::<ServerMessage>(100);

    // Task to send messages to WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let text = serde_json::to_string(&msg).unwrap();
            if ws_sender
                .send(Message::Text(text.into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    // Main message loop
    while let Some(msg) = ws_receiver.next().await {
        let msg = match msg {
            Ok(Message::Text(text)) => text,
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => continue,
        };

        match serde_json::from_str::<ClientMessage>(&msg) {
            Ok(ClientMessage::NewSession(req)) => {
                let session_id = Uuid::new_v4().to_string();

                // Store session state
                let state = SessionState {
                    session_id: session_id.clone(),
                    workspace_dir: req.cwd.clone(),
                    created_at: Utc::now(),
                };
                sessions.insert(session_id.clone(), state);
                current_session_id = Some(session_id.clone());

                // Send NewSessionResponse
                let response = ServerMessage::NewSession(NewSessionResponse::new(session_id.clone()));
                if tx.send(response).await.is_err() {
                    break;
                }

                // Start Claude process
                match start_claude_process(&req, &session_id).await {
                    Ok((stdout, stdin, handle)) => {
                        claude_stdin = Some(stdin);
                        process_handle = Some(handle);

                        // Spawn task to read Claude stdout and forward to WebSocket
                        let tx_clone = tx.clone();
                        let session_id_clone = session_id.clone();
                        tokio::spawn(async move {
                            forward_claude_output(stdout, tx_clone, session_id_clone).await;
                        });

                        tracing::info!("Claude process started for session {}", session_id);
                    }
                    Err(e) => {
                        let error = ServerMessage::Error {
                            message: format!("Failed to start Claude: {}", e),
                        };
                        let _ = tx.send(error).await;
                    }
                }
            }
            Ok(ClientMessage::Prompt(req)) => {
                if let (Some(stdin), Some(session_id)) =
                    (&mut claude_stdin, &current_session_id)
                {
                    // Convert ContentBlocks to string for Claude input
                    let content = prompt_to_string(&req.prompt);
                    let input_msg = InputMessage::user(&content, session_id.clone());
                    let json = serde_json::to_string(&input_msg).unwrap() + "\n";
                    if let Err(e) = stdin.write_all(json.as_bytes()).await {
                        let error = ServerMessage::Error {
                            message: format!("Failed to send to Claude: {}", e),
                        };
                        let _ = tx.send(error).await;
                    }
                } else {
                    let error = ServerMessage::Error {
                        message: "No active session".to_string(),
                    };
                    let _ = tx.send(error).await;
                }
            }
            Err(e) => {
                let error = ServerMessage::Error {
                    message: format!("Invalid message: {}", e),
                };
                let _ = tx.send(error).await;
            }
        }
    }

    // Cleanup
    drop(tx);
    send_task.abort();

    // Kill Claude process if running
    if let Some(mut handle) = process_handle {
        let _ = handle.kill().await;
    }

    // Remove session
    if let Some(session_id) = current_session_id {
        sessions.remove(&session_id);
        tracing::info!("Session {} cleaned up", session_id);
    }
}

// ============================================================================
// Claude Process Management
// ============================================================================

async fn start_claude_process(
    req: &NewSessionRequest,
    session_id: &str,
) -> Result<(ChildStdout, ChildStdin, ProcessHandle), String> {
    let mut options = ClaudeAgentOptions::new();
    options.cwd = Some(req.cwd.clone());

    // Add session ID to extra args
    options
        .extra_args
        .insert("session-id".to_string(), Some(session_id.to_string()));

    let mut transport =
        SubprocessCLITransport::new(options).map_err(|e| format!("Transport error: {}", e))?;

    transport
        .connect()
        .await
        .map_err(|e| format!("Connect error: {}", e))?;

    let (stdout, stdin, _stderr, handle) =
        transport.split().map_err(|e| format!("Split error: {}", e))?;

    Ok((stdout, stdin, handle))
}

async fn forward_claude_output(stdout: ChildStdout, tx: mpsc::Sender<ServerMessage>, session_id: String) {
    let mut reader = FramedRead::new(stdout, LinesCodec::new());

    while let Some(result) = reader.next().await {
        match result {
            Ok(line) => {
                if line.is_empty() {
                    continue;
                }

                // Parse as JSON and forward as ClaudeEvent
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&line) {
                    let msg = ServerMessage::ClaudeEvent { data };
                    if tx.send(msg).await.is_err() {
                        break;
                    }
                }
            }
            Err(e) => {
                tracing::error!("Error reading Claude output: {}", e);
                break;
            }
        }
    }

    tracing::info!("Claude output stream ended for session {}", session_id);
}

// ============================================================================
// Helper Functions
// ============================================================================

fn prompt_to_string(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text_content) => Some(text_content.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
