//! SDK Control Protocol types for Claude Agent SDK.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use super::hooks::HookEvent;
use super::permissions::PermissionUpdate;
use super::messages::{ResultMessage, StreamEvent, SystemMessage};

/// SDK Control Interrupt Request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SDKControlInterruptRequest {
    pub subtype: String, // "interrupt"
}

/// SDK Control Permission Request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SDKControlPermissionRequest {
    pub subtype: String, // "can_use_tool"
    pub tool_name: String,
    pub input: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_suggestions: Option<Vec<PermissionUpdate>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_path: Option<String>,
}


/// SDK Control Set Permission Mode Request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SDKControlSetPermissionModeRequest {
    pub subtype: String, // "set_permission_mode"
    pub mode: String,
}

/// SDK Hook Callback Request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SDKHookCallbackRequest {
    pub subtype: String, // "hook_callback"
    pub callback_id: String,
    pub input: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
}

/// SDK Control MCP Message Request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SDKControlMcpMessageRequest {
    pub subtype: String, // "mcp_message"
    pub server_name: String,
    pub message: serde_json::Value,
}

/// SDK Control Rewind Files Request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SDKControlRewindFilesRequest {
    pub subtype: String, // "rewind_files"
    pub user_message_id: String,
}

/// SDK Control Request (union of all request types).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum SDKControlRequestType {
    Interrupt,
    #[serde(rename = "can_use_tool")]
    CanUseTool {
        tool_name: String,
        input: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        permission_suggestions: Option<Vec<PermissionUpdate>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        blocked_path: Option<String>,
    },
    Initialize {
        #[serde(skip_serializing_if = "Option::is_none")]
        hooks: Option<HashMap<HookEvent, serde_json::Value>>,
    },
    SetPermissionMode {
        mode: String,
    },
    HookCallback {
        callback_id: String,
        input: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_use_id: Option<String>,
    },
    McpMessage {
        server_name: String,
        message: serde_json::Value,
    },
    RewindFiles {
        user_message_id: String,
    },
}


/// SDK Control Response (union of success and error).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "subtype", rename_all = "lowercase")]
pub enum SDKControlResponseType {
    Success {
        request_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        response: Option<serde_json::Value>,
    },
    Error {
        request_id: String,
        error: String,
    },
}

/// SDK Control Response wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SDKControlResponse {
    #[serde(rename = "type")]
    pub type_: String, // "control_response"
    pub response: SDKControlResponseType,
}

impl SDKControlResponse {
    pub fn new(response: SDKControlResponseType) -> Self {
        Self {
            type_: "control_response".to_string(),
            response,
        }
    }
    pub fn success(request_id: String, response: serde_json::Value) -> Self {
        Self {
            type_: "control_response".to_string(),

            response: SDKControlResponseType::Success {
                request_id,
                response: Some(response),
            },
        }
    }
}

/// Protocol message types (top-level message envelope).
///
/// This enum represents all possible message types in the SDK control protocol.
/// It uses serde's tag-based deserialization to route messages based on the `type` field.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProtocolMessage {
    /// SDK control request from CLI
    ControlRequest {
        request_id: String,
        request: SDKControlRequestType,
    },
    /// SDK control response (usually filtered, not forwarded to app)
    ControlResponse {
        response: SDKControlResponseType,
    },
    /// User message (tool results, etc.)
    User {
        message: UserMessageInner,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_tool_use_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        uuid: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_use_result: Option<serde_json::Value>,
    },
    /// Assistant message with content blocks (wrapped in 'message' field by CLI)
    Assistant {
        message: AssistantMessageInner,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_tool_use_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        uuid: Option<String>,
    },
    /// Stream event for partial updates
    #[serde(rename = "stream_event")]
    Stream(StreamEvent),
    /// Result message with cost/usage info
    Result(ResultMessage),
    /// System message with metadata
    System(SystemMessage),
}

/// Inner assistant message content from CLI.
/// This is the structure inside the 'message' field of assistant messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessageInner {
    pub content: Vec<crate::claude::types::ContentBlock>,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<serde_json::Value>,
}

/// Inner user message content from CLI.
/// This is the structure inside the 'message' field of user messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessageInner {
    pub role: String,
    pub content: crate::claude::types::MessageContent,
}
