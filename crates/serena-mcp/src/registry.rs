//! Tool registry — maps tool names to implementations.
//!
//! Provides a registry for MCP tools with their names, descriptions,
//! JSON Schema input schemas, and handler functions. Tools are
//! registered at startup and dispatched by the MCP server.

use serde::Serialize;
use std::collections::HashMap;

/// A registered tool with its metadata and handler.
pub struct ToolHandler {
    /// Tool name (used by MCP client to invoke)
    pub name: &'static str,
    /// Human-readable description
    pub description: &'static str,
    /// JSON Schema for input parameters
    pub input_schema: serde_json::Value,
    /// The handler function (boxed async)
    pub handler: Box<dyn Fn(serde_json::Value) -> futures::future::BoxFuture<'static, Result<serde_json::Value, String>> + Send + Sync>,
}

/// Registry of all MCP tools exposed by Serena.
///
/// # Example
///
/// ```
/// use serena_mcp::registry::ToolRegistry;
///
/// let mut registry = ToolRegistry::new();
/// assert!(registry.is_empty());
/// ```
pub struct ToolRegistry {
    tools: HashMap<&'static str, ToolHandler>,
}

impl ToolRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool handler.
    pub fn register(&mut self, handler: ToolHandler) {
        self.tools.insert(handler.name, handler);
    }

    /// Get a tool handler by name.
    pub fn get(&self, name: &str) -> Option<&ToolHandler> {
        self.tools.get(name)
    }

    /// List all registered tool names.
    pub fn list_tools(&self) -> Vec<&str> {
        self.tools.keys().copied().collect()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Get the list of tools in MCP-compatible format.
    pub fn mcp_tool_list(&self) -> Vec<McpToolDefinition> {
        self.tools.values().map(|t| McpToolDefinition {
            name: t.name,
            description: t.description,
            input_schema: t.input_schema.clone(),
        }).collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// MCP-compatible tool definition (name, description, JSON Schema).
#[derive(Debug, Clone, Serialize)]
pub struct McpToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_registry() {
        let registry = ToolRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_register_and_list() {
        let mut registry = ToolRegistry::new();
        registry.register(ToolHandler {
            name: "test_tool",
            description: "A test tool",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "input": { "type": "string" }
                }
            }),
            handler: Box::new(|params| {
                Box::pin(async move {
                    Ok(serde_json::json!({ "result": params }))
                })
            }),
        });

        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());
        assert!(registry.list_tools().contains(&"test_tool"));
    }

    #[test]
    fn test_get_handler() {
        let mut registry = ToolRegistry::new();
        registry.register(ToolHandler {
            name: "get_symbols",
            description: "Get symbols from a file",
            input_schema: serde_json::json!({}),
            handler: Box::new(|_| {
                Box::pin(async move { Ok(serde_json::json!({})) })
            }),
        });

        let handler = registry.get("get_symbols");
        assert!(handler.is_some());
        assert_eq!(handler.unwrap().name, "get_symbols");
        assert_eq!(handler.unwrap().description, "Get symbols from a file");

        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_mcp_tool_list_format() {
        let mut registry = ToolRegistry::new();
        registry.register(ToolHandler {
            name: "tool_a",
            description: "Tool A",
            input_schema: serde_json::json!({"type": "object"}),
            handler: Box::new(|_| {
                Box::pin(async move { Ok(serde_json::json!({})) })
            }),
        });
        registry.register(ToolHandler {
            name: "tool_b",
            description: "Tool B",
            input_schema: serde_json::json!({"type": "object"}),
            handler: Box::new(|_| {
                Box::pin(async move { Ok(serde_json::json!({})) })
            }),
        });

        let defs = registry.mcp_tool_list();
        assert_eq!(defs.len(), 2);

        let names: Vec<&str> = defs.iter().map(|d| d.name).collect();
        assert!(names.contains(&"tool_a"));
        assert!(names.contains(&"tool_b"));
    }

    #[test]
    fn test_handler_execution() {
        let mut registry = ToolRegistry::new();
        registry.register(ToolHandler {
            name: "echo",
            description: "Echo input",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string" }
                }
            }),
            handler: Box::new(|params| {
                Box::pin(async move { Ok(params) })
            }),
        });

        let handler = registry.get("echo").unwrap();
        let input = serde_json::json!({ "message": "hello" });
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on((handler.handler)(input.clone()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), input);
    }

    #[test]
    fn test_register_overwrite() {
        let mut registry = ToolRegistry::new();
        registry.register(ToolHandler {
            name: "tool",
            description: "Original",
            input_schema: serde_json::json!({}),
            handler: Box::new(|_| Box::pin(async move { Ok(serde_json::json!({"v": 1})) })),
        });
        registry.register(ToolHandler {
            name: "tool",
            description: "Overwritten",
            input_schema: serde_json::json!({}),
            handler: Box::new(|_| Box::pin(async move { Ok(serde_json::json!({"v": 2})) })),
        });

        assert_eq!(registry.len(), 1);
        assert_eq!(registry.get("tool").unwrap().description, "Overwritten");
    }
}
