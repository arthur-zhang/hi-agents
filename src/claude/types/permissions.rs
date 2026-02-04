//! Permission-related types for Claude Agent SDK.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Permission modes for the SDK.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    /// Default permission mode.
    Default,
    /// Accept edits mode.
    AcceptEdits,
    /// Plan mode.
    Plan,
    /// Bypass permissions mode.
    BypassPermissions,
    /// Delegate mode - delegate permission decisions to parent process.
    Delegate,
    /// Don't ask mode - deny all permission requests without prompting.
    DontAsk,
}

/// Permission behavior options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionBehavior {
    /// Allow the action.
    Allow,
    /// Deny the action.
    Deny,
    /// Ask the user.
    Ask,
}

/// Permission update destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionUpdateDestination {
    /// User settings.
    UserSettings,
    /// Project settings.
    ProjectSettings,
    /// Local settings.
    LocalSettings,
    /// Session settings.
    Session,
}

/// Permission rule value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRuleValue {
    /// Tool name.
    #[serde(rename = "toolName")]
    pub tool_name: String,
    /// Rule content (optional).
    #[serde(rename = "ruleContent", skip_serializing_if = "Option::is_none")]
    pub rule_content: Option<String>,
}

/// Permission update configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PermissionUpdate {
    /// Add permission rules.
    AddRules {
        #[serde(skip_serializing_if = "Option::is_none")]
        rules: Option<Vec<PermissionRuleValue>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        behavior: Option<PermissionBehavior>,
        #[serde(skip_serializing_if = "Option::is_none")]
        destination: Option<PermissionUpdateDestination>,
    },
    /// Replace permission rules.
    ReplaceRules {
        #[serde(skip_serializing_if = "Option::is_none")]
        rules: Option<Vec<PermissionRuleValue>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        behavior: Option<PermissionBehavior>,
        #[serde(skip_serializing_if = "Option::is_none")]
        destination: Option<PermissionUpdateDestination>,
    },
    /// Remove permission rules.
    RemoveRules {
        #[serde(skip_serializing_if = "Option::is_none")]
        rules: Option<Vec<PermissionRuleValue>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        behavior: Option<PermissionBehavior>,
        #[serde(skip_serializing_if = "Option::is_none")]
        destination: Option<PermissionUpdateDestination>,
    },
    /// Set permission mode.
    SetMode {
        mode: PermissionMode,
        #[serde(skip_serializing_if = "Option::is_none")]
        destination: Option<PermissionUpdateDestination>,
    },
    /// Add directories.
    AddDirectories {
        directories: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        destination: Option<PermissionUpdateDestination>,
    },
    /// Remove directories.
    RemoveDirectories {
        directories: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        destination: Option<PermissionUpdateDestination>,
    },
}

/// Context information for tool permission callbacks.
#[derive(Debug, Clone)]
pub struct ToolPermissionContext {
    /// Future: abort signal support (currently always None).
    pub signal: Option<()>,
    /// Permission suggestions from CLI.
    pub suggestions: Vec<PermissionUpdate>,
}

/// Permission result for allowing an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionResultAllow {
    /// Behavior is always "allow".
    #[serde(rename = "behavior")]
    pub behavior: String, // Always "allow"
    /// Updated input (optional).
    #[serde(rename = "updatedInput", skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<serde_json::Value>,
    /// Updated permissions (optional).
    #[serde(rename = "updatedPermissions", skip_serializing_if = "Option::is_none")]
    pub updated_permissions: Option<Vec<PermissionUpdate>>,
}

impl Default for PermissionResultAllow {
    fn default() -> Self {
        Self {
            behavior: "allow".to_string(),
            updated_input: None,
            updated_permissions: None,
        }
    }
}

/// Permission result for denying an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionResultDeny {
    /// Behavior is always "deny".
    #[serde(rename = "behavior")]
    pub behavior: String, // Always "deny"
    /// Message explaining the denial.
    #[serde(default)]
    pub message: String,
    /// Whether to interrupt execution.
    #[serde(default)]
    pub interrupt: bool,
}

impl Default for PermissionResultDeny {
    fn default() -> Self {
        Self {
            behavior: "deny".to_string(),
            message: String::new(),
            interrupt: false,
        }
    }
}

/// Permission result (allow or deny).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PermissionResult {
    /// Allow the action.
    Allow(PermissionResultAllow),
    /// Deny the action.
    Deny(PermissionResultDeny),
}

/// Trait for tool permission callbacks.
#[async_trait]
pub trait CanUseTool: Send + Sync {
    /// Check if a tool can be used.
    ///
    /// # Arguments
    /// * `tool_name` - Name of the tool
    /// * `input` - Tool input parameters
    /// * `context` - Permission context
    ///
    /// # Returns
    /// Permission result (allow or deny)
    async fn can_use(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
        context: &ToolPermissionContext,
    ) -> crate::claude::error::Result<PermissionResult>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_mode_serialization() {
        let mode = PermissionMode::AcceptEdits;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, r#""acceptEdits""#);

        let deserialized: PermissionMode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, mode);
    }

    #[test]
    fn test_permission_update_serialization() {
        let update = PermissionUpdate::SetMode {
            mode: PermissionMode::Plan,
            destination: Some(PermissionUpdateDestination::Session),
        };
        let json = serde_json::to_value(&update).unwrap();
        assert_eq!(json["type"], "setMode");
        assert_eq!(json["mode"], "plan");
        assert_eq!(json["destination"], "session");
    }
}
