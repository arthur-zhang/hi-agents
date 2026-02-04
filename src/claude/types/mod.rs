//! Type definitions for Claude Agent SDK.

pub mod agent;
pub mod control;
pub mod hooks;
pub mod mcp;
pub mod messages;
pub mod permissions;
pub mod sandbox;

// Re-export commonly used types
pub use agent::{
    AgentDefinition, AgentModel, ClaudeAgentOptions, McpServersConfig, SdkBeta, SettingSource,
    SystemPromptConfig, ToolsConfig,
};
pub use control::{
    AssistantMessageInner, ProtocolMessage, SDKControlInterruptRequest,
    SDKControlMcpMessageRequest, SDKControlPermissionRequest,
    SDKControlRequestType, SDKControlResponse, SDKControlResponseType,
    SDKControlRewindFilesRequest, SDKControlSetPermissionModeRequest, SDKHookCallbackRequest,
    UserMessageInner,
};
pub use crate::claude::error::{Error, Result};
pub use hooks::{
    AsyncHookJSONOutput, BaseHookInput, CompactTrigger, HookCallback, HookConfig, HookContext,
    HookEvent, HookInput, HookJSONOutput, HookMatcher, HookSpecificOutput, PermissionDecision,
    PostToolUseHookSpecificOutput, PreToolUseHookSpecificOutput, SessionStartHookSpecificOutput,
    SyncHookJSONOutput, UserPromptSubmitHookSpecificOutput,
};
pub use mcp::{McpServerConfig, SdkPluginConfig};
pub use messages::{
    AssistantMessage, AssistantMessageError, ContentBlock, ContentBlockContent, InputMessage,
    Message, MessageContent, ResultMessage, StreamEvent, SystemMessage, UserMessage,
};
pub use permissions::{
    CanUseTool, PermissionBehavior, PermissionMode, PermissionResult, PermissionResultAllow,
    PermissionResultDeny, PermissionRuleValue, PermissionUpdate, PermissionUpdateDestination,
    ToolPermissionContext,
};
pub use sandbox::{SandboxIgnoreViolations, SandboxNetworkConfig, SandboxSettings};
