//! MCP (Model Context Protocol) server configuration types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// MCP server configuration (union of different server types).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpServerConfig {
    /// Stdio server configuration.
    #[serde(rename = "stdio")]
    Stdio {
        command: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        args: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        env: Option<HashMap<String, String>>,
    },
    /// SSE (Server-Sent Events) server configuration.
    Sse {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<HashMap<String, String>>,
    },
    /// HTTP server configuration.
    Http {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<HashMap<String, String>>,
    },
    /// SDK server configuration.
    Sdk {
        name: String,
        /// MCP Server instance (skipped during serialization).
        /// Use Arc<dyn Any> for type erasure since we can't reference the MCP Server trait directly.
        #[serde(skip)]
        instance: Option<Arc<dyn std::any::Any + Send + Sync>>,
    },
}

/// SDK plugin configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkPluginConfig {
    /// Plugin type (currently only "local" is supported).
    #[serde(rename = "type")]
    pub type_: String,
    /// Path to the plugin.
    pub path: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_stdio_config_serialization() {
        let config = McpServerConfig::Stdio {
            command: "node".to_string(),
            args: Some(vec!["server.js".to_string()]),
            env: None,
        };
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["type"], "stdio");
        assert_eq!(json["command"], "node");
        assert_eq!(json["args"][0], "server.js");
    }

    #[test]
    fn test_mcp_sse_config_serialization() {
        let config = McpServerConfig::Sse {
            url: "https://example.com/sse".to_string(),
            headers: Some(HashMap::from([(
                "Authorization".to_string(),
                "Bearer token".to_string(),
            )])),
        };
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["type"], "sse");
        assert_eq!(json["url"], "https://example.com/sse");
    }

    #[test]
    fn test_sdk_plugin_config_serialization() {
        let config = SdkPluginConfig {
            type_: "local".to_string(),
            path: "/path/to/plugin".to_string(),
        };
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["type"], "local");
        assert_eq!(json["path"], "/path/to/plugin");
    }
}
