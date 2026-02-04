//! Message types for Claude Agent SDK.

use serde::{Deserialize, Serialize};

/// Assistant message error types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssistantMessageError {
    AuthenticationFailed,
    BillingError,
    RateLimit,
    InvalidRequest,
    ServerError,
    Unknown,
}

/// Content block types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    TextDelta {
        text: String,
    },
    Thinking {
        thinking: String,
        signature: String,
    },
    ThinkingDelta {
        thinking: String,
    },
    Image {
        source: ImageSource,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ServerToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    McpToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<ContentBlockContent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    McpToolResult {
        tool_use_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<ContentBlockContent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    // Other types that we ignore
    #[serde(other)]
    Unknown,
}

/// Image source for image content blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    Base64 {
        data: String,
        media_type: String,
    },
    Url {
        url: String,
    },
}

/// Content for tool result blocks (can be string or array).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContentBlockContent {
    String(String),
    Array(Vec<serde_json::Value>),
}

/// Message content (can be string or array of content blocks).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    String(String),
    Blocks(Vec<ContentBlock>),
}

/// User message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub content: MessageContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_tool_use_id: Option<String>,
}

/// Assistant message with content blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub content: Vec<ContentBlock>,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_tool_use_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AssistantMessageError>,
}

/// System message with metadata.
/// 
/// System messages have a `subtype` field and various other fields depending on the subtype.
/// Extra fields are captured in the `extra` map.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMessage {
    pub subtype: String,
    /// Extra fields that vary by subtype (hook_id, hook_name, session_id, etc.)
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Result message with cost and usage information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultMessage {
    pub subtype: String,
    pub duration_ms: i64,
    pub duration_api_ms: i64,
    pub is_error: bool,
    pub num_turns: i32,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_cost_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_output: Option<serde_json::Value>,
    /// Error messages when is_error is true
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

/// Stream event for partial message updates during streaming.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEvent {
    pub uuid: String,
    pub session_id: String,
    pub event: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_tool_use_id: Option<String>,
}

/// Message types (union of all message types).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Message {
    User(UserMessage),
    Assistant(AssistantMessage),
    System(SystemMessage),
    Result(ResultMessage),
    Stream(StreamEvent),
}

/// Input message body
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputMessageBody {
    pub role: String,
    pub content: MessageContent,
}

/// Input message structure for sending to Claude
#[derive(Debug, Clone, serde::Serialize)]
pub struct InputMessage {
    /// Message type (always "user")
    #[serde(rename = "type")]
    pub r#type: String,
    /// Message content
    pub message: InputMessageBody,
    /// Parent tool use ID (for tool results)
    pub parent_tool_use_id: Option<String>,
    /// Session ID
    pub session_id: String,
}

impl InputMessage {
    /// Create a new user message with string content
    pub fn user(content: impl Into<String>, session_id: String) -> Self {
        Self {
            r#type: "user".to_string(),
            message: InputMessageBody {
                role: "user".to_string(),
                content: MessageContent::String(content.into()),
            },
            parent_tool_use_id: None,
            session_id,
        }
    }

    /// Create a new user message with block content
    pub fn user_blocks(blocks: Vec<ContentBlock>, session_id: String) -> Self {
        Self {
            r#type: "user".to_string(),
            message: InputMessageBody {
                role: "user".to_string(),
                content: MessageContent::Blocks(blocks),
            },
            parent_tool_use_id: None,
            session_id,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_block_text_serialization() {
        let block = ContentBlock::Text {
            text: "Hello".to_string(),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "text");
        assert_eq!(json["text"], "Hello");
    }

    #[test]
    fn test_content_block_tool_use_serialization() {
        let block = ContentBlock::ToolUse {
            id: "123".to_string(),
            name: "Bash".to_string(),
            input: serde_json::json!({"command": "ls"}),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "tool_use");
        assert_eq!(json["name"], "Bash");
    }

    #[test]
    fn test_message_content_string() {
        let content = MessageContent::String("Hello".to_string());
        let json = serde_json::to_value(&content).unwrap();
        assert_eq!(json, "Hello");
    }

    #[test]
    fn test_assistant_message_error() {
        let error = AssistantMessageError::RateLimit;
        let json = serde_json::to_string(&error).unwrap();
        assert_eq!(json, r#""rate_limit""#);
    }
}
