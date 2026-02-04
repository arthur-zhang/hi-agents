//! Hook system types for Claude Agent SDK.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Hook event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    UserPromptSubmit,
    Stop,
    SubagentStop,
    PreCompact,
}

/// Base hook input fields present across many hook events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseHookInput {
    pub session_id: String,
    pub transcript_path: String,
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
}

/// Hook input data (discriminated by hook_event_name).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "hook_event_name", rename_all = "PascalCase")]
pub enum HookInput {
    PreToolUse {
        #[serde(flatten)]
        base: BaseHookInput,
        tool_name: String,
        tool_input: serde_json::Value,
    },
    PostToolUse {
        #[serde(flatten)]
        base: BaseHookInput,
        tool_name: String,
        tool_input: serde_json::Value,
        tool_response: serde_json::Value,
    },
    UserPromptSubmit {
        #[serde(flatten)]
        base: BaseHookInput,
        prompt: String,
    },
    Stop {
        #[serde(flatten)]
        base: BaseHookInput,
        stop_hook_active: bool,
    },
    SubagentStop {
        #[serde(flatten)]
        base: BaseHookInput,
        stop_hook_active: bool,
    },
    PreCompact {
        #[serde(flatten)]
        base: BaseHookInput,
        trigger: CompactTrigger,
        #[serde(skip_serializing_if = "Option::is_none")]
        custom_instructions: Option<String>,
    },
}

/// Compact trigger type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompactTrigger {
    Manual,
    Auto,
}

/// Hook-specific output for PreToolUse events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreToolUseHookSpecificOutput {
    #[serde(rename = "hookEventName")]
    pub hook_event_name: String, // "PreToolUse"
    #[serde(rename = "permissionDecision", skip_serializing_if = "Option::is_none")]
    pub permission_decision: Option<PermissionDecision>,
    #[serde(
        rename = "permissionDecisionReason",
        skip_serializing_if = "Option::is_none"
    )]
    pub permission_decision_reason: Option<String>,
    #[serde(rename = "updatedInput", skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<serde_json::Value>,
}

/// Hook-specific output for PostToolUse events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostToolUseHookSpecificOutput {
    #[serde(rename = "hookEventName")]
    pub hook_event_name: String, // "PostToolUse"
    #[serde(rename = "additionalContext", skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

/// Hook-specific output for UserPromptSubmit events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPromptSubmitHookSpecificOutput {
    #[serde(rename = "hookEventName")]
    pub hook_event_name: String, // "UserPromptSubmit"
    #[serde(rename = "additionalContext", skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

/// Hook-specific output for SessionStart events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStartHookSpecificOutput {
    #[serde(rename = "hookEventName")]
    pub hook_event_name: String, // "SessionStart"
    #[serde(rename = "additionalContext", skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

/// Hook-specific output union.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HookSpecificOutput {
    PreToolUse(PreToolUseHookSpecificOutput),
    PostToolUse(PostToolUseHookSpecificOutput),
    UserPromptSubmit(UserPromptSubmitHookSpecificOutput),
    SessionStart(SessionStartHookSpecificOutput),
}

/// Permission decision for PreToolUse hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionDecision {
    Allow,
    Deny,
    Ask,
}

/// Async hook output that defers hook execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsyncHookJSONOutput {
    /// Set to true to defer hook execution.
    /// Note: This is converted to "async" when sent to the CLI.
    #[serde(rename = "async")]
    pub async_: bool,
    /// Optional timeout in milliseconds for the async operation.
    #[serde(rename = "asyncTimeout", skip_serializing_if = "Option::is_none")]
    pub async_timeout: Option<i32>,
}

/// Synchronous hook output with control and decision fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncHookJSONOutput {
    /// Whether Claude should proceed after hook execution (default: true).
    /// Note: This is converted to "continue" when sent to the CLI.
    #[serde(rename = "continue", skip_serializing_if = "Option::is_none")]
    pub continue_: Option<bool>,
    /// Hide stdout from transcript mode (default: false).
    #[serde(rename = "suppressOutput", skip_serializing_if = "Option::is_none")]
    pub suppress_output: Option<bool>,
    /// Message shown when continue is false.
    #[serde(rename = "stopReason", skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    /// Set to "block" to indicate blocking behavior.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>, // "block"
    /// Warning message displayed to the user.
    #[serde(rename = "systemMessage", skip_serializing_if = "Option::is_none")]
    pub system_message: Option<String>,
    /// Feedback message for Claude about the decision.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Hook-specific outputs.
    #[serde(rename = "hookSpecificOutput", skip_serializing_if = "Option::is_none")]
    pub hook_specific_output: Option<HookSpecificOutput>,
}

/// Hook JSON output (async or sync).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HookJSONOutput {
    Async(AsyncHookJSONOutput),
    Sync(SyncHookJSONOutput),
}

/// Context information for hook callbacks.
#[derive(Debug, Clone)]
pub struct HookContext {
    /// Reserved for future abort signal support. Currently always None.
    pub signal: Option<()>,
}

/// Trait for hook callbacks.
#[async_trait]
pub trait HookCallback: Send + Sync {
    /// Execute the hook callback.
    ///
    /// # Arguments
    /// * `input` - Hook input data
    /// * `tool_use_id` - Optional tool use identifier
    /// * `context` - Hook context
    ///
    /// # Returns
    /// Hook output (async or sync)
    async fn call(
        &self,
        input: HookInput,
        tool_use_id: Option<String>,
        context: HookContext,
    ) -> crate::claude::error::Result<HookJSONOutput>;
}

/// Hook matcher configuration.
pub struct HookMatcher {
    /// Matcher pattern (e.g., "Bash", "Write|MultiEdit|Edit").
    pub matcher: Option<String>,
    /// List of hook callbacks.
    pub hooks: Vec<Box<dyn HookCallback>>,
    /// Timeout in seconds for all hooks in this matcher (default: 60).
    pub timeout: Option<f64>,
}

/// Hook configuration map.
pub type HookConfig = HashMap<HookEvent, Vec<HookMatcher>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_event_serialization() {
        let event = HookEvent::PreToolUse;
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(json, r#""PreToolUse""#);
    }

    #[test]
    fn test_hook_input_serialization() {
        let input = HookInput::PreToolUse {
            base: BaseHookInput {
                session_id: "test".to_string(),
                transcript_path: "/path".to_string(),
                cwd: "/cwd".to_string(),
                permission_mode: None,
            },
            tool_name: "Bash".to_string(),
            tool_input: serde_json::json!({"command": "ls"}),
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "PreToolUse");
        assert_eq!(json["tool_name"], "Bash");
    }
}
