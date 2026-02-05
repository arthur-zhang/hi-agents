//! Agent abstraction layer
//!
//! This module provides a unified interface for different AI agent backends
//! (Claude, Codex, OpenCode, etc.) to integrate with the ACP protocol.

mod traits;
pub mod backends;

pub use traits::*;
