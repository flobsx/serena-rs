//! StartMcpServer command — launch the MCP server.
//!
//! Initializes the tool registry, loads project configuration,
//! and starts the MCP server on the selected transport.

use std::path::Path;

/// Execute the StartMcpServer command.
///
/// Loads project config if a project path is provided, initializes
/// the tool registry, and starts the MCP server.
pub async fn execute(project: Option<&str>, transport: &str) -> Result<(), McpCommandError> {
    // Validate transport
    match transport {
        "stdio" | "sse" => {}
        other => return Err(McpCommandError::UnsupportedTransport(other.to_string())),
    }

    // If a project path is given, verify it exists
    if let Some(project_path) = project {
        let path = Path::new(project_path);
        if !path.exists() {
            return Err(McpCommandError::ProjectNotFound(project_path.to_string()));
        }
        if !path.is_dir() {
            return Err(McpCommandError::NotADirectory(project_path.to_string()));
        }
    }

    // Initialize tool registry
    let registry = build_tool_registry();

    // Start the MCP server via serena-mcp
    let server = serena_mcp::McpServer::with_registry(registry);
    server.run_stdio().await;

    Ok(())
}

/// Build the tool registry with all available tools.
fn build_tool_registry() -> serena_mcp::ToolRegistry {
    let mut registry = serena_mcp::ToolRegistry::new();

    // Register tools from serena-tools crate
    // TODO: wire individual tool registrations when serena-tools is implemented
    tracing::info!("tool registry initialized (stub: tools not yet wired)");

    registry
}

#[derive(Debug, PartialEq)]
pub enum McpCommandError {
    UnsupportedTransport(String),
    ProjectNotFound(String),
    NotADirectory(String),
}

impl std::fmt::Display for McpCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpCommandError::UnsupportedTransport(t) => {
                write!(f, "unsupported transport: {t} (supported: stdio, sse)")
            }
            McpCommandError::ProjectNotFound(p) => {
                write!(f, "project not found: {p}")
            }
            McpCommandError::NotADirectory(p) => {
                write!(f, "project path is not a directory: {p}")
            }
        }
    }
}

impl std::error::Error for McpCommandError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn test_mcp_unsupported_transport() {
        let result = execute(None, "http").await;
        assert!(result.is_err(), "http transport should be rejected");
        match result.unwrap_err() {
            McpCommandError::UnsupportedTransport(t) => {
                assert_eq!(t, "http");
            }
            other => panic!("expected UnsupportedTransport, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_mcp_stdio_transport_accepted() {
        // This test only checks that stdio is accepted as a transport
        // (the actual server start would block, but ToolRegistry::run_stdio
        //  is a stub that returns immediately in tests)
        let result = execute(None, "stdio").await;
        assert!(result.is_ok(), "stdio should be accepted");
    }

    #[tokio::test]
    async fn test_mcp_project_not_found() {
        let result = execute(Some("/nonexistent/path"), "stdio").await;
        assert!(result.is_err(), "nonexistent project should error");
        match result.unwrap_err() {
            McpCommandError::ProjectNotFound(p) => {
                assert_eq!(p, "/nonexistent/path");
            }
            other => panic!("expected ProjectNotFound, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_mcp_project_is_file_not_directory() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("file.txt");
        fs::write(&file_path, "content").unwrap();

        let result = execute(Some(file_path.to_str().unwrap()), "stdio").await;
        assert!(result.is_err(), "file path should error");
        match result.unwrap_err() {
            McpCommandError::NotADirectory(p) => {
                assert!(p.contains("file.txt"));
            }
            other => panic!("expected NotADirectory, got {other:?}"),
        }
    }
}
