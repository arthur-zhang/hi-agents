//! Agent backend implementations

pub mod claude;

pub use claude::ClaudeBackend;

use super::{AgentBackend, AgentType};

/// Create an agent backend based on type
pub fn create_backend(agent_type: AgentType) -> Box<dyn AgentBackend> {
    match agent_type {
        AgentType::Claude => Box::new(ClaudeBackend::new()),
        AgentType::Codex => {
            // TODO: Implement CodexBackend
            unimplemented!("Codex backend not yet implemented")
        }
        AgentType::OpenCode => {
            // TODO: Implement OpenCodeBackend
            unimplemented!("OpenCode backend not yet implemented")
        }
    }
}
