//! Standardized MCP error handling.
//!
//! Re-exports and extends rmcp's `ErrorData` with Serena-specific error helpers.
//! All MCP errors use standard JSON-RPC error codes as defined in the MCP spec.

use std::borrow::Cow;

pub use rmcp::ErrorData as McpError;
use serde_json::Value;

/// Helper functions for constructing common MCP errors.
pub trait McpErrorExt {
    /// Create an "internal error" (code -32603).
    fn internal_error(message: impl Into<Cow<'static, str>>) -> McpError;

    /// Create an "invalid params" error (code -32602).
    fn invalid_params(message: impl Into<Cow<'static, str>>) -> McpError;

    /// Create a "method not found" error (code -32601).
    fn method_not_found(name: impl Into<Cow<'static, str>>) -> McpError;

    /// Create a "tool not found" error (code -32602 with custom data).
    fn tool_not_found(name: &str) -> McpError;

    /// Create a "parse error" (code -32700).
    fn parse_error(message: impl Into<Cow<'static, str>>) -> McpError;

    /// Create an "invalid request" error (code -32600).
    fn invalid_request(message: impl Into<Cow<'static, str>>) -> McpError;
}

impl McpErrorExt for McpError {
    fn internal_error(message: impl Into<Cow<'static, str>>) -> McpError {
        McpError::new(
            rmcp::model::ErrorCode::INTERNAL_ERROR,
            message,
            None,
        )
    }

    fn invalid_params(message: impl Into<Cow<'static, str>>) -> McpError {
        McpError::new(
            rmcp::model::ErrorCode::INVALID_PARAMS,
            message,
            None,
        )
    }

    fn method_not_found(name: impl Into<Cow<'static, str>>) -> McpError {
        McpError::new(
            rmcp::model::ErrorCode::METHOD_NOT_FOUND,
            format!("method '{}' not found", name.into()),
            None,
        )
    }

    fn tool_not_found(name: &str) -> McpError {
        McpError::new(
            rmcp::model::ErrorCode::INVALID_PARAMS,
            format!("tool not found: {name}"),
            Some(Value::String(name.to_string())),
        )
    }

    fn parse_error(message: impl Into<Cow<'static, str>>) -> McpError {
        McpError::parse_error(message, None)
    }

    fn invalid_request(message: impl Into<Cow<'static, str>>) -> McpError {
        McpError::new(
            rmcp::model::ErrorCode::INVALID_REQUEST,
            message,
            None,
        )
    }
}

/// Build a successful `CallToolResult` with text content.
pub fn text_result(text: impl Into<String>) -> rmcp::model::CallToolResult {
    rmcp::model::CallToolResult {
        content: vec![rmcp::model::Content::text(text.into())],
        structured_content: None,
        is_error: Some(false),
        meta: None,
    }
}

/// Build an error `CallToolResult` with an error message.
pub fn error_result(message: impl Into<String>) -> rmcp::model::CallToolResult {
    rmcp::model::CallToolResult {
        content: vec![rmcp::model::Content::text(message.into())],
        structured_content: None,
        is_error: Some(true),
        meta: None,
    }
}

/// Build a `CallToolResult` from JSON.
pub fn json_result(value: Value) -> rmcp::model::CallToolResult {
    rmcp::model::CallToolResult {
        content: vec![rmcp::model::Content::json(&value)],
        structured_content: Some(value),
        is_error: Some(false),
        meta: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_internal_error_code() {
        let err = McpError::internal_error("something broke");
        assert_eq!(err.code.0, -32603);
        assert_eq!(err.message, "something broke");
    }

    #[test]
    fn test_invalid_params_code() {
        let err = McpError::invalid_params("bad argument");
        assert_eq!(err.code.0, -32602);
        assert_eq!(err.message, "bad argument");
    }

    #[test]
    fn test_method_not_found() {
        let err = McpError::method_not_found("foo");
        assert_eq!(err.code.0, -32601);
        assert!(err.message.contains("foo"));
    }

    #[test]
    fn test_tool_not_found() {
        let err = McpError::tool_not_found("nonexistent");
        assert_eq!(err.code.0, -32602);
        assert!(err.message.contains("nonexistent"));
    }

    #[test]
    fn test_text_result_is_not_error() {
        let result = text_result("hello world");
        assert_eq!(result.is_error, Some(false));
        assert_eq!(result.content.len(), 1);
    }

    #[test]
    fn test_error_result_is_error() {
        let result = error_result("oops");
        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn test_json_result() {
        let val = serde_json::json!({"key": "value"});
        let result = json_result(val);
        assert_eq!(result.is_error, Some(false));
        assert!(result.structured_content.is_some());
    }
}
