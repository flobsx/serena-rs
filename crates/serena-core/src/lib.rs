//! Serena.rs — Agent orchestration, modes, and agent context
//!
//! The core orchestrator that ties together all components: config loading,
//! project management, memory, and tool routing.

pub mod agent;
pub mod context;
pub mod mode;

pub use agent::SerenaAgent;
pub use context::AgentContext;
pub use mode::AgentMode;
