//! Agent configuration types for Claude Agent SDK.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use super::hooks::{HookEvent, HookMatcher};
use super::mcp::{McpServerConfig, SdkPluginConfig};
use super::permissions::{CanUseTool, PermissionMode};
use super::sandbox::SandboxSettings;

/// Setting source types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingSource {
    User,
    Project,
    Local,
}

/// Agent model types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentModel {
    Sonnet,
    Opus,
    Haiku,
    Inherit,
}

/// SDK Beta features.
pub type SdkBeta = String;

/// System prompt preset configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SystemPromptConfig {
    #[serde(rename = "preset")]
    Preset {
        preset: String, // "claude_code"
        #[serde(skip_serializing_if = "Option::is_none")]
        append: Option<String>,
    },
    #[serde(rename = "custom")]
    Custom { content: String },
}

/// Tools configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolsConfig {
    Preset {
        #[serde(rename = "type")]
        type_: String, // "preset"
        preset: String, // "claude_code"
    },
    List(Vec<String>),
}

/// MCP servers configuration (can be a map or a path to config file).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpServersConfig {
    Map(HashMap<String, McpServerConfig>),
    Path(String),
}

impl Default for McpServersConfig {
    fn default() -> Self {
        Self::Map(HashMap::new())
    }
}

/// Agent definition configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    pub description: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<AgentModel>,
}

/// Main configuration options for Claude Agent SDK.
///
/// This struct contains all configuration options for running a Claude agent.
/// Note: This struct does not implement Serialize/Deserialize because it contains
/// function pointers and trait objects.
#[derive(Default)]
pub struct ClaudeAgentOptions {
    /// Tools configuration (preset or list).
    pub tools: Option<ToolsConfig>,
    /// List of allowed tools.
    pub allowed_tools: Vec<String>,
    /// System prompt configuration.
    pub system_prompt: Option<SystemPromptConfig>,
    /// MCP servers configuration.
    pub mcp_servers: McpServersConfig,
    /// Permission mode.
    pub permission_mode: Option<PermissionMode>,
    /// Continue previous conversation.
    pub continue_conversation: bool,
    /// Resume from session ID.
    pub resume: Option<String>,
    /// Maximum number of turns.
    pub max_turns: Option<i32>,
    /// Maximum budget in USD.
    pub max_budget_usd: Option<f64>,
    /// List of disallowed tools.
    pub disallowed_tools: Vec<String>,
    /// Model name.
    pub model: Option<String>,
    /// Fallback model name.
    pub fallback_model: Option<String>,
    /// Beta features to enable.
    pub betas: Vec<SdkBeta>,
    /// Permission prompt tool name.
    pub permission_prompt_tool_name: Option<String>,
    /// Current working directory.
    pub cwd: Option<PathBuf>,
    /// Path to Claude CLI.
    pub cli_path: Option<PathBuf>,
    /// Settings file path.
    pub settings: Option<String>,
    /// Additional directories to add.
    pub add_dirs: Vec<PathBuf>,
    /// Environment variables.
    pub env: HashMap<String, String>,
    /// Extra CLI arguments.
    pub extra_args: HashMap<String, Option<String>>,
    /// Maximum buffer size for CLI stdout.
    pub max_buffer_size: Option<usize>,
    /// Callback for stderr output from CLI.
    pub stderr: Option<Box<dyn Fn(String) + Send + Sync>>,
    /// Tool permission callback.
    pub can_use_tool: Option<Box<dyn CanUseTool>>,
    /// Hook configurations.
    pub hooks: Option<HashMap<HookEvent, Vec<HookMatcher>>>,
    /// User identifier.
    pub user: Option<String>,
    /// Include partial messages during streaming.
    pub include_partial_messages: bool,
    /// Fork session when resuming (create new session ID).
    pub fork_session: bool,
    /// Agent definitions for custom agents.
    pub agents: Option<HashMap<String, AgentDefinition>>,
    /// Setting sources to load (user, project, local).
    pub setting_sources: Option<Vec<SettingSource>>,
    /// Sandbox configuration for bash command isolation.
    pub sandbox: Option<SandboxSettings>,
    /// Plugin configurations.
    pub plugins: Vec<SdkPluginConfig>,
    /// Maximum tokens for thinking blocks.
    pub max_thinking_tokens: Option<i32>,
    /// Output format for structured outputs (matches Messages API structure).
    pub output_format: Option<serde_json::Value>,
    /// Enable file checkpointing to track file changes during the session.
    pub enable_file_checkpointing: bool,
}

impl ClaudeAgentOptions {
    /// Create a new ClaudeAgentOptions with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Clone the options, excluding non-cloneable fields (callbacks, hooks).
    ///
    /// This creates a shallow clone that copies all configuration values
    /// but does not clone trait objects (can_use_tool, hooks, stderr).
    #[allow(clippy::should_implement_trait)]
    pub fn clone(&self) -> Self {
        Self {
            tools: self.tools.clone(),
            allowed_tools: self.allowed_tools.clone(),
            system_prompt: self.system_prompt.clone(),
            mcp_servers: self.mcp_servers.clone(),
            permission_mode: self.permission_mode,
            continue_conversation: self.continue_conversation,
            resume: self.resume.clone(),
            max_turns: self.max_turns,
            max_budget_usd: self.max_budget_usd,
            disallowed_tools: self.disallowed_tools.clone(),
            model: self.model.clone(),
            fallback_model: self.fallback_model.clone(),
            betas: self.betas.clone(),
            permission_prompt_tool_name: self.permission_prompt_tool_name.clone(),
            cwd: self.cwd.clone(),
            cli_path: self.cli_path.clone(),
            settings: self.settings.clone(),
            add_dirs: self.add_dirs.clone(),
            env: self.env.clone(),
            extra_args: self.extra_args.clone(),
            max_buffer_size: self.max_buffer_size,
            stderr: None,       // Cannot clone function pointer
            can_use_tool: None, // Cannot clone trait object
            hooks: None,        // Cannot clone hooks
            user: self.user.clone(),
            include_partial_messages: self.include_partial_messages,
            fork_session: self.fork_session,
            agents: self.agents.clone(),
            setting_sources: self.setting_sources.clone(),
            sandbox: self.sandbox.clone(),
            plugins: self.plugins.clone(),
            max_thinking_tokens: self.max_thinking_tokens,
            output_format: self.output_format.clone(),
            enable_file_checkpointing: self.enable_file_checkpointing,
        }
    }

    /// Set the tools configuration.
    pub fn with_tools(mut self, tools: ToolsConfig) -> Self {
        self.tools = Some(tools);
        self
    }

    /// Set the system prompt.
    pub fn with_system_prompt(mut self, prompt: SystemPromptConfig) -> Self {
        self.system_prompt = Some(prompt);
        self
    }

    /// Set the permission mode.
    pub fn with_permission_mode(mut self, mode: PermissionMode) -> Self {
        self.permission_mode = Some(mode);
        self
    }

    /// Set the model.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set the current working directory.
    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Set the maximum number of turns.
    pub fn with_max_turns(mut self, max_turns: i32) -> Self {
        self.max_turns = Some(max_turns);
        self
    }

    /// Set the maximum budget in USD.
    pub fn with_max_budget_usd(mut self, max_budget: f64) -> Self {
        self.max_budget_usd = Some(max_budget);
        self
    }

    /// Enable sandbox.
    pub fn with_sandbox(mut self, sandbox: SandboxSettings) -> Self {
        self.sandbox = Some(sandbox);
        self
    }

    /// Add an MCP server.
    pub fn add_mcp_server(mut self, name: impl Into<String>, config: McpServerConfig) -> Self {
        match &mut self.mcp_servers {
            McpServersConfig::Map(map) => {
                map.insert(name.into(), config);
            }
            McpServersConfig::Path(_) => {
                // Convert to map if it was a path
                let mut map = HashMap::new();
                map.insert(name.into(), config);
                self.mcp_servers = McpServersConfig::Map(map);
            }
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_model_serialization() {
        let model = AgentModel::Sonnet;
        let json = serde_json::to_string(&model).unwrap();
        assert_eq!(json, r#""sonnet""#);
    }

    #[test]
    fn test_setting_source_serialization() {
        let source = SettingSource::Project;
        let json = serde_json::to_string(&source).unwrap();
        assert_eq!(json, r#""project""#);
    }

    #[test]
    fn test_system_prompt_preset_serialization() {
        let config = SystemPromptConfig::Preset {
            preset: "claude_code".to_string(),
            append: Some("Additional instructions".to_string()),
        };
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["type"], "preset");
        assert_eq!(json["preset"], "claude_code");
        assert_eq!(json["append"], "Additional instructions");
    }

    #[test]
    fn test_agent_definition_serialization() {
        let agent = AgentDefinition {
            description: "Test agent".to_string(),
            prompt: "You are a test agent".to_string(),
            tools: Some(vec!["Bash".to_string(), "Read".to_string()]),
            model: Some(AgentModel::Haiku),
        };
        let json = serde_json::to_value(&agent).unwrap();
        assert_eq!(json["description"], "Test agent");
        assert_eq!(json["model"], "haiku");
    }

    #[test]
    fn test_claude_agent_options_builder() {
        let options = ClaudeAgentOptions::new()
            .with_model("claude-sonnet-4")
            .with_max_turns(10)
            .with_permission_mode(PermissionMode::Plan);

        assert_eq!(options.model, Some("claude-sonnet-4".to_string()));
        assert_eq!(options.max_turns, Some(10));
        assert_eq!(options.permission_mode, Some(PermissionMode::Plan));
    }
}
