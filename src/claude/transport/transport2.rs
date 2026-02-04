use crate::claude::ClaudeAgentOptions;
use crate::claude::process_handle::ProcessHandle;
use crate::claude::types::{Error, Result};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tracing::info;

pub type SplitSubprocess = (ChildStdout, ChildStdin, ChildStderr, ProcessHandle);
pub struct SubprocessCLITransport {
    process: Option<Child>,
    options: ClaudeAgentOptions,
    cli_path: PathBuf,
    ready: bool,
}
impl SubprocessCLITransport {
    pub fn new(options: ClaudeAgentOptions) -> Result<Self> {
        let cli_path = if let Some(ref path) = options.cli_path {
            path.clone()
        } else {
            Self::find_cli()?
        };
        Ok(Self {
            process: None,
            options,
            cli_path,
            ready: false,
        })
    }
    /// Find Claude Code CLI binary.
    fn find_cli() -> Result<PathBuf> {
        // Try to find in PATH
        if let Ok(path) = which::which("claude") {
            return Ok(path);
        }

        // Try common installation locations
        let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/"));
        let locations = vec![
            PathBuf::from(&home).join(".npm-global/bin/claude"),
            PathBuf::from("/usr/local/bin/claude"),
            PathBuf::from(&home).join(".local/bin/claude"),
            PathBuf::from(&home).join("node_modules/.bin/claude"),
            PathBuf::from(&home).join(".yarn/bin/claude"),
            PathBuf::from(&home).join(".claude/local/claude"),
        ];

        for path in locations {
            if path.exists() && path.is_file() {
                return Ok(path);
            }
        }

        Err(Error::CLINotFound(
            "Claude Code not found. Install with:\n  npm install -g @anthropic-ai/claude-code\n\
            \nIf already installed locally, try:\n  export PATH=\"$HOME/node_modules/.bin:$PATH\"\n\
            \nOr provide the path via ClaudeAgentOptions:\n  ClaudeAgentOptions { cli_path: Some(\"/path/to/claude\".into()), .. }".to_string()
        ))
    }
    fn build_command(&self) -> Vec<String> {
        let mut cmd = vec![
            self.cli_path.to_string_lossy().to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
        ];

        // System prompt
        if let Some(ref system_prompt) = self.options.system_prompt {
            match system_prompt {
                crate::claude::types::SystemPromptConfig::Preset { preset, append } => {
                    if preset == "claude_code" {
                        if let Some(append_text) = append {
                            cmd.push("--append-system-prompt".to_string());
                            cmd.push(append_text.clone());
                        }
                    }
                }
                crate::claude::types::SystemPromptConfig::Custom { content } => {
                    cmd.push("--system-prompt".to_string());
                    cmd.push(content.clone());
                }
            }
        }
        // If system_prompt is None, don't pass --system-prompt flag
        // This allows Claude Code to use its default system prompt

        // Tools
        if let Some(ref tools) = self.options.tools {
            match tools {
                crate::claude::types::ToolsConfig::Preset { preset, .. } => {
                    if preset == "claude_code" {
                        cmd.push("--tools".to_string());
                        cmd.push("default".to_string());
                    }
                }
                crate::claude::types::ToolsConfig::List(tools_list) => {
                    if tools_list.is_empty() {
                        cmd.push("--tools".to_string());
                        cmd.push(String::new());
                    } else {
                        cmd.push("--tools".to_string());
                        cmd.push(tools_list.join(","));
                    }
                }
            }
        }

        // Allowed tools
        if !self.options.allowed_tools.is_empty() {
            cmd.push("--allowedTools".to_string());
            cmd.push(self.options.allowed_tools.join(","));
        }

        // Max turns
        if let Some(max_turns) = self.options.max_turns {
            cmd.push("--max-turns".to_string());
            cmd.push(max_turns.to_string());
        }

        // Max budget
        if let Some(max_budget) = self.options.max_budget_usd {
            cmd.push("--max-budget-usd".to_string());
            cmd.push(max_budget.to_string());
        }

        // Disallowed tools
        if !self.options.disallowed_tools.is_empty() {
            cmd.push("--disallowedTools".to_string());
            cmd.push(self.options.disallowed_tools.join(","));
        }

        // Model
        if let Some(ref model) = self.options.model {
            cmd.push("--model".to_string());
            cmd.push(model.clone());
        }

        // Fallback model
        if let Some(ref fallback_model) = self.options.fallback_model {
            cmd.push("--fallback-model".to_string());
            cmd.push(fallback_model.clone());
        }

        // Betas
        if !self.options.betas.is_empty() {
            cmd.push("--betas".to_string());
            cmd.push(self.options.betas.join(","));
        }

        // Permission prompt tool
        if let Some(ref tool_name) = self.options.permission_prompt_tool_name {
            cmd.push("--permission-prompt-tool".to_string());
            cmd.push(tool_name.clone());
        }

        // Permission mode
        if let Some(ref mode) = self.options.permission_mode {
            cmd.push("--permission-mode".to_string());
            cmd.push(
                serde_json::to_string(mode)
                    .unwrap_or_default()
                    .trim_matches('"')
                    .to_string(),
            );
        }

        // Continue conversation
        if self.options.continue_conversation {
            cmd.push("--continue".to_string());
        }

        // Resume
        if let Some(ref resume) = self.options.resume {
            cmd.push("--resume".to_string());
            cmd.push(resume.clone());
        }

        // Settings
        if let Some(ref settings) = self.options.settings {
            cmd.push("--settings".to_string());
            cmd.push(settings.clone());
        }

        // Add directories
        for dir in &self.options.add_dirs {
            cmd.push("--add-dir".to_string());
            cmd.push(dir.to_string_lossy().to_string());
        }

        // MCP servers
        if let crate::claude::types::McpServersConfig::Map(ref servers) = self.options.mcp_servers {
            if !servers.is_empty() {
                let servers_json = serde_json::json!({
                    "mcpServers": servers
                });
                cmd.push("--mcp-config".to_string());
                cmd.push(servers_json.to_string());
            }
        }

        // Include partial messages
        if self.options.include_partial_messages {
            cmd.push("--include-partial-messages".to_string());
        }

        // Fork session
        if self.options.fork_session {
            cmd.push("--fork-session".to_string());
        }

        // Agents
        if let Some(ref agents) = self.options.agents {
            let agents_json = serde_json::to_string(agents).unwrap_or_default();
            cmd.push("--agents".to_string());
            cmd.push(agents_json);
        }

        // Setting sources
        if let Some(ref sources) = self.options.setting_sources {
            let sources_str = sources
                .iter()
                .map(|s| {
                    serde_json::to_string(s)
                        .unwrap_or_default()
                        .trim_matches('"')
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join(",");
            cmd.push("--setting-sources".to_string());
            cmd.push(sources_str);
        }

        // Plugins
        for plugin in &self.options.plugins {
            if plugin.type_ == "local" {
                cmd.push("--plugin-dir".to_string());
                cmd.push(plugin.path.clone());
            }
        }

        // Extra args
        for (flag, value) in &self.options.extra_args {
            if let Some(val) = value {
                cmd.push(format!("--{}", flag));
                cmd.push(val.clone());
            } else {
                cmd.push(format!("--{}", flag));
            }
        }

        // Max thinking tokens
        if let Some(max_thinking) = self.options.max_thinking_tokens {
            cmd.push("--max-thinking-tokens".to_string());
            cmd.push(max_thinking.to_string());
        }

        // Output format (JSON schema)
        if let Some(ref output_format) = self.options.output_format {
            if let Some(schema) = output_format.get("schema") {
                cmd.push("--json-schema".to_string());
                cmd.push(schema.to_string());
            }
        }

        // Prompt handling
        // match &self.prompt {
        //     PromptInput::Stream(_) => {
        cmd.push("--input-format".to_string());
        cmd.push("stream-json".to_string());
        //     }
        //     PromptInput::String(s) => {
        //         cmd.push("--print".to_string());
        //         cmd.push("--".to_string());
        //         cmd.push(s.clone());
        //     }
        // }

        cmd
    }
    pub async fn connect(&mut self) -> Result<()> {
        if self.process.is_some() {
            return Ok(());
        }

        let cmd_args = self.build_command();
        println!("Starting Claude CLI: {:?}", cmd_args);

        let mut command = Command::new(&cmd_args[0]);
        command.args(&cmd_args[1..]);

        // Set up stdio
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        // Set working directory if specified
        if let Some(ref cwd) = self.options.cwd {
            command.current_dir(cwd);
        }

        // Set environment variables
        for (key, value) in &self.options.env {
            command.env(key, value);
        }
        command.env("CLAUDE_CODE_ENTRYPOINT", "sdk-ts");

        // Enable file checkpointing if requested
        if self.options.enable_file_checkpointing {
            command.env("CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING", "true");
        }

        // Spawn process
        let child = command
            .spawn()
            .map_err(|e| Error::Process(format!("Failed to spawn Claude CLI: {}", e)))?;

        self.process = Some(child);
        self.ready = true;

        println!("Claude CLI process started successfully");
        Ok(())
    }

    pub fn split(mut self) -> Result<SplitSubprocess> {
        // Ensure process is started
        let mut child = self.process.take().ok_or_else(|| {
            Error::Process("Process not started. Call connect() first.".to_string())
        })?;

        // Take stdin, stdout, stderr
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Process("stdin not available".to_string()))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Process("stdout not available".to_string()))?;

        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::Process("stderr not available".to_string()))?;

        // Return the handles directly
        let process_handle = ProcessHandle::new(child);

        Ok((stdout, stdin, stderr, process_handle))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::transport::stream::ReadHalf;
    use futures::StreamExt;
    use std::collections::HashMap;
    use std::time::Duration;

    #[tokio::test]
    async fn test() {
        let mut options = ClaudeAgentOptions::new();
        let mut map = HashMap::new();
        let id = uuid::Uuid::new_v4().to_string();
        println!("id: {}", id);
        map.insert("session-id".to_string(), Some(id));
        options.extra_args = map;

        let mut t = SubprocessCLITransport::new(options).unwrap();
        t.connect().await.unwrap();

        let (stdout, stdin, e, h) = t.split().unwrap();
        tokio::spawn(async move {
            let mut r = ReadHalf::new(stdout, "".into());
            // while let Some(Ok(events)) = r.next_events().await {
            //     for e in events {
            //         // println!("universal event: {:#?}", e);
            //     }
            //
            // }
        });

        // let w = WriteHalf::new(stdin);
        // w.send_user_message("who are you".to_string()).await.unwrap();
        // tokio::time::sleep(Duration::from_secs(10000)).await;
    }
}
