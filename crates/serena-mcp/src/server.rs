//! MCP server implementation using rmcp.
//!
//! Manages the MCP protocol lifecycle: initialization, tool listing,
//! tool execution, and transport.

use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

use crate::registry::{ToolHandler, ToolRegistry};

/// The Serena MCP server.
pub struct McpServer {
    /// Tool registry with all registered handlers
    registry: Arc<Mutex<ToolRegistry>>,
}

impl McpServer {
    /// Create a new MCP server with an empty tool registry.
    pub fn new() -> Self {
        Self {
            registry: Arc::new(Mutex::new(ToolRegistry::new())),
        }
    }

    /// Create a server with a pre-populated registry (for testing).
    pub fn with_registry(registry: ToolRegistry) -> Self {
        Self {
            registry: Arc::new(Mutex::new(registry)),
        }
    }

    /// Register a tool handler with the server.
    pub async fn register_tool(&self, handler: ToolHandler) {
        let mut reg = self.registry.lock().await;
        reg.register(handler);
    }

    /// Run the MCP server over stdio transport.
    pub async fn run_stdio(&self) {
        info!("Starting MCP server (stdio transport)");

        let tools = {
            let reg = self.registry.lock().await;
            reg.mcp_tool_list()
        };

        info!(count = tools.len(), "MCP server ready");

        // Keep running until interrupted (actual MCP protocol handling
        // via rmcp will be added in a future iteration)
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
        }
    }

    /// Get the number of registered tools.
    pub async fn tool_count(&self) -> usize {
        self.registry.lock().await.len()
    }

    /// Get the MCP tool list for testing/diagnostics.
    pub async fn tool_list(&self) -> Vec<crate::registry::McpToolDefinition> {
        self.registry.lock().await.mcp_tool_list()
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Register all built-in tools with the server.
pub async fn register_builtin_tools(server: &McpServer) {
    register_symbol_tools(server).await;
    info!("All built-in tools registered");
}

/// Register symbol/language-server tools.
async fn register_symbol_tools(server: &McpServer) {
    use std::sync::Arc as StdArc;
    let manager = StdArc::new(serena_tools::symbol_tools::SymbolToolManager::new());

    // get_symbols tool
    server.register_tool(ToolHandler {
        name: "get_symbols",
        description: "Get all symbols (functions, classes, variables, etc.) from a source file using the appropriate language server",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the source file"
                },
                "text": {
                    "type": "string",
                    "description": "Optional file content to send to the LSP server"
                }
            },
            "required": ["file_path"]
        }),
        handler: Box::new({
            let mgr = manager.clone();
            move |params| {
                let mgr = mgr.clone();
                Box::pin(async move {
                    let file_path = params.get("file_path")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "Missing required parameter: file_path".to_string())?;
                    let text = params.get("text").and_then(|v| v.as_str());

                    let symbols = mgr.get_symbols(file_path, text).await?;
                    Ok(serde_json::json!({ "symbols": symbols }))
                })
            }
        }),
    }).await;

    // find_symbol tool
    server.register_tool(ToolHandler {
        name: "find_symbol",
        description: "Find symbols matching a query in a source file using the appropriate language server",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the source file"
                },
                "query": {
                    "type": "string",
                    "description": "Symbol name or pattern to search for"
                },
                "text": {
                    "type": "string",
                    "description": "Optional file content"
                }
            },
            "required": ["file_path", "query"]
        }),
        handler: Box::new({
            let mgr = manager.clone();
            move |params| {
                let mgr = mgr.clone();
                Box::pin(async move {
                    let file_path = params.get("file_path")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "Missing required parameter: file_path".to_string())?;
                    let query = params.get("query")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "Missing required parameter: query".to_string())?;
                    let text = params.get("text").and_then(|v| v.as_str());

                    let results = mgr.find_symbol(file_path, query, text).await?;
                    Ok(serde_json::json!({ "results": results }))
                })
            }
        }),
    }).await;

    // list_symbols tool
    server.register_tool(ToolHandler {
        name: "list_symbols",
        description: "List all symbols in a file as a flat list with hierarchy",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the source file"
                },
                "text": {
                    "type": "string",
                    "description": "Optional file content"
                }
            },
            "required": ["file_path"]
        }),
        handler: Box::new({
            let mgr = manager;
            move |params| {
                let mgr = mgr.clone();
                Box::pin(async move {
                    let file_path = params.get("file_path")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "Missing required parameter: file_path".to_string())?;
                    let text = params.get("text").and_then(|v| v.as_str());

                    let entries = mgr.list_symbols(file_path, text).await?;
                    Ok(serde_json::json!({ "symbols": entries }))
                })
            }
        }),
    }).await;

    info!("Symbol tools registered");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_server_creation() {
        let server = McpServer::new();
        assert_eq!(server.tool_count().await, 0);
    }

    #[tokio::test]
    async fn test_register_and_count_tools() {
        let server = McpServer::new();
        server.register_tool(ToolHandler {
            name: "test",
            description: "test tool",
            input_schema: serde_json::json!({}),
            handler: Box::new(|_| Box::pin(async move { Ok(serde_json::json!({})) })),
        }).await;
        assert_eq!(server.tool_count().await, 1);
    }

    #[tokio::test]
    async fn test_register_builtin_symbol_tools() {
        let server = McpServer::new();
        register_builtin_tools(&server).await;

        let count = server.tool_count().await;
        assert!(count >= 3, "Expected at least 3 symbol tools, got {count}");

        let tools = server.tool_list().await;
        let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
        assert!(names.contains(&"get_symbols"));
        assert!(names.contains(&"find_symbol"));
        assert!(names.contains(&"list_symbols"));
    }

    #[tokio::test]
    async fn test_with_registry() {
        let mut registry = ToolRegistry::new();
        registry.register(ToolHandler {
            name: "pre_registered",
            description: "already registered",
            input_schema: serde_json::json!({}),
            handler: Box::new(|_| Box::pin(async move { Ok(serde_json::json!({})) })),
        });

        let server = McpServer::with_registry(registry);
        assert_eq!(server.tool_count().await, 1);
    }
}
