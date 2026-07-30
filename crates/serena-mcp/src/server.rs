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
    register_file_tools(server).await;
    register_search_tools(server).await;
    register_shell_tools(server).await;
    register_memory_tools(server).await;
    register_config_tools(server).await;
    register_lsp_tools(server).await;
    register_workflow_tools(server).await;
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

// ---------------------------------------------------------------------------
// File tools
// ---------------------------------------------------------------------------

/// Register file-system tools (read_file, list_dir, find_file, replace_content).
async fn register_file_tools(server: &McpServer) {
    // read_file tool
    server.register_tool(ToolHandler {
        name: "read_file",
        description: "Read the entire contents of a file as text",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to read"
                }
            },
            "required": ["file_path"]
        }),
        handler: Box::new(|params| {
            Box::pin(async move {
                let file_path = params.get("file_path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter: file_path".to_string())?;
                let content = serena_tools::file_tools::read_file(file_path).await?;
                Ok(serde_json::json!({ "content": content }))
            })
        }),
    }).await;

    // list_dir tool
    server.register_tool(ToolHandler {
        name: "list_dir",
        description: "List all entries (files, directories, symlinks) in a directory",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "dir_path": {
                    "type": "string",
                    "description": "Path to the directory to list"
                }
            },
            "required": ["dir_path"]
        }),
        handler: Box::new(|params| {
            Box::pin(async move {
                let dir_path = params.get("dir_path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter: dir_path".to_string())?;
                let entries = serena_tools::file_tools::list_dir(dir_path).await?;
                Ok(serde_json::json!({ "entries": entries }))
            })
        }),
    }).await;

    // find_file tool
    server.register_tool(ToolHandler {
        name: "find_file",
        description: "Find files matching a pattern in a directory tree (supports *.rs suffix patterns)",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "dir_path": {
                    "type": "string",
                    "description": "Root directory to search in"
                },
                "pattern": {
                    "type": "string",
                    "description": "File pattern (e.g. *.rs, test* or exact name)"
                }
            },
            "required": ["dir_path", "pattern"]
        }),
        handler: Box::new(|params| {
            Box::pin(async move {
                let dir_path = params.get("dir_path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter: dir_path".to_string())?;
                let pattern = params.get("pattern")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter: pattern".to_string())?;
                let results = serena_tools::file_tools::find_file(dir_path, pattern).await?;
                Ok(serde_json::json!({ "files": results }))
            })
        }),
    }).await;

    // replace_content tool
    server.register_tool(ToolHandler {
        name: "replace_content",
        description: "Search and replace text in a file (simple string replacement, not regex)",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file to modify"
                },
                "search": {
                    "type": "string",
                    "description": "Text to search for"
                },
                "replacement": {
                    "type": "string",
                    "description": "Text to replace matches with"
                }
            },
            "required": ["file_path", "search", "replacement"]
        }),
        handler: Box::new(|params| {
            Box::pin(async move {
                let file_path = params.get("file_path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter: file_path".to_string())?;
                let search = params.get("search")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter: search".to_string())?;
                let replacement = params.get("replacement")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter: replacement".to_string())?;
                let count = serena_tools::file_tools::replace_content(file_path, search, replacement).await?;
                Ok(serde_json::json!({ "replacements": count }))
            })
        }),
    }).await;

    info!("File tools registered");
}

// ---------------------------------------------------------------------------
// Search tools
// ---------------------------------------------------------------------------

/// Register search tools (search_for_pattern).
async fn register_search_tools(server: &McpServer) {
    server.register_tool(ToolHandler {
        name: "search_for_pattern",
        description: "Search file contents for a regex pattern and return matching lines",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file to search in"
                },
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to search for"
                }
            },
            "required": ["file_path", "pattern"]
        }),
        handler: Box::new(|params| {
            Box::pin(async move {
                let file_path = params.get("file_path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter: file_path".to_string())?;
                let pattern = params.get("pattern")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter: pattern".to_string())?;
                let matches = serena_tools::search_tools::search_for_pattern(file_path, pattern).await?;
                Ok(serde_json::json!({ "matches": matches }))
            })
        }),
    }).await;

    info!("Search tools registered");
}

// ---------------------------------------------------------------------------
// Shell tools
// ---------------------------------------------------------------------------

/// Register shell tools (execute_shell_command).
async fn register_shell_tools(server: &McpServer) {
    server.register_tool(ToolHandler {
        name: "execute_shell_command",
        description: "Execute a shell command and return stdout, stderr, and exit code",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to execute"
                }
            },
            "required": ["command"]
        }),
        handler: Box::new(|params| {
            Box::pin(async move {
                let command = params.get("command")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter: command".to_string())?;
                let result = serena_tools::shell_tools::execute_shell_command(command).await?;
                Ok(serde_json::to_value(result).unwrap_or_default())
            })
        }),
    }).await;

    info!("Shell tools registered");
}

// ---------------------------------------------------------------------------
// Memory tools
// ---------------------------------------------------------------------------

/// Register memory tools (write_memory, read_memory, search_memories, delete_memory).
async fn register_memory_tools(server: &McpServer) {
    // Initialise memory store default location
    let base_dir = std::env::current_dir()
        .unwrap_or_default()
        .join(".serena")
        .join("memories");
    serena_tools::memory_tools::init_memory_store(
        base_dir.to_str().unwrap_or(".serena/memories")
    );

    // write_memory tool
    server.register_tool(ToolHandler {
        name: "write_memory",
        description: "Store a persistent memory with a tag and content. Returns the generated memory id.",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "tag": {
                    "type": "string",
                    "description": "Semantic tag/category for the memory"
                },
                "content": {
                    "type": "string",
                    "description": "Memory content to store"
                }
            },
            "required": ["tag", "content"]
        }),
        handler: Box::new(|params| {
            Box::pin(async move {
                let tag = params.get("tag")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter: tag".to_string())?;
                let content = params.get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter: content".to_string())?;
                let id = serena_tools::memory_tools::write_memory(tag, content).await?;
                Ok(serde_json::json!({ "id": id }))
            })
        }),
    }).await;

    // read_memory tool
    server.register_tool(ToolHandler {
        name: "read_memory",
        description: "Read a stored memory by its id",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Memory id to retrieve"
                }
            },
            "required": ["id"]
        }),
        handler: Box::new(|params| {
            Box::pin(async move {
                let id = params.get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter: id".to_string())?;
                let content = serena_tools::memory_tools::read_memory(id).await?;
                Ok(serde_json::json!({ "content": content }))
            })
        }),
    }).await;

    // search_memories tool
    server.register_tool(ToolHandler {
        name: "search_memories",
        description: "Search stored memories by tag (empty tag returns all)",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "tag": {
                    "type": "string",
                    "description": "Tag to search for (empty string returns all)"
                }
            },
            "required": ["tag"]
        }),
        handler: Box::new(|params| {
            Box::pin(async move {
                let tag = params.get("tag")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter: tag".to_string())?;
                let results = serena_tools::memory_tools::search_memories(tag).await?;
                Ok(serde_json::json!({ "memories": results }))
            })
        }),
    }).await;

    // delete_memory tool
    server.register_tool(ToolHandler {
        name: "delete_memory",
        description: "Delete a stored memory by its id",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Memory id to delete"
                }
            },
            "required": ["id"]
        }),
        handler: Box::new(|params| {
            Box::pin(async move {
                let id = params.get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter: id".to_string())?;
                serena_tools::memory_tools::delete_memory(id).await?;
                Ok(serde_json::json!({ "deleted": true }))
            })
        }),
    }).await;

    info!("Memory tools registered");
}

// ---------------------------------------------------------------------------
// Config tools
// ---------------------------------------------------------------------------

/// Register config tools (get_config, set_config, list_config).
async fn register_config_tools(server: &McpServer) {
    // Initialise config store default location
    let config_dir = std::env::current_dir()
        .unwrap_or_default()
        .join(".serena")
        .join("config.json");
    serena_tools::config_tools::init_config(
        config_dir.to_str().unwrap_or(".serena/config.json")
    );

    // get_config tool
    server.register_tool(ToolHandler {
        name: "get_config",
        description: "Get a configuration value by key",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "key": {
                    "type": "string",
                    "description": "Configuration key to retrieve"
                }
            },
            "required": ["key"]
        }),
        handler: Box::new(|params| {
            Box::pin(async move {
                let key = params.get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter: key".to_string())?;
                let value = serena_tools::config_tools::get_config(key).await?;
                Ok(serde_json::json!({ "value": value }))
            })
        }),
    }).await;

    // set_config tool
    server.register_tool(ToolHandler {
        name: "set_config",
        description: "Set a configuration value",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "key": {
                    "type": "string",
                    "description": "Configuration key to set"
                },
                "value": {
                    "description": "Value to store (any JSON type)"
                }
            },
            "required": ["key", "value"]
        }),
        handler: Box::new(|params| {
            Box::pin(async move {
                let key = params.get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter: key".to_string())?;
                let value = params.get("value")
                    .cloned()
                    .ok_or_else(|| "Missing required parameter: value".to_string())?;
                serena_tools::config_tools::set_config(key, value).await?;
                Ok(serde_json::json!({ "success": true }))
            })
        }),
    }).await;

    // list_config tool
    server.register_tool(ToolHandler {
        name: "list_config",
        description: "List all configuration keys and values",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {}
        }),
        handler: Box::new(|_params| {
            Box::pin(async move {
                let config = serena_tools::config_tools::list_config().await?;
                Ok(serde_json::json!({ "config": config }))
            })
        }),
    }).await;

    info!("Config tools registered");
}

// ---------------------------------------------------------------------------
// LSP tools
// ---------------------------------------------------------------------------

/// Register LSP tools (get_definition, find_references, get_hover, etc.).
async fn register_lsp_tools(server: &McpServer) {
    use std::sync::Arc as StdArc;
    let manager = StdArc::new(serena_tools::symbol_tools::SymbolToolManager::new());

    // get_definition tool
    server.register_tool(ToolHandler {
        name: "get_definition",
        description: "Get the definition location of a symbol at a given position in a file using the language server",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "Path to the source file" },
                "line": { "type": "integer", "description": "Line number (0-indexed)" },
                "column": { "type": "integer", "description": "Column number (0-indexed)" },
                "text": { "type": "string", "description": "Optional file content to send to the LSP server" }
            },
            "required": ["file_path", "line", "column"]
        }),
        handler: Box::new({
            let mgr = manager.clone();
            move |params| {
                let mgr = mgr.clone();
                Box::pin(async move {
                    let file_path = params.get("file_path").and_then(|v| v.as_str()).ok_or_else(|| "Missing required parameter: file_path".to_string())?;
                    let line = params.get("line").and_then(|v| v.as_u64()).ok_or_else(|| "Missing required parameter: line".to_string())? as u32;
                    let column = params.get("column").and_then(|v| v.as_u64()).ok_or_else(|| "Missing required parameter: column".to_string())? as u32;
                    let text = params.get("text").and_then(|v| v.as_str());
                    let locations = mgr.get_definition(file_path, line, column, text).await?;
                    Ok(serde_json::json!({ "locations": locations }))
                })
            }
        }),
    }).await;

    // find_references tool
    server.register_tool(ToolHandler {
        name: "find_references",
        description: "Find all references to a symbol at a given position",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "Path to the source file" },
                "line": { "type": "integer", "description": "Line number (0-indexed)" },
                "column": { "type": "integer", "description": "Column number (0-indexed)" },
                "include_declaration": { "type": "boolean", "description": "Whether to include the declaration site" },
                "text": { "type": "string", "description": "Optional file content to send to the LSP server" }
            },
            "required": ["file_path", "line", "column"]
        }),
        handler: Box::new({
            let mgr = manager.clone();
            move |params| {
                let mgr = mgr.clone();
                Box::pin(async move {
                    let file_path = params.get("file_path").and_then(|v| v.as_str()).ok_or_else(|| "Missing required parameter: file_path".to_string())?;
                    let line = params.get("line").and_then(|v| v.as_u64()).ok_or_else(|| "Missing required parameter: line".to_string())? as u32;
                    let column = params.get("column").and_then(|v| v.as_u64()).ok_or_else(|| "Missing required parameter: column".to_string())? as u32;
                    let include_declaration = params.get("include_declaration").and_then(|v| v.as_bool()).unwrap_or(true);
                    let text = params.get("text").and_then(|v| v.as_str());
                    let locations = mgr.find_references(file_path, line, column, include_declaration, text).await?;
                    Ok(serde_json::json!({ "locations": locations }))
                })
            }
        }),
    }).await;

    // get_hover tool
    server.register_tool(ToolHandler {
        name: "get_hover",
        description: "Get hover information (type signature, docs) for a symbol at a given position",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "Path to the source file" },
                "line": { "type": "integer", "description": "Line number (0-indexed)" },
                "column": { "type": "integer", "description": "Column number (0-indexed)" },
                "text": { "type": "string", "description": "Optional file content to send to the LSP server" }
            },
            "required": ["file_path", "line", "column"]
        }),
        handler: Box::new({
            let mgr = manager.clone();
            move |params| {
                let mgr = mgr.clone();
                Box::pin(async move {
                    let file_path = params.get("file_path").and_then(|v| v.as_str()).ok_or_else(|| "Missing required parameter: file_path".to_string())?;
                    let line = params.get("line").and_then(|v| v.as_u64()).ok_or_else(|| "Missing required parameter: line".to_string())? as u32;
                    let column = params.get("column").and_then(|v| v.as_u64()).ok_or_else(|| "Missing required parameter: column".to_string())? as u32;
                    let text = params.get("text").and_then(|v| v.as_str());
                    let hover = mgr.get_hover(file_path, line, column, text).await?;
                    Ok(serde_json::json!({ "hover": hover }))
                })
            }
        }),
    }).await;

    // get_completion tool
    server.register_tool(ToolHandler {
        name: "get_completion",
        description: "Get code completion items at a given position",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "Path to the source file" },
                "line": { "type": "integer", "description": "Line number (0-indexed)" },
                "column": { "type": "integer", "description": "Column number (0-indexed)" },
                "text": { "type": "string", "description": "Optional file content to send to the LSP server" }
            },
            "required": ["file_path", "line", "column"]
        }),
        handler: Box::new({
            let mgr = manager.clone();
            move |params| {
                let mgr = mgr.clone();
                Box::pin(async move {
                    let file_path = params.get("file_path").and_then(|v| v.as_str()).ok_or_else(|| "Missing required parameter: file_path".to_string())?;
                    let line = params.get("line").and_then(|v| v.as_u64()).ok_or_else(|| "Missing required parameter: line".to_string())? as u32;
                    let column = params.get("column").and_then(|v| v.as_u64()).ok_or_else(|| "Missing required parameter: column".to_string())? as u32;
                    let text = params.get("text").and_then(|v| v.as_str());
                    let items = mgr.get_completion(file_path, line, column, text).await?;
                    Ok(serde_json::json!({ "completions": items }))
                })
            }
        }),
    }).await;

    // get_diagnostics tool
    server.register_tool(ToolHandler {
        name: "get_diagnostics",
        description: "Get diagnostics (errors, warnings) for a file from the language server",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "Path to the source file" },
                "text": { "type": "string", "description": "Optional file content to send to the LSP server" }
            },
            "required": ["file_path"]
        }),
        handler: Box::new({
            let mgr = manager.clone();
            move |params| {
                let mgr = mgr.clone();
                Box::pin(async move {
                    let file_path = params.get("file_path").and_then(|v| v.as_str()).ok_or_else(|| "Missing required parameter: file_path".to_string())?;
                    let text = params.get("text").and_then(|v| v.as_str());
                    let diagnostics = mgr.get_diagnostics(file_path, text).await?;
                    Ok(serde_json::json!({ "diagnostics": diagnostics }))
                })
            }
        }),
    }).await;

    // rename_symbol tool
    server.register_tool(ToolHandler {
        name: "rename_symbol",
        description: "Rename a symbol at a given position across the entire workspace",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "Path to the source file" },
                "line": { "type": "integer", "description": "Line number (0-indexed)" },
                "column": { "type": "integer", "description": "Column number (0-indexed)" },
                "new_name": { "type": "string", "description": "New name for the symbol" },
                "text": { "type": "string", "description": "Optional file content to send to the LSP server" }
            },
            "required": ["file_path", "line", "column", "new_name"]
        }),
        handler: Box::new({
            let mgr = manager.clone();
            move |params| {
                let mgr = mgr.clone();
                Box::pin(async move {
                    let file_path = params.get("file_path").and_then(|v| v.as_str()).ok_or_else(|| "Missing required parameter: file_path".to_string())?;
                    let line = params.get("line").and_then(|v| v.as_u64()).ok_or_else(|| "Missing required parameter: line".to_string())? as u32;
                    let column = params.get("column").and_then(|v| v.as_u64()).ok_or_else(|| "Missing required parameter: column".to_string())? as u32;
                    let new_name = params.get("new_name").and_then(|v| v.as_str()).ok_or_else(|| "Missing required parameter: new_name".to_string())?;
                    let text = params.get("text").and_then(|v| v.as_str());
                    let result = mgr.rename_symbol(file_path, line, column, new_name, text).await?;
                    Ok(result)
                })
            }
        }),
    }).await;

    // apply_code_action tool
    server.register_tool(ToolHandler {
        name: "apply_code_action",
        description: "Execute a code action or workspace command (e.g. quick-fix, refactoring)",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Command identifier to execute" },
                "arguments": { "type": "array", "description": "Arguments for the command", "items": {} }
            },
            "required": ["command"]
        }),
        handler: Box::new({
            let mgr = manager.clone();
            move |params| {
                let mgr = mgr.clone();
                Box::pin(async move {
                    let command = params.get("command").and_then(|v| v.as_str()).ok_or_else(|| "Missing required parameter: command".to_string())?;
                    let arguments = params.get("arguments").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                    let result = mgr.apply_code_action(command, arguments).await?;
                    Ok(result)
                })
            }
        }),
    }).await;

    // format_code tool
    server.register_tool(ToolHandler {
        name: "format_code",
        description: "Format a document using the language server's document formatting",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "Path to the source file to format" },
                "text": { "type": "string", "description": "Optional file content to send to the LSP server" }
            },
            "required": ["file_path"]
        }),
        handler: Box::new({
            let mgr = manager.clone();
            move |params| {
                let mgr = mgr.clone();
                Box::pin(async move {
                    let file_path = params.get("file_path").and_then(|v| v.as_str()).ok_or_else(|| "Missing required parameter: file_path".to_string())?;
                    let text = params.get("text").and_then(|v| v.as_str());
                    let edits = mgr.format_code(file_path, text).await?;
                    Ok(serde_json::json!({ "edits": edits }))
                })
            }
        }),
    }).await;

    info!("LSP tools registered");
}

// ---------------------------------------------------------------------------
// Workflow tools
// ---------------------------------------------------------------------------

/// Register workflow tools (serena_info).
async fn register_workflow_tools(server: &McpServer) {
    // serena_info tool
    server.register_tool(ToolHandler {
        name: "serena_info",
        description: "Get information about the Serena MCP server, available tools, and version",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {}
        }),
        handler: Box::new(|_params| {
            Box::pin(async move {
                Ok(serde_json::json!({
                    "name": "Serena.rs",
                    "version": env!("CARGO_PKG_VERSION"),
                    "description": "MCP Toolkit for Coding Agents",
                }))
            })
        }),
    }).await;

    info!("Workflow tools registered");
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
        assert!(count >= 3, "Expected at least 3 built-in tools, got {count}");

        let tools = server.tool_list().await;
        let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
        assert!(names.contains(&"get_symbols"));
        assert!(names.contains(&"find_symbol"));
        assert!(names.contains(&"list_symbols"));
    }

    #[tokio::test]
    async fn test_register_builtin_all_tools() {
        let server = McpServer::new();
        register_builtin_tools(&server).await;

        let count = server.tool_count().await;
        assert!(
            count >= 16,
            "Expected at least 16 built-in tools, got {count}"
        );

        let tools = server.tool_list().await;
        let names: Vec<&str> = tools.iter().map(|t| t.name).collect();

        // Symbol tools
        assert!(names.contains(&"get_symbols"));
        assert!(names.contains(&"find_symbol"));
        assert!(names.contains(&"list_symbols"));

        // File tools
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"list_dir"));
        assert!(names.contains(&"find_file"));
        assert!(names.contains(&"replace_content"));

        // Search tools
        assert!(names.contains(&"search_for_pattern"));

        // Shell tools
        assert!(names.contains(&"execute_shell_command"));

        // Memory tools
        assert!(names.contains(&"write_memory"));
        assert!(names.contains(&"read_memory"));
        assert!(names.contains(&"search_memories"));
        assert!(names.contains(&"delete_memory"));

        // Config tools
        assert!(names.contains(&"get_config"));
        assert!(names.contains(&"set_config"));
        assert!(names.contains(&"list_config"));

        // LSP tools
        assert!(names.contains(&"get_definition"));
        assert!(names.contains(&"find_references"));
        assert!(names.contains(&"get_hover"));
        assert!(names.contains(&"get_completion"));
        assert!(names.contains(&"get_diagnostics"));
        assert!(names.contains(&"rename_symbol"));
        assert!(names.contains(&"apply_code_action"));
        assert!(names.contains(&"format_code"));

        // Workflow tools
        assert!(names.contains(&"serena_info"));
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
