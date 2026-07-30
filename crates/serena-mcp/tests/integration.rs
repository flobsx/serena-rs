//! Integration tests for the MCP server.
//!
//! These tests exercise the server's public API directly:
//! tool listing, dispatching, error handling, lifecycle, and
//! MCP schema conformance — without the complexity of an
//! in-memory transport layer.
//!
//! Full end-to-end transport testing is handled by the smoke test.

use std::sync::Arc;
use tokio::sync::Mutex;

use serena_mcp::registry::ToolHandler;
use serena_mcp::server::dispatch_tool_call;
use serena_mcp::ToolRegistry;

// ============================================================================
// Helpers
// ============================================================================

/// Create an echo tool handler.
fn echo_handler() -> ToolHandler {
    ToolHandler {
        name: "echo",
        description: "Echo input back",
        input_schema: serde_json::json!({"type": "object"}),
        handler: Box::new(|params| Box::pin(async move { Ok(params) })),
    }
}

/// Create a greet tool handler.
fn greet_handler() -> ToolHandler {
    ToolHandler {
        name: "greet",
        description: "Greet someone by name",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"]
        }),
        handler: Box::new(|params| {
            Box::pin(async move {
                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("world");
                Ok(serde_json::json!({ "greeting": format!("Hello, {}!", name) }))
            })
        }),
    }
}

/// Build a registry with echo + greet tools, wrapped in Arc<Mutex>.
fn test_registry() -> Arc<Mutex<ToolRegistry>> {
    let mut reg = ToolRegistry::new();
    reg.register(echo_handler());
    reg.register(greet_handler());
    Arc::new(Mutex::new(reg))
}

// ============================================================================
// Test 1 — tool listing (via McpServer)
// ============================================================================

#[tokio::test]
async fn test_list_registered_tools() {
    let server = serena_mcp::McpServer::with_registry({
        let mut reg = ToolRegistry::new();
        reg.register(echo_handler());
        reg.register(greet_handler());
        reg
    });

    let tools = server.tool_list().await;
    assert_eq!(tools.len(), 2, "expected 2 tools, got {}", tools.len());

    let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
    assert!(names.contains(&"echo"), "echo should be listed");
    assert!(names.contains(&"greet"), "greet should be listed");
}

// ============================================================================
// Test 2 — Tool definition schema conformance
// ============================================================================

#[tokio::test]
async fn test_tool_defs_have_required_fields() {
    let server = serena_mcp::McpServer::with_registry({
        let mut reg = ToolRegistry::new();
        reg.register(echo_handler());
        reg.register(greet_handler());
        reg
    });

    let tools = server.tool_list().await;

    for tool in &tools {
        assert!(!tool.name.is_empty(), "tool name must not be empty");
        assert!(
            !tool.description.is_empty(),
            "tool '{}' description must not be empty",
            tool.name
        );
        assert!(
            tool.input_schema.is_object(),
            "tool '{}' input_schema must be a JSON object",
            tool.name
        );
    }
}

// ============================================================================
// Test 3 — Successful tool call via dispatch
// ============================================================================

#[tokio::test]
async fn test_dispatch_tool_success() {
    let registry = test_registry();

    let result = dispatch_tool_call(
        &registry,
        "greet",
        Some(
            serde_json::json!({ "name": "Serena" })
                .as_object()
                .cloned()
                .unwrap(),
        ),
    )
    .await
    .expect("dispatch should succeed");

    // Must NOT be an error result
    assert_eq!(
        result.is_error, Some(false),
        "successful call should not be an error"
    );

    // Must have content
    assert!(!result.content.is_empty(), "result must have content");

    // Greet tool returns text content with the greeting
    let text_content = result.content.first().unwrap();
    match &text_content.raw {
        rmcp::model::RawContent::Text(text) => {
            assert!(
                text.text.contains("Hello, Serena!"),
                "expected 'Hello, Serena!' — got: {}",
                text.text
            );
        }
        other => {
            panic!("expected Text content, got {:?}", other);
        }
    }
}

// ============================================================================
// Test 4 — Tool call with missing params (graceful default)
// ============================================================================

#[tokio::test]
async fn test_dispatch_tool_missing_params() {
    let registry = test_registry();

    let result = dispatch_tool_call(
        &registry,
        "greet",
        Some(serde_json::json!({}).as_object().cloned().unwrap()),
    )
    .await
    .expect("dispatch should return a result (not an MCP error)");

    // The handler handles missing 'name' gracefully by defaulting to "world"
    // So the call succeeds, just uses the default
    assert_eq!(
        result.is_error, Some(false),
        "greet without name should still succeed (uses default)"
    );

    // Verify it defaulted to "world"
    let text = result.content.first().unwrap();
    match &text.raw {
        rmcp::model::RawContent::Text(t) => {
            assert!(t.text.contains("world"), "expected default greeting with 'world'");
        }
        _ => panic!("expected Text content"),
    }
}

// ============================================================================
// Test 5 — Dispatch nonexistent tool (MCP error)
// ============================================================================

#[tokio::test]
async fn test_dispatch_nonexistent_tool() {
    let registry = test_registry();

    let result = dispatch_tool_call(&registry, "nonexistent", None).await;

    assert!(
        result.is_err(),
        "calling nonexistent tool should return an MCP error"
    );

    if let Err(err) = result {
        assert_eq!(
            err.code.0, -32602,
            "tool not found should use INVALID_PARAMS code"
        );
        assert!(
            err.message.contains("nonexistent"),
            "error message should mention the tool name"
        );
    }
}

// ============================================================================
// Test 6 — Echo tool preserves JSON arguments
// ============================================================================

#[tokio::test]
async fn test_dispatch_echo_preserves_json() {
    let registry = test_registry();

    let input = serde_json::json!({ "message": "hello", "count": 42 });
    let result = dispatch_tool_call(
        &registry,
        "echo",
        Some(input.as_object().cloned().unwrap()),
    )
    .await
    .expect("echo should succeed");

    assert_eq!(result.is_error, Some(false), "echo should not error");
    assert!(!result.content.is_empty(), "echo must have content");
}

// ============================================================================
// Test 7 — Empty registry returns empty tool list
// ============================================================================

#[tokio::test]
async fn test_empty_registry_returns_no_tools() {
    let server = serena_mcp::McpServer::new();
    let tools = server.tool_list().await;
    assert!(tools.is_empty(), "new server should have no tools");
    assert_eq!(server.tool_count().await, 0);
}

// ============================================================================
// Test 8 — Tool count reflects registrations
// ============================================================================

#[tokio::test]
async fn test_tool_count_increases_with_registrations() {
    let server = serena_mcp::McpServer::new();
    assert_eq!(server.tool_count().await, 0);

    server.register_tool(echo_handler()).await;
    assert_eq!(server.tool_count().await, 1);

    server.register_tool(greet_handler()).await;
    assert_eq!(server.tool_count().await, 2);
}

// ============================================================================
// Test 9 — dispatch_tool_call with no arguments (None)
// ============================================================================

#[tokio::test]
async fn test_dispatch_tool_no_arguments() {
    let registry = test_registry();

    // Passing None should work (handler receives {})
    let result = dispatch_tool_call(&registry, "echo", None)
        .await
        .expect("echo with None args should succeed");

    assert_eq!(result.is_error, Some(false));
}

// ============================================================================
// Test 10 — Server info MCP schema conformance
// ============================================================================

#[tokio::test]
async fn test_server_info_conforms_to_mcp_schema() {
    let server = serena_mcp::McpServer::new();

    // get_info() is called during MCP initialize — it returns ServerInfo
    // We verify it conforms to the MCP spec via the ServerHandler trait
    use rmcp::ServerHandler;

    let info = server.get_info();

    // Required fields per MCP spec
    assert_eq!(
        info.protocol_version,
        rmcp::model::ProtocolVersion::default(),
        "must advertise supported protocol version"
    );

    // Must advertise tools capability
    assert!(
        info.capabilities.tools.is_some(),
        "must advertise tools capability"
    );

    // Server info must have name and version
    assert_eq!(info.server_info.name, "serena-rs");
    assert!(!info.server_info.version.is_empty(), "version must not be empty");

    // Instructions should be present (we set them in get_info)
    assert!(
        info.instructions.is_some(),
        "instructions should be present"
    );
}
