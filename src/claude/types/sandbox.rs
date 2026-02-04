//! Sandbox configuration types for Claude Agent SDK.

use serde::{Deserialize, Serialize};

/// Network configuration for sandbox.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxNetworkConfig {
    /// Unix socket paths accessible in sandbox (e.g., SSH agents).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_unix_sockets: Option<Vec<String>>,
    /// Allow all Unix sockets (less secure).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_all_unix_sockets: Option<bool>,
    /// Allow binding to localhost ports (macOS only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_local_binding: Option<bool>,
    /// HTTP proxy port if bringing your own proxy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_proxy_port: Option<i32>,
    /// SOCKS5 proxy port if bringing your own proxy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socks_proxy_port: Option<i32>,
}

/// Violations to ignore in sandbox.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SandboxIgnoreViolations {
    /// File paths for which violations should be ignored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<Vec<String>>,
    /// Network hosts for which violations should be ignored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<Vec<String>>,
}

/// Sandbox settings configuration.
///
/// This controls how Claude Code sandboxes bash commands for filesystem
/// and network isolation.
///
/// **Important:** Filesystem and network restrictions are configured via permission
/// rules, not via these sandbox settings:
/// - Filesystem read restrictions: Use Read deny rules
/// - Filesystem write restrictions: Use Edit allow/deny rules
/// - Network restrictions: Use WebFetch allow/deny rules
///
/// # Example
///
/// ```rust
/// use claude_agent_sdk::types::sandbox::{SandboxSettings, SandboxNetworkConfig};
///
/// let sandbox_settings = SandboxSettings {
///     enabled: Some(true),
///     auto_allow_bash_if_sandboxed: Some(true),
///     excluded_commands: Some(vec!["docker".to_string()]),
///     network: Some(SandboxNetworkConfig {
///         allow_unix_sockets: Some(vec!["/var/run/docker.sock".to_string()]),
///         allow_local_binding: Some(true),
///         ..Default::default()
///     }),
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxSettings {
    /// Enable bash sandboxing (macOS/Linux only). Default: false
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Auto-approve bash commands when sandboxed. Default: true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_allow_bash_if_sandboxed: Option<bool>,
    /// Commands that should run outside the sandbox (e.g., ["git", "docker"])
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excluded_commands: Option<Vec<String>>,
    /// Allow commands to bypass sandbox via dangerouslyDisableSandbox.
    /// When false, all commands must run sandboxed (or be in excludedCommands). Default: true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_unsandboxed_commands: Option<bool>,
    /// Network configuration for sandbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<SandboxNetworkConfig>,
    /// Violations to ignore.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_violations: Option<SandboxIgnoreViolations>,
    /// Enable weaker sandbox for unprivileged Docker environments (Linux only).
    /// Reduces security. Default: false
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_weaker_nested_sandbox: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_settings_default() {
        let settings = SandboxSettings::default();
        assert!(settings.enabled.is_none());
        assert!(settings.network.is_none());
    }

    #[test]
    fn test_sandbox_settings_serialization() {
        let settings = SandboxSettings {
            enabled: Some(true),
            auto_allow_bash_if_sandboxed: Some(true),
            excluded_commands: Some(vec!["docker".to_string(), "git".to_string()]),
            network: Some(SandboxNetworkConfig {
                allow_unix_sockets: Some(vec!["/var/run/docker.sock".to_string()]),
                allow_local_binding: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };

        let json = serde_json::to_value(&settings).unwrap();
        assert_eq!(json["enabled"], true);
        assert_eq!(json["autoAllowBashIfSandboxed"], true);
        assert_eq!(json["excludedCommands"][0], "docker");
        assert_eq!(json["network"]["allowLocalBinding"], true);
    }

    #[test]
    fn test_sandbox_network_config_serialization() {
        let config = SandboxNetworkConfig {
            allow_unix_sockets: Some(vec!["/tmp/socket".to_string()]),
            http_proxy_port: Some(8080),
            ..Default::default()
        };

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["allowUnixSockets"][0], "/tmp/socket");
        assert_eq!(json["httpProxyPort"], 8080);
    }
}
