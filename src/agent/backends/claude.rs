//! Claude agent backend implementation

use crate::agent::{AgentBackend, AgentError, AgentResult, NotificationStream, TurnCompletion, TurnResult};
use crate::claude::process_handle::ProcessHandle;
use crate::claude::transport::stream::into_event_stream;
use crate::claude::transport::transport2::SubprocessCLITransport;
use crate::claude::types::{
    ClaudeAgentOptions, ContentBlock, InputMessage, MessageContent, ProtocolMessage,
};

use agent_client_protocol::{
    ContentBlock as AcpContentBlock, ContentChunk, NewSessionRequest, PromptRequest,
    SessionNotification, SessionUpdate, StopReason, ToolCall, ToolCallStatus, ToolCallUpdate,
    ToolCallUpdateFields,
};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use std::any::Any;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tokio::io::AsyncWriteExt;
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::mpsc;

// ============================================================================
// Tool Use Cache (moved from ws.rs)
// ============================================================================

/// Type of tool use
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolUseType {
    ToolUse,
    ServerToolUse,
    McpToolUse,
}

/// Entry in the tool use cache
#[derive(Debug, Clone)]
pub struct ToolUseCacheEntry {
    pub tool_type: ToolUseType,
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// Cache for tool use information
pub type ToolUseCache = HashMap<String, ToolUseCacheEntry>;

// ============================================================================
// Shared State for Turn Execution
// ============================================================================

struct TurnState {
    stdout: Option<ChildStdout>,
    tool_use_cache: ToolUseCache,
    completion: Option<TurnCompletion>,
}

// ============================================================================
// Claude Backend
// ============================================================================

pub struct ClaudeBackend {
    stdin: Option<ChildStdin>,
    stdout: Option<ChildStdout>,
    process: Option<ProcessHandle>,
    tool_use_cache: ToolUseCache,
    pending_completion: Option<TurnCompletion>,
    // Shared state for turn execution
    turn_state: Option<Arc<Mutex<TurnState>>>,
}

impl ClaudeBackend {
    pub fn new() -> Self {
        Self {
            stdin: None,
            stdout: None,
            process: None,
            tool_use_cache: ToolUseCache::new(),
            pending_completion: None,
            turn_state: None,
        }
    }
}

#[async_trait]
impl AgentBackend for ClaudeBackend {
    async fn start_session(
        &mut self,
        req: &NewSessionRequest,
        session_id: &str,
    ) -> AgentResult<()> {
        let mut options = ClaudeAgentOptions::new();
        options.cwd = Some(req.cwd.clone());
        options.max_thinking_tokens = Some(50000);
        options.include_partial_messages = true;
        options
            .extra_args
            .insert("session-id".to_string(), Some(session_id.to_string()));

        let mut transport = SubprocessCLITransport::new(options)
            .map_err(|e| AgentError::Process(e.to_string()))?;

        transport
            .connect()
            .await
            .map_err(|e| AgentError::Process(e.to_string()))?;

        let (stdout, stdin, _stderr, handle) = transport
            .split()
            .map_err(|e| AgentError::Process(e.to_string()))?;

        self.stdin = Some(stdin);
        self.stdout = Some(stdout);
        self.process = Some(handle);

        tracing::info!("Claude process started for session {}", session_id);
        Ok(())
    }

    async fn execute_turn(
        &mut self,
        req: &PromptRequest,
        session_id: &str,
    ) -> AgentResult<TurnResult> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| AgentError::Internal("No stdin available".into()))?;
        let stdout = self
            .stdout
            .take()
            .ok_or_else(|| AgentError::Internal("No stdout available".into()))?;

        // Convert prompt to string
        let content = prompt_to_string(&req.prompt);
        let input_msg = InputMessage::user(&content, session_id.to_string());
        let json = serde_json::to_string(&input_msg).unwrap() + "\n";

        // Send to Claude stdin
        stdin
            .write_all(json.as_bytes())
            .await
            .map_err(|e| AgentError::Io(e))?;

        // Take ownership of tool_use_cache for the stream
        let tool_use_cache = std::mem::take(&mut self.tool_use_cache);

        // Create shared state for the stream to return data
        let turn_state = Arc::new(Mutex::new(TurnState {
            stdout: None,
            tool_use_cache: ToolUseCache::new(),
            completion: None,
        }));
        self.turn_state = Some(turn_state.clone());

        // Create notification stream
        let stream = ClaudeNotificationStream::new(
            stdout,
            session_id.to_string(),
            tool_use_cache,
            turn_state,
        );

        Ok(TurnResult {
            notifications: Box::pin(stream),
        })
    }

    fn take_completion(&mut self) -> TurnCompletion {
        // First try to get completion from turn_state (set by stream)
        if let Some(turn_state) = self.turn_state.take() {
            let mut state = turn_state.lock().unwrap();
            // Restore stdout and tool_use_cache
            if let Some(stdout) = state.stdout.take() {
                self.stdout = Some(stdout);
            }
            self.tool_use_cache = std::mem::take(&mut state.tool_use_cache);
            if let Some(completion) = state.completion.take() {
                return completion;
            }
        }

        self.pending_completion
            .take()
            .unwrap_or(TurnCompletion::Stop(StopReason::EndTurn))
    }

    async fn kill(&mut self) -> AgentResult<()> {
        if let Some(mut handle) = self.process.take() {
            handle
                .kill()
                .await
                .map_err(|e| AgentError::Process(e.to_string()))?;
        }
        Ok(())
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
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

// ============================================================================
// Claude Notification Stream
// ============================================================================

use crate::claude::transport::stream::EventStream;
use crate::claude::types::{ContentBlockContent, ImageSource, ResultMessage, StreamEvent};

/// Stream event types for parsing
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamEventType {
    ContentBlockStart { content_block: ContentBlock },
    ContentBlockDelta { delta: ContentBlock },
    MessageStart,
    MessageDelta,
    MessageStop,
    ContentBlockStop,
    #[serde(other)]
    Unknown,
}

/// Role for message content
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    Assistant,
    User,
}

/// A stream that converts Claude ProtocolMessages to ACP SessionNotifications
pub struct ClaudeNotificationStream {
    inner: Option<EventStream>,
    session_id: String,
    tool_use_cache: ToolUseCache,
    pending_notifications: Vec<SessionNotification>,
    completion: Option<TurnCompletion>,
    finished: bool,
    // Shared state to return data to backend
    turn_state: Arc<Mutex<TurnState>>,
}

impl ClaudeNotificationStream {
    pub fn new(
        stdout: ChildStdout,
        session_id: String,
        tool_use_cache: ToolUseCache,
        turn_state: Arc<Mutex<TurnState>>,
    ) -> Self {
        Self {
            inner: Some(into_event_stream(stdout)),
            session_id,
            tool_use_cache,
            pending_notifications: Vec::new(),
            completion: None,
            finished: false,
            turn_state,
        }
    }

    /// Save state back to shared turn_state when stream finishes
    fn save_state_to_shared(&mut self) {
        if let Some(inner) = self.inner.take() {
            let stdout = inner.into_inner();
            let mut state = self.turn_state.lock().unwrap();
            state.stdout = Some(stdout);
            state.tool_use_cache = std::mem::take(&mut self.tool_use_cache);
            state.completion = self.completion.take();
        }
    }

    /// Process a protocol message and return notifications
    fn process_message(&mut self, msg: ProtocolMessage) -> Option<TurnCompletion> {
        match msg {
            ProtocolMessage::System(system_message) => {
                // System messages are internal, don't forward
                match system_message.subtype.as_str() {
                    "init" | "compact_boundary" | "hook_started" | "task_notification"
                    | "hook_progress" | "hook_response" | "status" | "files_persisted" => {}
                    _ => {
                        tracing::warn!("Unknown system message subtype: {:?}", system_message);
                    }
                }
                None
            }
            ProtocolMessage::Result(result_message) => {
                self.handle_result_message(&result_message)
            }
            ProtocolMessage::Stream(stream_event) => {
                let notifications = self.stream_event_to_notifications(&stream_event);
                self.pending_notifications.extend(notifications);
                None
            }
            ProtocolMessage::Assistant { message, .. } => {
                // Filter out text and thinking blocks (handled by stream events)
                let filtered_content: Vec<ContentBlock> = message
                    .content
                    .into_iter()
                    .filter(|block| {
                        !matches!(
                            block,
                            ContentBlock::Text { .. } | ContentBlock::Thinking { .. }
                        )
                    })
                    .collect();

                let notifications = self.blocks_to_notifications(filtered_content, MessageRole::Assistant);
                self.pending_notifications.extend(notifications);
                None
            }
            ProtocolMessage::User { message, .. } => {
                // Handle special local command output
                if let MessageContent::String(ref content) = message.content {
                    if content.contains("<local-command-stdout>") {
                        if content.contains("Context Usage") {
                            let cleaned = content
                                .replace("<local-command-stdout>", "")
                                .replace("</local-command-stdout>", "");
                            let notifications = self.text_to_notifications(&cleaned, MessageRole::Assistant);
                            self.pending_notifications.extend(notifications);
                        }
                        return None;
                    }
                    if content.contains("<local-command-stderr>") {
                        return None;
                    }
                }

                // Skip user messages that are just text echoes
                let should_skip = match &message.content {
                    MessageContent::String(_) => true,
                    MessageContent::Blocks(blocks) => {
                        blocks.len() == 1 && matches!(blocks.first(), Some(ContentBlock::Text { .. }))
                    }
                };

                if !should_skip {
                    let blocks = match message.content {
                        MessageContent::String(s) => {
                            let notifications = self.text_to_notifications(&s, MessageRole::User);
                            self.pending_notifications.extend(notifications);
                            return None;
                        }
                        MessageContent::Blocks(blocks) => blocks,
                    };
                    let notifications = self.blocks_to_notifications(blocks, MessageRole::User);
                    self.pending_notifications.extend(notifications);
                }
                None
            }
            ProtocolMessage::ControlRequest { .. } | ProtocolMessage::ControlResponse { .. } => {
                // Control messages are internal protocol - don't forward
                None
            }
        }
    }

    fn handle_result_message(&self, result: &ResultMessage) -> Option<TurnCompletion> {
        let result_text = result.result.clone().unwrap_or_default();
        let errors_text = result.errors.join(", ");

        match result.subtype.as_str() {
            "success" => {
                if result_text.contains("Please run /login") {
                    return Some(TurnCompletion::Error(AgentError::AuthRequired));
                }
                if result.is_error {
                    return Some(TurnCompletion::Error(AgentError::Internal(result_text)));
                }
                Some(TurnCompletion::Stop(StopReason::EndTurn))
            }
            "error_during_execution" => {
                if result.is_error {
                    let message = if errors_text.is_empty() {
                        result.subtype.clone()
                    } else {
                        errors_text
                    };
                    return Some(TurnCompletion::Error(AgentError::Internal(message)));
                }
                Some(TurnCompletion::Stop(StopReason::EndTurn))
            }
            "error_max_budget_usd" | "error_max_turns" | "error_max_structured_output_retries" => {
                if result.is_error {
                    let message = if errors_text.is_empty() {
                        result.subtype.clone()
                    } else {
                        errors_text
                    };
                    return Some(TurnCompletion::Error(AgentError::Internal(message)));
                }
                Some(TurnCompletion::Stop(StopReason::MaxTurnRequests))
            }
            _ => {
                tracing::warn!("Unknown result subtype: {}", result.subtype);
                None
            }
        }
    }

    fn stream_event_to_notifications(&mut self, stream_event: &StreamEvent) -> Vec<SessionNotification> {
        let event_type: Result<StreamEventType, _> =
            serde_json::from_value(stream_event.event.clone());

        match event_type {
            Ok(StreamEventType::ContentBlockStart { content_block }) => {
                self.blocks_to_notifications(vec![content_block], MessageRole::Assistant)
            }
            Ok(StreamEventType::ContentBlockDelta { delta }) => {
                self.blocks_to_notifications(vec![delta], MessageRole::Assistant)
            }
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

    fn text_to_notifications(&self, text: &str, role: MessageRole) -> Vec<SessionNotification> {
        let update = match role {
            MessageRole::Assistant => SessionUpdate::AgentMessageChunk(ContentChunk::new(
                AcpContentBlock::Text(agent_client_protocol::TextContent::new(text)),
            )),
            MessageRole::User => SessionUpdate::UserMessageChunk(ContentChunk::new(
                AcpContentBlock::Text(agent_client_protocol::TextContent::new(text)),
            )),
        };
        vec![SessionNotification::new(self.session_id.clone(), update)]
    }

    fn blocks_to_notifications(
        &mut self,
        blocks: Vec<ContentBlock>,
        role: MessageRole,
    ) -> Vec<SessionNotification> {
        let mut output = Vec::new();

        for block in blocks {
            if let Some(update) = self.content_block_to_session_update(block, role) {
                output.push(SessionNotification::new(self.session_id.clone(), update));
            }
        }

        output
    }

    fn content_block_to_session_update(
        &mut self,
        block: ContentBlock,
        role: MessageRole,
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
                let mut image_content =
                    agent_client_protocol::ImageContent::new(&data, &mime_type);
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
                let tool_type = match &block {
                    ContentBlock::ServerToolUse { .. } => ToolUseType::ServerToolUse,
                    ContentBlock::McpToolUse { .. } => ToolUseType::McpToolUse,
                    _ => ToolUseType::ToolUse,
                };

                self.tool_use_cache.insert(
                    id.clone(),
                    ToolUseCacheEntry {
                        tool_type,
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    },
                );

                // Skip TodoWrite tool
                if name == "TodoWrite" {
                    return None;
                }

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
                let cached = self.tool_use_cache.get(&tool_use_id)?;

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
            ContentBlock::Unknown => None,
        }
    }
}

impl Stream for ClaudeNotificationStream {
    type Item = SessionNotification;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // First, return any pending notifications
        if !self.pending_notifications.is_empty() {
            return Poll::Ready(Some(self.pending_notifications.remove(0)));
        }

        // If we've finished, return None
        if self.finished {
            return Poll::Ready(None);
        }

        // Get mutable reference to inner stream
        let inner = match self.inner.as_mut() {
            Some(inner) => inner,
            None => {
                self.finished = true;
                return Poll::Ready(None);
            }
        };

        // Poll the inner stream
        match Pin::new(inner).poll_next(cx) {
            Poll::Ready(Some(Ok(msg))) => {
                if let Some(completion) = self.process_message(msg) {
                    self.completion = Some(completion);
                    self.finished = true;
                    // Save state back to shared turn_state
                    self.save_state_to_shared();
                }
                // Re-poll to return pending notifications or check for more messages
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Poll::Ready(Some(Err(e))) => {
                tracing::error!("Error reading Claude output: {}", e);
                self.completion = Some(TurnCompletion::Error(AgentError::Internal(e.to_string())));
                self.finished = true;
                // Save state back to shared turn_state
                self.save_state_to_shared();
                Poll::Ready(None)
            }
            Poll::Ready(None) => {
                // Stream ended
                if self.completion.is_none() {
                    self.completion = Some(TurnCompletion::Stop(StopReason::EndTurn));
                }
                self.finished = true;
                // Save state back to shared turn_state
                self.save_state_to_shared();
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
