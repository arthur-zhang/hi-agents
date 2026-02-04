use agent_client_protocol::{AgentSide, ClientRequest, ContentBlock as AcpContentBlock, NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse, Side, StopReason};
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
use std::sync::atomic::AtomicI64;
use tokio::io::AsyncWriteExt;
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::mpsc;
use tokio_util::codec::{FramedRead, LinesCodec};
use uuid::Uuid;

use crate::claude::process_handle::ProcessHandle;
use crate::claude::transport::transport2::SubprocessCLITransport;
use crate::claude::types::{ClaudeAgentOptions, ContentBlock, InputMessage, ProtocolMessage};

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
// Outgoing Message (Response or Notification)
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
    let mut claude_stdin: Option<ChildStdin> = None;
    let mut process_handle: Option<ProcessHandle> = None;
    let notification_id = Arc::new(AtomicI64::new(0));

    // Channel for sending messages to WebSocket
    let (tx, mut rx) = mpsc::channel::<OutgoingMessage>(100);

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
        let client_request = match AgentSide::decode_request(&request.method, raw_value.as_deref()) {
            Ok(req) => req,
            Err(e) => {
                let code = if e.message.contains("Method not found") { METHOD_NOT_FOUND } else { INVALID_REQUEST };
                let _ = tx.send(OutgoingMessage::Response(
                    JsonRpcResponse::error(request.id, code, e.message)
                )).await;
                continue;
            }
        };

        // Route by ClientRequest variant
        let response = match client_request {
            ClientRequest::NewSessionRequest(req) => {
                handle_session_new(request.id.clone(), req, &sessions, &mut current_session_id, &mut claude_stdin, &mut process_handle, &tx, &notification_id).await
            }
            ClientRequest::PromptRequest(req) => {
                handle_session_prompt(request.id.clone(), req, &mut claude_stdin, &current_session_id).await
            }
            _ => {
                JsonRpcResponse::error(request.id, METHOD_NOT_FOUND, format!("Method not implemented: {}", request.method))
            }
        };

        if tx.send(OutgoingMessage::Response(response)).await.is_err() {
            break;
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
// Request Handlers
// ============================================================================

async fn handle_session_new(
    id: RequestId,
    req: NewSessionRequest,
    sessions: &SessionManager,
    current_session_id: &mut Option<String>,
    claude_stdin: &mut Option<ChildStdin>,
    process_handle: &mut Option<ProcessHandle>,
    tx: &mpsc::Sender<OutgoingMessage>,
    notification_id: &Arc<AtomicI64>,
) -> JsonRpcResponse {
    let session_id = Uuid::new_v4().to_string();

    // Store session state
    let state = SessionState {
        session_id: session_id.clone(),
        workspace_dir: req.cwd.clone(),
        created_at: Utc::now(),
    };
    sessions.insert(session_id.clone(), state);
    *current_session_id = Some(session_id.clone());

    // Start Claude process
    match start_claude_process(&req, &session_id).await {
        Ok((stdout, stdin, handle)) => {
            *claude_stdin = Some(stdin);
            *process_handle = Some(handle);

            // Spawn task to read Claude stdout and forward as notifications
            let tx_clone = tx.clone();
            let session_id_clone = session_id.clone();
            let notification_id_clone = notification_id.clone();
            tokio::spawn(async move {
                forward_claude_output(stdout, tx_clone, session_id_clone, notification_id_clone).await;
            });

            tracing::info!("Claude process started for session {}", session_id);
        }
        Err(e) => {
            return JsonRpcResponse::error(id.clone(), INTERNAL_ERROR, format!("Failed to start Claude: {}", e));
        }
    }

    // Return NewSessionResponse
    let response = NewSessionResponse::new(session_id);
    JsonRpcResponse::success(id, response)
}

async fn handle_session_prompt(
    id: RequestId,
    req: PromptRequest,
    claude_stdin: &mut Option<ChildStdin>,
    current_session_id: &Option<String>,
) -> JsonRpcResponse {
    if let (Some(stdin), Some(session_id)) = (claude_stdin, current_session_id) {
        // Convert ContentBlocks to string for Claude input
        let content = prompt_to_string(&req.prompt);
        let input_msg = InputMessage::user(&content, session_id.clone());
        let json = serde_json::to_string(&input_msg).unwrap() + "\n";

        if let Err(e) = stdin.write_all(json.as_bytes()).await {
            return JsonRpcResponse::error(id.clone(), INTERNAL_ERROR, format!("Failed to send to Claude: {}", e));
        }

        // Return PromptResponse (actual response will come via notifications)
        let response = PromptResponse::new(StopReason::EndTurn);
        JsonRpcResponse::success(id, response)
    } else {
        JsonRpcResponse::error(id, INVALID_REQUEST, "No active session")
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

async fn forward_claude_output(
    stdout: ChildStdout,
    tx: mpsc::Sender<OutgoingMessage>,
    session_id: String,
    _notification_id: Arc<AtomicI64>,
) {
    let mut reader = FramedRead::new(stdout, LinesCodec::new());

    while let Some(result) = reader.next().await {
        match result {
            Ok(line) => {
                if line.is_empty() {
                    continue;
                }

                // Parse as ProtocolMessage and convert to SessionNotification
                match serde_json::from_str::<ProtocolMessage>(&line) {
                    Ok(msg) => {
                        let notifications = protocol_message_to_notifications(&session_id, msg);
                        for notification in notifications {
                            let json_notification = JsonRpcNotification::new(
                                "session/update",
                                notification,
                            );
                            if tx.send(OutgoingMessage::Notification(json_notification)).await.is_err() {
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse ProtocolMessage: {} - line: {}", e, &line);
                        // Fallback: send raw data
                        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&line) {
                            let notification = JsonRpcNotification::new(
                                "session/update",
                                serde_json::json!({
                                    "sessionId": session_id,
                                    "update": {
                                        "sessionUpdate": "raw",
                                        "data": data
                                    }
                                }),
                            );
                            if tx.send(OutgoingMessage::Notification(notification)).await.is_err() {
                                break;
                            }
                        }
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
// Protocol Message to ACP Notification Conversion
// ============================================================================

/// Convert ProtocolMessage to JSON notifications following ACP SessionNotification format
fn protocol_message_to_notifications(session_id: &str, msg: ProtocolMessage) -> Vec<serde_json::Value> {
    let mut notifications = Vec::new();

    println!("----------");
    println!("{:#?}", msg);
    println!("----------");
    match msg {
        ProtocolMessage::Assistant { message, .. } => {
            // Process assistant message content blocks
            for block in message.content {
                match block {
                    ContentBlock::Text { text } => {
                        notifications.push(serde_json::json!({
                            "sessionId": session_id,
                            "update": {
                                "sessionUpdate": "agent_message_chunk",
                                "content": {
                                    "type": "text",
                                    "text": text
                                }
                            }
                        }));
                    }
                    ContentBlock::Thinking { thinking, .. } => {
                        notifications.push(serde_json::json!({
                            "sessionId": session_id,
                            "update": {
                                "sessionUpdate": "agent_thought_chunk",
                                "content": {
                                    "type": "text",
                                    "text": thinking
                                }
                            }
                        }));
                    }
                    ContentBlock::ToolUse { id, name, input } => {
                        notifications.push(serde_json::json!({
                            "sessionId": session_id,
                            "update": {
                                "sessionUpdate": "tool_call",
                                "toolCallId": id,
                                "title": name,
                                "status": "pending",
                                "rawInput": input
                            }
                        }));
                    }
                    ContentBlock::ToolResult { tool_use_id, content, is_error } => {
                        let status = if is_error.unwrap_or(false) { "failed" } else { "completed" };
                        let raw_output = content.map(|c| match c {
                            crate::claude::types::ContentBlockContent::String(s) => serde_json::Value::String(s),
                            crate::claude::types::ContentBlockContent::Array(arr) => serde_json::Value::Array(arr),
                        });
                        notifications.push(serde_json::json!({
                            "sessionId": session_id,
                            "update": {
                                "sessionUpdate": "tool_call_update",
                                "toolCallId": tool_use_id,
                                "status": status,
                                "rawOutput": raw_output
                            }
                        }));
                    }
                }
            }
        }
        ProtocolMessage::Stream(stream_event) => {
            // Handle stream events (partial updates)
            if let Some(event) = stream_event.event.as_object() {
                if let Some(event_type) = event.get("type").and_then(|v| v.as_str()) {
                    match event_type {
                        "content_block_start" | "content_block_delta" => {
                            // Extract content from the event
                            let content_block = event.get("content_block").or_else(|| event.get("delta"));
                            if let Some(block) = content_block {
                                if let Some(block_type) = block.get("type").and_then(|v| v.as_str()) {
                                    match block_type {
                                        "text" | "text_delta" => {
                                            if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                                                notifications.push(serde_json::json!({
                                                    "sessionId": session_id,
                                                    "update": {
                                                        "sessionUpdate": "agent_message_chunk",
                                                        "content": {
                                                            "type": "text",
                                                            "text": text
                                                        }
                                                    }
                                                }));
                                            }
                                        }
                                        "thinking" | "thinking_delta" => {
                                            if let Some(thinking) = block.get("thinking").and_then(|v| v.as_str()) {
                                                notifications.push(serde_json::json!({
                                                    "sessionId": session_id,
                                                    "update": {
                                                        "sessionUpdate": "agent_thought_chunk",
                                                        "content": {
                                                            "type": "text",
                                                            "text": thinking
                                                        }
                                                    }
                                                }));
                                            }
                                        }
                                        "tool_use" => {
                                            if let (Some(id), Some(name)) = (
                                                block.get("id").and_then(|v| v.as_str()),
                                                block.get("name").and_then(|v| v.as_str()),
                                            ) {
                                                let input = block.get("input").cloned().unwrap_or(serde_json::Value::Null);
                                                notifications.push(serde_json::json!({
                                                    "sessionId": session_id,
                                                    "update": {
                                                        "sessionUpdate": "tool_call",
                                                        "toolCallId": id,
                                                        "title": name,
                                                        "status": "pending",
                                                        "rawInput": input
                                                    }
                                                }));
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        ProtocolMessage::Result(result) => {
            // Result messages indicate end of turn - we don't need to send notifications for these
            // The PromptResponse will be sent separately
            tracing::debug!("Result message: subtype={}, is_error={}", result.subtype, result.is_error);
        }
        ProtocolMessage::System(system) => {
            // System messages are mostly internal - log them but don't forward
            tracing::debug!("System message: subtype={}", system.subtype);
        }
        ProtocolMessage::User { .. } => {
            // User messages are echoes of input - typically don't need to forward
        }
        ProtocolMessage::ControlRequest { .. } | ProtocolMessage::ControlResponse { .. } => {
            // Control messages are internal protocol - don't forward
        }
    }

    notifications
}

// ============================================================================
// Helper Functions
// ============================================================================

fn prompt_to_string(blocks: &[AcpContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            AcpContentBlock::Text(text_content) => Some(text_content.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
