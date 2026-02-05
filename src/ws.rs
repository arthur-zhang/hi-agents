use agent_client_protocol::{
    AgentSide, ClientRequest, ContentBlock as AcpContentBlock, ContentChunk, NewSessionRequest,
    NewSessionResponse, PromptRequest, PromptResponse, SessionNotification, SessionUpdate, Side,
    StopReason, ToolCall, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields,
};
use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
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
use tokio::io::AsyncWriteExt;
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::claude::transport::transport2::SubprocessCLITransport;
use crate::claude::types::{
    ClaudeAgentOptions, ContentBlock, ContentBlockContent, ImageSource, InputMessage,
    ProtocolMessage,
};
use crate::claude::{process_handle::ProcessHandle, transport::stream::into_event_stream};

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
const AUTH_REQUIRED: i32 = -32001; // Custom error code for authentication required

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

#[derive(Debug)]
enum PromptError {
    /// Authentication required - maps to RequestError.authRequired()
    AuthRequired,
    /// Internal error - maps to RequestError.internalError(undefined, message)
    Internal { message: String },
}

#[derive(Debug)]
enum PromptCompletion {
    Stop(StopReason),
    Error(PromptError),
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

/// Result of a single Turn execution
struct TurnResult {
    /// The stdout returned for reuse in the next turn
    stdout: ChildStdout,
    /// How the turn completed
    completion: PromptCompletion,
    /// Updated tool use cache
    tool_use_cache: ToolUseCache,
}

/// Execute a single turn: send prompt, process output until Result message
async fn execute_turn(
    req: &PromptRequest,
    stdin: &mut ChildStdin,
    stdout: ChildStdout,
    session_id: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
    mut tool_use_cache: ToolUseCache,
) -> TurnResult {
    // Send prompt to Claude stdin
    let content = prompt_to_string(&req.prompt);
    let input_msg = InputMessage::user(&content, session_id.to_string());
    let json = serde_json::to_string(&input_msg).unwrap() + "\n";

    if let Err(e) = stdin.write_all(json.as_bytes()).await {
        return TurnResult {
            stdout,
            completion: PromptCompletion::Error(PromptError::Internal {
                message: format!("Failed to send to Claude: {}", e),
            }),
            tool_use_cache,
        };
    }

    // Process stdout until we get a Result message
    let mut stream = into_event_stream(stdout);

    while let Some(result) = stream.next().await {
        match result {
            Ok(msg) => {
                match handle_msg(tx.clone(), &session_id.to_string(), &msg, &mut tool_use_cache)
                    .await
                {
                    Ok(Some(prompt_response)) => {
                        println!(
                            "prompt_response: {}",
                            serde_json::to_string(&prompt_response).unwrap()
                        );
                        // Turn complete - return stdout for reuse
                        let stdout = stream.into_inner();
                        return TurnResult {
                            stdout,
                            completion: PromptCompletion::Stop(prompt_response.stop_reason),
                            tool_use_cache,
                        };
                    }
                    Ok(None) => {
                        // Continue processing
                    }
                    Err(err) => {
                        let stdout = stream.into_inner();
                        return TurnResult {
                            stdout,
                            completion: PromptCompletion::Error(err),
                            tool_use_cache,
                        };
                    }
                }
            }
            Err(e) => {
                tracing::error!("Error reading Claude output: {}", e);
                let stdout = stream.into_inner();
                return TurnResult {
                    stdout,
                    completion: PromptCompletion::Error(PromptError::Internal {
                        message: format!("Error reading Claude output: {}", e),
                    }),
                    tool_use_cache,
                };
            }
        }
    }

    // Stream ended unexpectedly
    tracing::warn!("Claude output stream ended unexpectedly for session {}", session_id);
    let stdout = stream.into_inner();
    TurnResult {
        stdout,
        completion: PromptCompletion::Stop(StopReason::EndTurn),
        tool_use_cache,
    }
}

async fn handle_socket(socket: WebSocket, sessions: SessionManager) {
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let mut current_session_id: Option<String> = None;
    let mut claude_stdin: Option<ChildStdin> = None;
    let mut claude_stdout: Option<ChildStdout> = None;
    let mut process_handle: Option<ProcessHandle> = None;
    let mut tool_use_cache: ToolUseCache = ToolUseCache::new();

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
                let response = handle_session_new(
                    request.id.clone(),
                    req,
                    &sessions,
                    &mut current_session_id,
                    &mut claude_stdin,
                    &mut claude_stdout,
                    &mut process_handle,
                )
                .await;
                if tx.send(OutgoingMessage::Response(response)).await.is_err() {
                    break;
                }
            }
            ClientRequest::PromptRequest(req) => {
                // Check if we have an active session
                let (Some(stdin), Some(stdout), Some(session_id)) =
                    (claude_stdin.as_mut(), claude_stdout.take(), &current_session_id)
                else {
                    let _ = tx
                        .send(OutgoingMessage::Response(JsonRpcResponse::error(
                            request.id,
                            INTERNAL_ERROR,
                            "No active session",
                        )))
                        .await;
                    continue;
                };

                println!("sending prompt to claude");

                // Execute the turn - stdout ownership transfers in and out
                let turn_result =
                    execute_turn(&req, stdin, stdout, session_id, &tx, tool_use_cache).await;

                // Restore stdout for next turn
                claude_stdout = Some(turn_result.stdout);
                tool_use_cache = turn_result.tool_use_cache;

                println!("end of sending prompt to claude");

                // Handle completion
                let response = match turn_result.completion {
                    PromptCompletion::Stop(stop_reason) => {
                        println!("prompt completed: {:?}", stop_reason);
                        JsonRpcResponse::success(request.id, PromptResponse::new(stop_reason))
                    }
                    PromptCompletion::Error(err) => {
                        let (code, message) = match err {
                            PromptError::AuthRequired => {
                                (AUTH_REQUIRED, "Authentication required".to_string())
                            }
                            PromptError::Internal { message } => (INTERNAL_ERROR, message),
                        };
                        JsonRpcResponse::error(request.id, code, message)
                    }
                };

                if tx.send(OutgoingMessage::Response(response)).await.is_err() {
                    break;
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
    claude_stdout: &mut Option<ChildStdout>,
    process_handle: &mut Option<ProcessHandle>,
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
            *claude_stdout = Some(stdout);
            *process_handle = Some(handle);

            tracing::info!("Claude process started for session {}", session_id);
        }
        Err(e) => {
            return JsonRpcResponse::error(
                id.clone(),
                INTERNAL_ERROR,
                format!("Failed to start Claude: {}", e),
            );
        }
    }

    // Return NewSessionResponse
    let response = NewSessionResponse::new(session_id);
    JsonRpcResponse::success(id, response)
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
    options.max_thinking_tokens = Some(50000);
    options.include_partial_messages = true;

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

    let (stdout, stdin, _stderr, handle) = transport
        .split()
        .map_err(|e| format!("Split error: {}", e))?;

    Ok((stdout, stdin, handle))
}

async fn handle_msg(
    tx: mpsc::Sender<OutgoingMessage>,
    session_id: &String,
    msg: &ProtocolMessage,
    tool_use_cache: &mut ToolUseCache,
) -> Result<Option<PromptResponse>, PromptError> {
    println!("===========");
    println!("handle_msg: {:?}", serde_json::to_string(&msg).unwrap());
    println!("===========");
    match &msg {
        ProtocolMessage::System(system_message) => match system_message.subtype.as_str() {
            "init" => {
                return Ok(None);
            }
            "compact_boundary" | "hook_started" | "task_notification" | "hook_progress"
            | "hook_response" | "status" | "files_persisted" => {
                return Ok(None);
            }
            _ => {
                tracing::warn!("Unknown system message subtype: {:?}", system_message);
                return Ok(None);
            }
        },
        ProtocolMessage::Result(result_message) => {
            let result_text = result_message.result.clone().unwrap_or_default();
            let errors_text = result_message.errors.join(", ");

            match result_message.subtype.as_str() {
                "success" => {
                    if result_text.contains("Please run /login") {
                        return Err(PromptError::AuthRequired);
                    }
                    if result_message.is_error {
                        return Err(PromptError::Internal {
                            message: result_text,
                        });
                    }
                    return Ok(Some(PromptResponse::new(StopReason::EndTurn)));
                }
                "error_during_execution" => {
                    if result_message.is_error {
                        let message = if errors_text.is_empty() {
                            &result_message.subtype
                        } else {
                            &errors_text
                        };
                        return Err(PromptError::Internal {
                            message: message.clone(),
                        });
                    }
                    return Ok(Some(PromptResponse::new(StopReason::EndTurn)));
                }
                "error_max_budget_usd"
                | "error_max_turns"
                | "error_max_structured_output_retries" => {
                    if result_message.is_error {
                        let message = if errors_text.is_empty() {
                            &result_message.subtype
                        } else {
                            &errors_text
                        };
                        return Err(PromptError::Internal {
                            message: message.clone(),
                        });
                    }
                    return Ok(Some(PromptResponse::new(StopReason::MaxTurnRequests)));
                }
                _ => {
                    tracing::warn!("Unknown result subtype: {}", result_message.subtype);
                    return Ok(None);
                }
            }
        }
        ProtocolMessage::Stream(stream_event) => {
            // Stream events - convert to notifications and send
            let notifications =
                stream_event_to_acp_notifications(stream_event, session_id, tool_use_cache);

            for notification in notifications {
                let json_notification = JsonRpcNotification::new("session/update", notification);
                if tx
                    .send(OutgoingMessage::Notification(json_notification))
                    .await
                    .is_err()
                {
                    tracing::warn!("Failed to send stream notification");
                }
            }
            return Ok(None);
        }
        ProtocolMessage::Assistant {
            message,
            parent_tool_use_id: _,
            session_id: _,
            uuid: _,
        } => {
            // Assistant messages - convert to notifications and send
            // Filter out text and thinking blocks (handled by stream events)
            let filtered_content: Vec<ContentBlock> = message
                .content
                .clone()
                .into_iter()
                .filter(|block| {
                    !matches!(
                        block,
                        ContentBlock::Text { .. } | ContentBlock::Thinking { .. }
                    )
                })
                .collect();

            let notifications = to_acp_notifications(
                AcpContent::Blocks(filtered_content),
                MessageRole::Assistant,
                session_id,
                tool_use_cache,
            );

            for notification in notifications {
                let json_notification = JsonRpcNotification::new("session/update", notification);
                if tx
                    .send(OutgoingMessage::Notification(json_notification))
                    .await
                    .is_err()
                {
                    tracing::warn!("Failed to send assistant notification");
                }
            }
            return Ok(None);
        }
        ProtocolMessage::User {
            message,
            parent_tool_use_id: _,
            session_id: _,
            uuid: _,
            tool_use_result: _,
        } => {
            // Handle special local command output (slash commands like /compact, /context)
            if let crate::claude::types::MessageContent::String(ref content) = message.content {
                if content.contains("<local-command-stdout>") {
                    // Handle /context by sending its reply as regular agent message
                    if content.contains("Context Usage") {
                        let cleaned = content
                            .replace("<local-command-stdout>", "")
                            .replace("</local-command-stdout>", "");
                        let notifications = to_acp_notifications(
                            AcpContent::String(cleaned),
                            MessageRole::Assistant,
                            session_id,
                            tool_use_cache,
                        );
                        for notification in notifications {
                            let json_notification =
                                JsonRpcNotification::new("session/update", notification);
                            if tx
                                .send(OutgoingMessage::Notification(json_notification))
                                .await
                                .is_err()
                            {
                                tracing::warn!("Failed to send context notification");
                            }
                        }
                    }
                    tracing::info!("Local command stdout: {}", content);
                    return Ok(None);
                }

                if content.contains("<local-command-stderr>") {
                    tracing::error!("Local command stderr: {}", content);
                    return Ok(None);
                }
            }

            // Skip user messages that are just text (echoes of input)
            // Only forward user messages with tool results or other non-text content
            let should_skip = match &message.content {
                crate::claude::types::MessageContent::String(_) => true,
                crate::claude::types::MessageContent::Blocks(blocks) => {
                    // Skip if only one text block
                    blocks.len() == 1 && matches!(blocks.first(), Some(ContentBlock::Text { .. }))
                }
            };

            if should_skip {
                return Ok(None);
            }

            // Forward non-text user content (tool results, etc.)
            let content = match &message.content {
                crate::claude::types::MessageContent::String(s) => AcpContent::String(s.clone()),
                crate::claude::types::MessageContent::Blocks(blocks) => {
                    AcpContent::Blocks(blocks.clone())
                }
            };

            let notifications =
                to_acp_notifications(content, MessageRole::User, session_id, tool_use_cache);

            for notification in notifications {
                let json_notification = JsonRpcNotification::new("session/update", notification);
                if tx
                    .send(OutgoingMessage::Notification(json_notification))
                    .await
                    .is_err()
                {
                    tracing::warn!("Failed to send user notification");
                }
            }
            return Ok(None);
        }
        ProtocolMessage::ControlRequest { .. } | ProtocolMessage::ControlResponse { .. } => {
            // Control messages are internal protocol - don't forward
            return Ok(None);
        }
    }
}

/// Role for message content
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    Assistant,
    User,
}

/// Content that can be converted to ACP notifications
#[derive(Debug, Clone)]
pub enum AcpContent {
    String(String),
    Blocks(Vec<ContentBlock>),
}

impl From<String> for AcpContent {
    fn from(s: String) -> Self {
        AcpContent::String(s)
    }
}

impl From<&str> for AcpContent {
    fn from(s: &str) -> Self {
        AcpContent::String(s.to_string())
    }
}

impl From<Vec<ContentBlock>> for AcpContent {
    fn from(blocks: Vec<ContentBlock>) -> Self {
        AcpContent::Blocks(blocks)
    }
}

/// Cache for tool use information
pub type ToolUseCache = std::collections::HashMap<String, ToolUseCacheEntry>;

/// Entry in the tool use cache
#[derive(Debug, Clone)]
pub struct ToolUseCacheEntry {
    pub tool_type: ToolUseType,
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// Type of tool use
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolUseType {
    ToolUse,
    ServerToolUse,
    McpToolUse,
}

/// Convert content to ACP SessionNotifications.
/// This is a Rust port of the TypeScript `toAcpNotifications` function.
pub fn to_acp_notifications(
    content: AcpContent,
    role: MessageRole,
    session_id: &str,
    tool_use_cache: &mut ToolUseCache,
) -> Vec<SessionNotification> {
    match content {
        AcpContent::String(text) => {
            let update = match role {
                MessageRole::Assistant => SessionUpdate::AgentMessageChunk(ContentChunk::new(
                    AcpContentBlock::Text(agent_client_protocol::TextContent::new(&text)),
                )),
                MessageRole::User => SessionUpdate::UserMessageChunk(ContentChunk::new(
                    AcpContentBlock::Text(agent_client_protocol::TextContent::new(&text)),
                )),
            };
            vec![SessionNotification::new(session_id.to_string(), update)]
        }
        AcpContent::Blocks(blocks) => {
            let mut output = Vec::new();

            for block in blocks {
                let update = content_block_to_session_update(block, role, tool_use_cache);
                if let Some(u) = update {
                    output.push(SessionNotification::new(session_id.to_string(), u));
                }
            }

            output
        }
    }
}

/// Convert a single ContentBlock to a SessionUpdate
fn content_block_to_session_update(
    block: ContentBlock,
    role: MessageRole,
    tool_use_cache: &mut ToolUseCache,
) -> Option<SessionUpdate> {
    match block {
        ContentBlock::Text { text } | ContentBlock::TextDelta { text } => {
            let chunk = ContentChunk::new(AcpContentBlock::Text(
                agent_client_protocol::TextContent::new(&text),
            ));
            Some(match role {
                MessageRole::Assistant => SessionUpdate::AgentMessageChunk(chunk),
                MessageRole::User => SessionUpdate::UserMessageChunk(chunk),
            })
        }
        ContentBlock::Thinking { thinking, .. } | ContentBlock::ThinkingDelta { thinking } => {
            let chunk = ContentChunk::new(AcpContentBlock::Text(
                agent_client_protocol::TextContent::new(&thinking),
            ));
            Some(SessionUpdate::AgentThoughtChunk(chunk))
        }
        ContentBlock::Image { source } => {
            let (data, mime_type, uri) = match source {
                ImageSource::Base64 { data, media_type } => (data, media_type, None),
                ImageSource::Url { url } => (String::new(), String::new(), Some(url)),
            };
            let mut image_content = agent_client_protocol::ImageContent::new(&data, &mime_type);
            if let Some(u) = uri {
                image_content = image_content.uri(u);
            }
            let chunk = ContentChunk::new(AcpContentBlock::Image(image_content));
            Some(match role {
                MessageRole::Assistant => SessionUpdate::AgentMessageChunk(chunk),
                MessageRole::User => SessionUpdate::UserMessageChunk(chunk),
            })
        }
        ContentBlock::ToolUse {
            ref id,
            ref name,
            ref input,
        }
        | ContentBlock::ServerToolUse {
            ref id,
            ref name,
            ref input,
        }
        | ContentBlock::McpToolUse {
            ref id,
            ref name,
            ref input,
        } => {
            // Determine tool type
            let tool_type = match &block {
                ContentBlock::ServerToolUse { .. } => ToolUseType::ServerToolUse,
                ContentBlock::McpToolUse { .. } => ToolUseType::McpToolUse,
                _ => ToolUseType::ToolUse,
            };

            // Cache the tool use
            tool_use_cache.insert(
                id.clone(),
                ToolUseCacheEntry {
                    tool_type,
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                },
            );

            // Skip TodoWrite tool (handled separately for plan updates)
            if name == "TodoWrite" {
                return None;
            }

            // Create tool call notification
            let raw_input = serde_json::to_value(input).ok();
            let tool_call = ToolCall::new(id.clone(), name.clone())
                .status(ToolCallStatus::Pending)
                .raw_input(raw_input);

            Some(SessionUpdate::ToolCall(tool_call))
        }
        ContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
        }
        | ContentBlock::McpToolResult {
            tool_use_id,
            content,
            is_error,
        } => {
            let cached = tool_use_cache.get(&tool_use_id);
            if cached.is_none() {
                tracing::error!(
                    "[to_acp_notifications] Got a tool result for tool use that wasn't tracked: {}",
                    tool_use_id
                );
                return None;
            }

            let cached = cached.unwrap();

            // Skip TodoWrite results
            if cached.name == "TodoWrite" {
                return None;
            }

            let status = if is_error.unwrap_or(false) {
                ToolCallStatus::Failed
            } else {
                ToolCallStatus::Completed
            };

            let raw_output = content.as_ref().map(|c| match c {
                ContentBlockContent::String(s) => serde_json::Value::String(s.clone()),
                ContentBlockContent::Array(arr) => serde_json::Value::Array(arr.clone()),
            });

            let fields = ToolCallUpdateFields::new()
                .status(status)
                .raw_output(raw_output);

            let update = ToolCallUpdate::new(tool_use_id, fields);

            Some(SessionUpdate::ToolCallUpdate(update))
        }
        // Ignore unknown/other types
        ContentBlock::Unknown => None,
    }
}

/// Stream event types for parsing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamEventType {
    ContentBlockStart {
        content_block: ContentBlock,
    },
    ContentBlockDelta {
        delta: ContentBlock,
    },
    MessageStart,
    MessageDelta,
    MessageStop,
    ContentBlockStop,
    #[serde(other)]
    Unknown,
}

/// Convert a StreamEvent to ACP SessionNotifications.
/// This is a Rust port of the TypeScript `streamEventToAcpNotifications` function.
pub fn stream_event_to_acp_notifications(
    stream_event: &crate::claude::types::StreamEvent,
    session_id: &str,
    tool_use_cache: &mut ToolUseCache,
) -> Vec<SessionNotification> {
    // Parse the event JSON to determine the type
    let event_type: Result<StreamEventType, _> = serde_json::from_value(stream_event.event.clone());

    match event_type {
        Ok(StreamEventType::ContentBlockStart { content_block }) => to_acp_notifications(
            AcpContent::Blocks(vec![content_block]),
            MessageRole::Assistant,
            session_id,
            tool_use_cache,
        ),
        Ok(StreamEventType::ContentBlockDelta { delta }) => to_acp_notifications(
            AcpContent::Blocks(vec![delta]),
            MessageRole::Assistant,
            session_id,
            tool_use_cache,
        ),
        // No content for these events
        Ok(StreamEventType::MessageStart)
        | Ok(StreamEventType::MessageDelta)
        | Ok(StreamEventType::MessageStop)
        | Ok(StreamEventType::ContentBlockStop)
        | Ok(StreamEventType::Unknown) => Vec::new(),
        Err(e) => {
            tracing::warn!("Failed to parse stream event: {}", e);
            Vec::new()
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn build_result_message(is_error: bool, result: Option<String>) -> ProtocolMessage {
        ProtocolMessage::Result(crate::claude::types::ResultMessage {
            subtype: "success".to_string(),
            duration_ms: 0,
            duration_api_ms: 0,
            is_error,
            num_turns: 0,
            session_id: "session".to_string(),
            total_cost_usd: None,
            usage: None,
            result,
            structured_output: None,
            errors: Vec::new(),
        })
    }

    #[tokio::test]
    async fn handle_msg_returns_error_on_login_required() {
        let (tx, _rx) = mpsc::channel::<OutgoingMessage>(1);
        let session_id = "session".to_string();
        let (complete_tx, _complete_rx) = oneshot::channel();
        let prompt_complete_tx = Arc::new(Mutex::new(Some(complete_tx)));
        let msg = build_result_message(false, Some("Please run /login".to_string()));

        let result = handle_msg(tx, &session_id, prompt_complete_tx, msg).await;
        let err = result.expect_err("expected auth error");
        assert!(matches!(err, PromptError::AuthRequired));
    }

    #[tokio::test]
    async fn handle_msg_returns_error_on_error_result() {
        let (tx, _rx) = mpsc::channel::<OutgoingMessage>(1);
        let session_id = "session".to_string();
        let (complete_tx, _complete_rx) = oneshot::channel();
        let prompt_complete_tx = Arc::new(Mutex::new(Some(complete_tx)));
        let msg = build_result_message(true, Some("backend error".to_string()));

        let result = handle_msg(tx, &session_id, prompt_complete_tx, msg).await;
        let err = result.expect_err("expected error result");
        match err {
            PromptError::Internal { message } => {
                assert_eq!(message, "backend error");
            }
            _ => panic!("expected Internal error"),
        }
    }
}
