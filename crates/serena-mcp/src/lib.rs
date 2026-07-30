//! Serena.rs — MCP server (stdio transport, tool registration)
//!
//! Wraps all tool implementations into MCP-compatible tools using rmcp.
//! Supports stdio transport (V1) with SSE/HTTP planned for later.

pub mod server;
pub mod transport;
pub mod registry;

pub use server::McpServer;
pub use transport::Transport;
pub use registry::ToolRegistry;
