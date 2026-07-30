//! MCP server implementation using rmcp.
//!
//! Manages the MCP protocol lifecycle: initialization, tool listing,
//! tool execution, and transport.

use std::borrow::Cow;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

use crate::error::McpErrorExt;
use crate::registry::{ToolHandler, ToolRegistry};

use rmcp::model::*;
use rmcp::{
    RoleServer, ServerHandler,
    service::RequestContext,
};

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
    pub async fn run_stdio(self) {
        info!("Starting MCP server (stdio transport)");

        let count = self.tool_count().await;
        info!(count, "MCP server ready");

        // Build stdio transport: stdin + stdout
        // (tokio::io::Stdin, tokio::io::Stdout) implements IntoTransport
        let transport = (tokio::io::stdin(), tokio::io::stdout());

        // Serve the MCP protocol — this blocks until the client disconnects
        if let Err(e) = rmcp::serve_server(self, transport).await {
            info!(error = %e, "MCP server stopped");
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

// =============================================================================
// rmcp ServerHandler implementation
// =============================================================================

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation {
                name: "serena-rs".to_string(),
                title: Some("Serena.rs — MCP Toolkit".to_string()),
                version: env!("CARGO_PKG_VERSION").to_string(),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Serena.rs — MCP protocol server for coding agents. "
                    .to_string()
            ),
        }
    }

    async fn initialize(
        &self,
        request: InitializeRequestParam,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, rmcp::ErrorData> {
        info!(
            client = %request.client_info.name,
            version = %request.client_info.version,
            protocol = %request.protocol_version,
            "Client initializing"
        );
        if context.peer.peer_info().is_none() {
            context.peer.set_peer_info(request);
        }
        Ok(self.get_info())
    }

    async fn on_initialized(
        &self,
        _context: rmcp::service::NotificationContext<RoleServer>,
    ) {
        info!("Client initialized — ready to serve requests");
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, rmcp::ErrorData> {
        let tools = self.registry.lock().await.mcp_tool_list();
        let rmcp_tools: Vec<Tool> = tools.into_iter().map(convert_tool_def).collect();
        Ok(ListToolsResult::with_all_items(rmcp_tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        dispatch_tool_call(&self.registry, &request.name, request.arguments).await
    }
}

// =============================================================================
// Tool dispatch
// =============================================================================

/// Dispatch a tool call by name, looking up the handler in the registry.
///
/// Returns an MCP `CallToolResult` — successful or error.
pub async fn dispatch_tool_call(
    registry: &Arc<Mutex<ToolRegistry>>,
    name: &str,
    arguments: Option<JsonObject>,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let reg = registry.lock().await;
    let handler = reg.get(name).ok_or_else(|| rmcp::ErrorData::tool_not_found(name))?;

    let params_value = match arguments {
        Some(map) => serde_json::Value::Object(map),
        None => serde_json::json!({}),
    };

    match (handler.handler)(params_value).await {
        Ok(result) => {
            let text = if result.is_object() || result.is_array() {
                serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|_| result.to_string())
            } else {
                result.to_string()
            };
            Ok(CallToolResult::success(vec![Content::text(text)]))
        }
        Err(err_msg) => {
            Ok(CallToolResult::error(vec![Content::text(err_msg)]))
        }
    }
}

// =============================================================================
// Conversion helpers
// =============================================================================

/// Convert our `McpToolDefinition` into rmcp's `Tool`.
fn convert_tool_def(def: crate::registry::McpToolDefinition) -> Tool {
    let schema_value = def.input_schema;
    let schema_map = match schema_value {
        serde_json::Value::Object(map) => map,
        other => {
            // Wrap non-object schemas
            let mut map = serde_json::Map::new();
            map.insert("schema".to_string(), other);
            map
        }
    };

    Tool {
        name: Cow::Owned(def.name.to_string()),
        title: None,
        description: Some(Cow::Borrowed(def.description)),
        input_schema: Arc::new(schema_map),
        output_schema: None,
        annotations: None,
        icons: None,
    }
}

// =============================================================================
// Built-in tool registration
// =============================================================================

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

// =============================================================================
// Tests
// =============================================================================

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

    // -- ServerHandler tests --

    #[tokio::test]
    async fn test_get_info_returns_serena_name() {
        let server = McpServer::new();
        let info: ServerInfo = server.get_info();
        assert_eq!(info.server_info.name, "serena-rs");
        assert_eq!(info.server_info.version, env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn test_get_info_has_tools_capability() {
        let server = McpServer::new();
        let info = server.get_info();
        assert!(
            info.capabilities.tools.is_some(),
            "ServerInfo should advertise tools capability"
        );
    }

    #[tokio::test]
    async fn test_get_info_has_instructions() {
        let server = McpServer::new();
        let info = server.get_info();
        assert!(
            info.instructions.is_some(),
            "ServerInfo should have instructions"
        );
        let instr = info.instructions.unwrap();
        assert!(instr.contains("Serena"));
    }

    #[tokio::test]
    async fn test_mcp_tool_def_to_rmcp_conversion() {
        let server = McpServer::new();
        server.register_tool(ToolHandler {
            name: "my_tool",
            description: "Does something useful",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "input": { "type": "string" }
                },
                "required": ["input"]
            }),
            handler: Box::new(|_| Box::pin(async move { Ok(serde_json::json!({"ok": true})) })),
        }).await;

        let tools = server.tool_list().await;
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "my_tool");
        assert_eq!(tools[0].description, "Does something useful");
    }

    #[tokio::test]
    async fn test_call_tool_integration_via_registry() {
        let server = McpServer::new();
        server.register_tool(ToolHandler {
            name: "echo",
            description: "Echo input",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "msg": { "type": "string" }
                }
            }),
            handler: Box::new(|params| {
                Box::pin(async move { Ok(params) })
            }),
        }).await;

        // Test via internal dispatch helper
        let result = dispatch_tool_call(
            &server.registry,
            "echo",
            serde_json::json!({"msg": "hello"}).as_object().cloned(),
        ).await;

        assert!(result.is_ok(), "dispatch_tool_call should succeed: {:?}", result.err());
        let call_result = result.unwrap();
        assert_eq!(call_result.is_error, Some(false), "should not be an error");
    }

    #[tokio::test]
    async fn test_call_tool_not_found() {
        let server = McpServer::new();
        let result = dispatch_tool_call(
            &server.registry,
            "nonexistent",
            None,
        ).await;

        assert!(result.is_err(), "calling nonexistent tool should error");
    }
}
