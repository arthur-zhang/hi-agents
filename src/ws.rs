//! WebSocket handler for ACP protocol

use crate::agent::backends::create_backend;
use crate::agent::{AgentBackend, AgentError, AgentType, TurnCompletion};

use agent_client_protocol::{
    AgentSide, ClientRequest, NewSessionResponse, PromptResponse, Side,
};
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
use serde_json::value::RawValue;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

// ============================================================================
// JSON-RPC Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    String(String),
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: RequestId,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: RequestId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: &'static str,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcResponse {
    pub fn success(id: RequestId, result: impl Serialize) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(serde_json::to_value(result).unwrap()),
            error: None,
        }
    }

    pub fn error(id: RequestId, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

impl JsonRpcNotification {
    pub fn new(method: impl Into<String>, params: impl Serialize) -> Self {
        Self {
            jsonrpc: "2.0",
            method: method.into(),
            params: Some(serde_json::to_value(params).unwrap()),
        }
    }
}

// JSON-RPC Error Codes
const PARSE_ERROR: i32 = -32700;
const INVALID_REQUEST: i32 = -32600;
const METHOD_NOT_FOUND: i32 = -32601;
const INTERNAL_ERROR: i32 = -32603;
const AUTH_REQUIRED: i32 = -32001;

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
// Outgoing Message
// ============================================================================

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum OutgoingMessage {
    Response(JsonRpcResponse),
    Notification(JsonRpcNotification),
}

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
    let mut backend: Option<Box<dyn AgentBackend>> = None;

    // Channel for sending messages to WebSocket
    let (tx, mut rx) = mpsc::channel::<OutgoingMessage>(100);

    // Task to send messages to WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let text = serde_json::to_string(&msg).unwrap();
            if ws_sender.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    // Main message loop
    while let Some(msg) = ws_receiver.next().await {
        println!("received message: {:?}", msg);
        let msg = match msg {
            Ok(Message::Text(text)) => text,
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => continue,
        };

        // Parse JSON-RPC request
        let request: JsonRpcRequest = match serde_json::from_str(&msg) {
            Ok(req) => req,
            Err(e) => {
                let error = JsonRpcResponse {
                    jsonrpc: "2.0",
                    id: RequestId::Number(0),
                    result: None,
                    error: Some(JsonRpcError {
                        code: PARSE_ERROR,
                        message: format!("Parse error: {}", e),
                        data: None,
                    }),
                };
                let _ = tx.send(OutgoingMessage::Response(error)).await;
                continue;
            }
        };

        // Parse into ClientRequest using AgentSide::decode_request
        let raw_params = request.params.to_string();
        let raw_value = RawValue::from_string(raw_params).ok();
        let client_request = match AgentSide::decode_request(&request.method, raw_value.as_deref())
        {
            Ok(req) => req,
            Err(e) => {
                let code = if e.message.contains("Method not found") {
                    METHOD_NOT_FOUND
                } else {
                    INVALID_REQUEST
                };
                let _ = tx
                    .send(OutgoingMessage::Response(JsonRpcResponse::error(
                        request.id, code, e.message,
                    )))
                    .await;
                continue;
            }
        };

        // Route by ClientRequest variant
        match client_request {
            ClientRequest::NewSessionRequest(req) => {
                let session_id = Uuid::new_v4().to_string();

                // Store session state
                let state = SessionState {
                    session_id: session_id.clone(),
                    workspace_dir: req.cwd.clone(),
                    created_at: Utc::now(),
                };
                sessions.insert(session_id.clone(), state);
                current_session_id = Some(session_id.clone());

                // Create agent backend (can be extended to support different agent types)
                let agent_type = AgentType::Claude; // TODO: read from request metadata
                let mut agent = create_backend(agent_type);

                // Start session
                match agent.start_session(&req, &session_id).await {
                    Ok(()) => {
                        backend = Some(agent);
                        let response = NewSessionResponse::new(session_id);
                        let _ = tx
                            .send(OutgoingMessage::Response(JsonRpcResponse::success(
                                request.id, response,
                            )))
                            .await;
                    }
                    Err(e) => {
                        let _ = tx
                            .send(OutgoingMessage::Response(JsonRpcResponse::error(
                                request.id,
                                INTERNAL_ERROR,
                                format!("Failed to start agent: {}", e),
                            )))
                            .await;
                    }
                }
            }
            ClientRequest::PromptRequest(req) => {
                let Some(agent) = backend.as_mut() else {
                    let _ = tx
                        .send(OutgoingMessage::Response(JsonRpcResponse::error(
                            request.id,
                            INTERNAL_ERROR,
                            "No active session",
                        )))
                        .await;
                    continue;
                };
                let Some(session_id) = &current_session_id else {
                    let _ = tx
                        .send(OutgoingMessage::Response(JsonRpcResponse::error(
                            request.id,
                            INTERNAL_ERROR,
                            "No active session",
                        )))
                        .await;
                    continue;
                };

                println!("sending prompt to agent");

                // Execute turn
                match agent.execute_turn(&req, session_id).await {
                    Ok(turn_result) => {
                        // Stream notifications to client
                        let mut stream = turn_result.notifications;
                        while let Some(notification) = stream.next().await {
                            let json_notification =
                                JsonRpcNotification::new("session/update", notification);
                            if tx
                                .send(OutgoingMessage::Notification(json_notification))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }

                        let completion = agent.take_completion();
                        println!("end of sending prompt to agent");

                        // Send response
                        let response = match completion {
                            TurnCompletion::Stop(stop_reason) => {
                                println!("prompt completed: {:?}", stop_reason);
                                JsonRpcResponse::success(request.id, PromptResponse::new(stop_reason))
                            }
                            TurnCompletion::Error(err) => {
                                let (code, message) = match err {
                                    AgentError::AuthRequired => {
                                        (AUTH_REQUIRED, "Authentication required".to_string())
                                    }
                                    _ => (INTERNAL_ERROR, err.to_string()),
                                };
                                JsonRpcResponse::error(request.id, code, message)
                            }
                        };

                        if tx.send(OutgoingMessage::Response(response)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = tx
                            .send(OutgoingMessage::Response(JsonRpcResponse::error(
                                request.id,
                                INTERNAL_ERROR,
                                format!("Failed to execute turn: {}", e),
                            )))
                            .await;
                    }
                }
            }
            _ => {
                let response = JsonRpcResponse::error(
                    request.id,
                    METHOD_NOT_FOUND,
                    format!("Method not implemented: {}", request.method),
                );
                if tx.send(OutgoingMessage::Response(response)).await.is_err() {
                    break;
                }
            }
        }
    }

    // Cleanup
    drop(tx);
    send_task.abort();

    // Kill agent process if running
    if let Some(mut agent) = backend {
        let _ = agent.kill().await;
    }

    // Remove session
    if let Some(session_id) = current_session_id {
        sessions.remove(&session_id);
        tracing::info!("Session {} cleaned up", session_id);
    }
}
