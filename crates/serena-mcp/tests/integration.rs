//! Integration tests for the MCP server.
//!
//! These tests spin up a real MCP server on an in-memory duplex stream,
//! connect as a standard MCP client, and verify the full protocol lifecycle.
//! The server exposes a few test tools so we can test tools/list, tools/call,
//! error handling, and shutdown.

use rmcp::{
    model::*,
    service::{RunningService, RoleClient},
    ServiceExt,
};
use serena_mcp::registry::ToolHandler;
use serena_mcp::server::McpServer;
use std::sync::Arc;
use tokio::sync::Mutex;

// ============================================================================
// Helpers
// ============================================================================

/// Spin up a test MCP server over an in-memory duplex stream and connect
/// as a client. The server has two tools: `echo` and `greet`.
async fn setup_client() -> RunningService<RoleClient, ()> {
    let server = McpServer::new();

    // Register an echo tool (returns its input)
    server
        .register_tool(ToolHandler {
            name: "echo",
            description: "Echo input back",
            input_schema: serde_json::json!({"type": "object"}),
            handler: Box::new(|params| Box::pin(async move { Ok(params) })),
        })
        .await;

    // Register a greet tool
    server
        .register_tool(ToolHandler {
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
        })
        .await;

    // Create a bidirectional byte stream with a large buffer
    let (server_io, client_io) = tokio::io::duplex(1024 * 1024);

    // Spawn the MCP server on one end
    let server_handle = tokio::spawn(async move {
        if let Err(e) = rmcp::serve_server(server, server_io).await {
            eprintln!("MCP server stopped with error: {e}");
        }
    });

    // Connect as an MCP client on the other end
    let client = ().serve(client_io).await.unwrap();

    // Small delay for initialization to propagate
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    client
}

// ============================================================================
// Test 1 — Initialize + tools/list
// ============================================================================

#[tokio::test]
async fn test_initialize_and_list_tools() {
    let client = setup_client().await;

    // List all available tools
    let tools = client.list_all_tools().await.unwrap();

    // Should have our two registered tools
    assert_eq!(tools.len(), 2, "expected 2 tools, got {}", tools.len());

    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    assert!(names.contains(&"echo"), "echo tool should be listed");
    assert!(names.contains(&"greet"), "greet tool should be listed");
}

// ============================================================================
// Test 2 — tools/list schema conformance
// ============================================================================

#[tokio::test]
async fn test_tool_list_items_have_required_fields() {
    let client = setup_client().await;

    let tools = client.list_all_tools().await.unwrap();

    for tool in &tools {
        // Every tool must have a name
        assert!(!tool.name.is_empty(), "tool name must not be empty");

        // Every tool must have an input_schema
        assert!(
            !tool.input_schema.is_empty(),
            "tool '{}' must have an input_schema",
            tool.name
        );
    }
}

// ============================================================================
// Test 3 — Initialize + tools/call (successful)
// ============================================================================

#[tokio::test]
async fn test_call_tool_success() {
    let client = setup_client().await;

    let result = client
        .call_tool(CallToolRequestParam {
            name: "greet".into(),
            arguments: Some(
                serde_json::json!({ "name": "Serena" })
                    .as_object()
                    .cloned()
                    .unwrap(),
            ),
        })
        .await
        .unwrap();

    // Must NOT be an error result
    assert_eq!(result.is_error, Some(false), "call should succeed");

    // Must have content
    assert!(!result.content.is_empty(), "result must have content");

    // Greet tool returns text content with the greeting
    let text_content = result.content.first().unwrap();
    match &text_content.raw {
        rmcp::model::RawContent::Text(text) => {
            assert!(
                text.text.contains("Hello, Serena!"),
                "greeting should contain Hello, Serena! — got: {}",
                text.text
            );
        }
        other => {
            panic!("expected Text content, got {:?}", other);
        }
    }
}

// ============================================================================
// Test 4 — tools/call with missing required params (error)
// ============================================================================

#[tokio::test]
async fn test_call_tool_missing_params() {
    let client = setup_client().await;

    let result = client
        .call_tool(CallToolRequestParam {
            name: "greet".into(),
            arguments: Some(
                serde_json::json!({})
                    .as_object()
                    .cloned()
                    .unwrap(),
            ),
        })
        .await
        .unwrap();

    // Missing 'name' param — tool returns an error CallToolResult
    assert_eq!(
        result.is_error,
        Some(true),
        "missing params should produce an error result"
    );
}

// ============================================================================
// Test 5 — Call nonexistent tool (error)
// ============================================================================

#[tokio::test]
async fn test_call_nonexistent_tool() {
    let client = setup_client().await;

    let result = client
        .call_tool(CallToolRequestParam {
            name: "nonexistent".into(),
            arguments: None,
        })
        .await;

    // The server should return an MCP error for unknown tools
    assert!(
        result.is_err(),
        "calling nonexistent tool should return an error, got {:?}",
        result
    );
}

// ============================================================================
// Test 6 — Echo tool with JSON arguments
// ============================================================================

#[tokio::test]
async fn test_call_echo_with_json() {
    let client = setup_client().await;

    let input_args = serde_json::json!({ "message": "hello", "count": 42 });
    let result = client
        .call_tool(CallToolRequestParam {
            name: "echo".into(),
            arguments: Some(input_args.as_object().cloned().unwrap()),
        })
        .await
        .unwrap();

    assert_eq!(result.is_error, Some(false), "echo should succeed");
    assert!(!result.content.is_empty(), "echo result must have content");
}

// ============================================================================
// Test 7 — Clean shutdown via notification
// ============================================================================

#[tokio::test]
async fn test_graceful_shutdown() {
    let client = setup_client().await;

    // Send initialized notification (already sent during handshake by serve)
    // Try listing tools as a final check, then cancel
    let peer_info = client.peer_info();
    assert!(
        peer_info.is_some(),
        "peer_info should be available after init"
    );

    let server_info = peer_info.unwrap();
    assert_eq!(server_info.server_info.name, "serena-rs");

    // Cancel the client connection gracefully
    let result = client.cancel().await;
    assert!(result.is_ok(), "graceful cancel should succeed");
}

// ============================================================================
// Test 8 — Initialize response schema conformance
// ============================================================================

#[tokio::test]
async fn test_initialize_response_conforms_to_mcp_schema() {
    let client = setup_client().await;

    let peer_info = client.peer_info().unwrap();

    // Check required MCP fields
    assert_eq!(
        peer_info.protocol_version,
        ProtocolVersion::default(),
        "must advertise supported protocol version"
    );

    // Must advertise tools capability
    assert!(
        peer_info.capabilities.tools.is_some(),
        "must advertise tools capability"
    );

    // Server info must have name and version
    assert_eq!(peer_info.server_info.name, "serena-rs");
    assert!(!peer_info.server_info.version.is_empty(), "version not empty");
}
