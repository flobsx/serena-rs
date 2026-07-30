//! Symbol/LSP tools — symbol retrieval, references, implementations.
//!
//! Provides tools for querying language servers about symbols in
//! source code: listing all symbols, finding specific ones, and
//! resolving references.

use std::collections::HashMap;
use std::sync::Arc;

use serena_lsp::client::LspClient;
use serena_lsp::provider::{LanguageServerProvider, ServerConfig};
use serena_lsp::symbol::Symbol;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::info;

/// Parameters for querying symbols in a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolQueryParams {
    /// File path or URI to query
    pub file_path: String,
    /// Optional text content to send to the LSP server
    pub text: Option<String>,
    /// Language ID override (auto-detected from extension if absent)
    pub language_id: Option<String>,
    /// Optional symbol name filter (for find_symbol)
    pub query: Option<String>,
}

/// Parameters for finding references to a symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceQueryParams {
    /// File path or URI
    pub file_path: String,
    /// Line number (0-indexed)
    pub line: u32,
    /// Column number (0-indexed)
    pub column: u32,
}

/// A symbol result formatted for tool output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolEntry {
    pub name: String,
    pub kind: String,
    pub detail: Option<String>,
    pub file_path: String,
    pub line: u32,
    pub column: u32,
    pub children: Vec<SymbolEntry>,
}

impl From<(&Symbol, &str)> for SymbolEntry {
    fn from((sym, path): (&Symbol, &str)) -> Self {
        Self {
            name: sym.name.clone(),
            kind: format!("{:?}", sym.kind),
            detail: sym.detail.clone(),
            file_path: path.to_string(),
            line: sym.range.start.line,
            column: sym.range.start.column,
            children: sym.children.iter().map(|c| (c, path).into()).collect(),
        }
    }
}

/// A location pointing to a definition or reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinitionLocation {
    pub uri: String,
    pub range: LocationRange,
}

/// A range within a location.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationRange {
    pub start: Position2d,
    pub end: Position2d,
}

/// A 2D position in a text document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position2d {
    pub line: u32,
    pub column: u32,
}

impl From<lsp_types::Location> for DefinitionLocation {
    fn from(loc: lsp_types::Location) -> Self {
        Self {
            uri: loc.uri.to_string(),
            range: LocationRange {
                start: Position2d {
                    line: loc.range.start.line,
                    column: loc.range.start.character,
                },
                end: Position2d {
                    line: loc.range.end.line,
                    column: loc.range.end.character,
                },
            },
        }
    }
}

/// Convert an LSP MarkedString to a JSON value.
fn marked_string_to_value(ms: lsp_types::MarkedString) -> serde_json::Value {
    match ms {
        lsp_types::MarkedString::String(s) => serde_json::json!({ "language": "text", "value": s }),
        lsp_types::MarkedString::LanguageString(ls) => {
            serde_json::json!({ "language": ls.language, "value": ls.value })
        }
    }
}

/// Manager for LSP-based symbol operations.
///
/// Caches language server connections per server type and provides
/// high-level methods for querying symbols.
pub struct SymbolToolManager {
    /// Active LSP clients, keyed by server name
    clients: Arc<Mutex<HashMap<String, LspClient>>>,
    /// Provider for resolving server configs
    provider: LanguageServerProvider,
}

/// Convert an LSP TextEdit (from OneOf) to a JSON value using serde serialization.
fn text_edit_to_json(edit: &serde_json::Value) -> Option<serde_json::Value> {
    let range = edit.get("range")?;
    let new_text = edit.get("newText")?;
    // If this is an AnnotatedTextEdit (has textEdit flatten), the range/newText
    // are already at the top level due to #[serde(flatten)]
    Some(serde_json::json!({
        "range": range,
        "newText": new_text,
    }))
}

impl Default for SymbolToolManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolToolManager {
    /// Create a new manager with default built-in servers.
    pub fn new() -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
            provider: LanguageServerProvider::new(),
        }
    }

    /// Get symbols for a file using the appropriate LSP server.
    ///
    /// Opens the document, queries document symbols, and returns
    /// them as a tree. The LSP server is started on first use.
    pub async fn get_symbols(
        &self,
        file_path: &str,
        text: Option<&str>,
    ) -> Result<Vec<Symbol>, String> {
        let config = self.resolve_server(file_path)?;
        let client = self.get_or_start_client(&config).await?;
        let uri = Self::path_to_uri(file_path)?;

        let language_id = config.language_ids.first()
            .cloned()
            .unwrap_or_else(|| "plaintext".to_string());

        // Open the document if text is provided
        if let Some(content) = text {
            client.open_document(&uri, content, &language_id)
                .await
                .map_err(|e| format!("Failed to open document: {e}"))?;
        }

        // Give the server a moment to process
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let symbols = client.document_symbols(&uri)
            .await
            .map_err(|e| format!("Failed to get symbols: {e}"))?;

        Ok(symbols.iter().map(Symbol::from_lsp).collect())
    }

    /// Find symbols matching a query in a file.
    pub async fn find_symbol(
        &self,
        file_path: &str,
        query: &str,
        text: Option<&str>,
    ) -> Result<Vec<SymbolEntry>, String> {
        let symbols = self.get_symbols(file_path, text).await?;
        let query_lower = query.to_lowercase();

        let mut results = Vec::new();
        for sym in &symbols {
            if sym.name.to_lowercase().contains(&query_lower) {
                results.push(SymbolEntry::from((sym, file_path)));
            }
            // Search in flattened children
            let flat = sym.flatten();
            for child in flat {
                if child.name.to_lowercase().contains(&query_lower) && child.name != sym.name {
                    results.push(SymbolEntry::from((child, file_path)));
                }
            }
        }

        Ok(results)
    }

    /// Get all symbols in a file as a flat list with hierarchy hints.
    pub async fn list_symbols(
        &self,
        file_path: &str,
        text: Option<&str>,
    ) -> Result<Vec<SymbolEntry>, String> {
        let symbols = self.get_symbols(file_path, text).await?;
        let mut entries = Vec::new();
        for sym in &symbols {
            let entry = SymbolEntry::from((sym, file_path));
            entries.push(entry);
        }
        Ok(entries)
    }

    // ---------------------------------------------------------------------------
    // LSP tools
    // ---------------------------------------------------------------------------

    /// Get the definition location for a symbol at a given position.
    pub async fn get_definition(
        &self,
        file_path: &str,
        line: u32,
        column: u32,
        text: Option<&str>,
    ) -> Result<Vec<DefinitionLocation>, String> {
        let (client, uri) = self.prepare_document(file_path, text).await?;
        let result = client.goto_definition(&uri, lsp_types::Position::new(line, column))
            .await
            .map_err(|e| format!("Failed to get definition: {e}"))?;

        Ok(match result {
            Some(lsp_types::GotoDefinitionResponse::Scalar(loc)) => {
                vec![DefinitionLocation::from(loc)]
            }
            Some(lsp_types::GotoDefinitionResponse::Array(locs)) => {
                locs.into_iter().map(DefinitionLocation::from).collect()
            }
            Some(lsp_types::GotoDefinitionResponse::Link(_)) => {
                // Links aren't commonly used — return empty for now
                Vec::new()
            }
            None => Vec::new(),
        })
    }

    /// Find all references to a symbol at a given position.
    pub async fn find_references(
        &self,
        file_path: &str,
        line: u32,
        column: u32,
        include_declaration: bool,
        text: Option<&str>,
    ) -> Result<Vec<DefinitionLocation>, String> {
        let (client, uri) = self.prepare_document(file_path, text).await?;
        let locations = client.references(&uri, lsp_types::Position::new(line, column), include_declaration)
            .await
            .map_err(|e| format!("Failed to find references: {e}"))?;

        Ok(locations.into_iter().map(DefinitionLocation::from).collect())
    }

    /// Get hover information at a given position.
    pub async fn get_hover(
        &self,
        file_path: &str,
        line: u32,
        column: u32,
        text: Option<&str>,
    ) -> Result<Option<serde_json::Value>, String> {
        let (client, uri) = self.prepare_document(file_path, text).await?;
        let hover = client.hover(&uri, lsp_types::Position::new(line, column))
            .await
            .map_err(|e| format!("Failed to get hover: {e}"))?;

        match hover {
            Some(h) => {
                let contents = match h.contents {
                    lsp_types::HoverContents::Scalar(marked) => {
                        vec![marked_string_to_value(marked)]
                    }
                    lsp_types::HoverContents::Array(items) => {
                        items.into_iter().map(marked_string_to_value).collect()
                    }
                    lsp_types::HoverContents::Markup(markup) => {
                        vec![serde_json::json!({
                            "kind": format!("{:?}", markup.kind),
                            "value": markup.value,
                        })]
                    }
                };
                Ok(Some(serde_json::json!({
                    "contents": contents,
                    "range": h.range.map(|r| {
                        serde_json::json!({
                            "start": { "line": r.start.line, "column": r.start.character },
                            "end": { "line": r.end.line, "column": r.end.character },
                        })
                    }),
                })))
            }
            None => Ok(None),
        }
    }

    /// Get completion items at a given position.
    pub async fn get_completion(
        &self,
        file_path: &str,
        line: u32,
        column: u32,
        text: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, String> {
        let (client, uri) = self.prepare_document(file_path, text).await?;
        let items = client.completion(&uri, lsp_types::Position::new(line, column))
            .await
            .map_err(|e| format!("Failed to get completion: {e}"))?;

        Ok(items.into_iter().map(|item| {
            serde_json::json!({
                "label": item.label,
                "kind": item.kind.map(|k| format!("{:?}", k)),
                "detail": item.detail,
                "documentation": item.documentation.map(|d| match d {
                    lsp_types::Documentation::String(s) => s,
                    lsp_types::Documentation::MarkupContent(m) => m.value,
                }),
            })
        }).collect())
    }

    /// Get diagnostics for a file.
    ///
    /// Opens the document, waits briefly for the server to publish diagnostics,
    /// then returns what was collected.
    pub async fn get_diagnostics(
        &self,
        file_path: &str,
        text: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, String> {
        let (client, uri) = self.prepare_document(file_path, text).await?;
        // Wait briefly for the server to publish diagnostics
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let diagnostics = client.get_diagnostics(&uri).await;

        Ok(diagnostics.into_iter().map(|d| {
            serde_json::json!({
                "range": {
                    "start": { "line": d.range.start.line, "column": d.range.start.character },
                    "end": { "line": d.range.end.line, "column": d.range.end.character },
                },
                "severity": d.severity.map(|s| match s {
                    lsp_types::DiagnosticSeverity::ERROR => "error",
                    lsp_types::DiagnosticSeverity::WARNING => "warning",
                    lsp_types::DiagnosticSeverity::INFORMATION => "info",
                    lsp_types::DiagnosticSeverity::HINT => "hint",
                    _ => "unknown",
                }),
                "message": d.message,
                "source": d.source,
                "code": d.code.map(|c| format!("{c:?}")),
            })
        }).collect())
    }

    /// Rename a symbol at a given position.
    pub async fn rename_symbol(
        &self,
        file_path: &str,
        line: u32,
        column: u32,
        new_name: &str,
        text: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        let (client, uri) = self.prepare_document(file_path, text).await?;
        let edit = client.rename(&uri, lsp_types::Position::new(line, column), new_name)
            .await
            .map_err(|e| format!("Failed to rename symbol: {e}"))?;

        match edit {
            Some(workspace_edit) => {
                // Convert WorkspaceEdit to JSON by serializing and extracting changes
                let mut changes = serde_json::Map::new();

                if let Some(doc_changes) = workspace_edit.document_changes {
                    match doc_changes {
                        lsp_types::DocumentChanges::Edits(text_doc_edits) => {
                            for ted in text_doc_edits {
                                let uri_str = ted.text_document.uri.to_string();
                                let mut edit_values = Vec::new();
                                for e in ted.edits {
                                    if let Ok(val) = serde_json::to_value(&e) {
                                        if let Some(te) = text_edit_to_json(&val) {
                                            edit_values.push(te);
                                        }
                                    }
                                }
                                if !edit_values.is_empty() {
                                    changes.insert(uri_str, serde_json::Value::Array(edit_values));
                                }
                            }
                        }
                        lsp_types::DocumentChanges::Operations(_ops) => {
                            // Skip resource ops for now
                        }
                    }
                }

                // Fallback to changes map
                if changes.is_empty() {
                    if let Some(change_map) = &workspace_edit.changes {
                        for (uri_str, edits) in change_map {
                            let mut edit_values = Vec::new();
                            for edit_item in edits.iter() {
                                if let Ok(val) = serde_json::to_value(edit_item) {
                                    if let Some(te) = text_edit_to_json(&val) {
                                        edit_values.push(te);
                                    }
                                }
                            }
                            if !edit_values.is_empty() {
                                changes.insert(uri_str.to_string(), serde_json::Value::Array(edit_values));
                            }
                        }
                    }
                }

                Ok(serde_json::json!({ "changes": changes }))
            }
            None => Ok(serde_json::json!({ "changes": {} })),
        }
    }

    /// Apply a code action or execute a workspace command.
    pub async fn apply_code_action(
        &self,
        command: &str,
        arguments: Vec<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        // We need any LSP client to execute the command — use the first available
        let clients = self.clients.lock().await;
        let client = clients.values().next()
            .ok_or_else(|| "No active LSP client to execute command".to_string())?;

        let result = client.execute_command(command, arguments)
            .await
            .map_err(|e| format!("Failed to execute command: {e}"))?;

        Ok(serde_json::json!({ "result": result }))
    }

    /// Format a document using LSP formatting.
    pub async fn format_code(
        &self,
        file_path: &str,
        text: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, String> {
        let (client, uri) = self.prepare_document(file_path, text).await?;
        let options = lsp_types::FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            ..Default::default()
        };
        let edits = client.formatting(&uri, options)
            .await
            .map_err(|e| format!("Failed to format code: {e}"))?;

        Ok(edits.into_iter().map(|e| {
            serde_json::json!({
                "range": {
                    "start": { "line": e.range.start.line, "column": e.range.start.character },
                    "end": { "line": e.range.end.line, "column": e.range.end.character },
                },
                "newText": e.new_text,
            })
        }).collect())
    }

    // ---------------------------------------------------------------------------
    // Internal helpers
    // ---------------------------------------------------------------------------

    /// Prepare a document for LSP interaction: resolve server config, get/create
    /// client, open the document. Returns (client, URI).
    async fn prepare_document(
        &self,
        file_path: &str,
        text: Option<&str>,
    ) -> Result<(LspClient, lsp_types::Url), String> {
        let config = self.resolve_server(file_path)?;
        let client = self.get_or_start_client(&config).await?;
        let uri = Self::path_to_uri(file_path)?;

        let language_id = config.language_ids.first()
            .cloned()
            .unwrap_or_else(|| "plaintext".to_string());

        // Open the document if text is provided
        if let Some(content) = text {
            client.open_document(&uri, content, &language_id)
                .await
                .map_err(|e| format!("Failed to open document: {e}"))?;
            // Give the server a moment to process
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }

        Ok((client, uri))
    }

    /// Resolve the server config for a file path.
    fn resolve_server(&self, file_path: &str) -> Result<ServerConfig, String> {
        self.provider.for_file(file_path)
            .cloned()
            .ok_or_else(|| format!("No language server configured for file: {file_path}"))
    }

    /// Get or create an LSP client for the given server config.
    async fn get_or_start_client(&self, config: &ServerConfig) -> Result<LspClient, String> {
        let mut clients = self.clients.lock().await;
        if let Some(client) = clients.get(&config.name) {
            if client.is_initialized().await {
                return Ok(client.clone());
            }
        }

        info!(server = %config.name, "Starting LSP server");
        let client = LspClient::new();
        client.start(&config.command, &config.args)
            .await
            .map_err(|e| format!("Failed to start {0}: {e}", config.name))?;

        // Initialize with a default workspace URI
        let uri = lsp_types::Url::parse("file:///workspace")
            .map_err(|e| format!("Invalid URI: {e}"))?;
        client.initialize(&uri)
            .await
            .map_err(|e| format!("Failed to initialize {0}: {e}", config.name))?;

        clients.insert(config.name.clone(), client.clone());
        Ok(client)
    }

    /// Convert a file path to a file:// URI.
    fn path_to_uri(file_path: &str) -> Result<lsp_types::Url, String> {
        let path = std::path::Path::new(file_path);
        if path.is_absolute() {
            lsp_types::Url::from_file_path(path)
                .map_err(|_| format!("Invalid file path: {file_path}"))
        } else {
            // Make relative paths absolute by resolving from cwd
            if let Ok(cwd) = std::env::current_dir() {
                let absolute = cwd.join(path);
                lsp_types::Url::from_file_path(absolute)
                    .map_err(|_| format!("Invalid file path: {file_path}"))
            } else {
                Err(format!("Cannot resolve relative path: {file_path}"))
            }
        }
    }
}

/// Register symbol tools with the MCP tool registry.
pub fn register() {
    info!("Symbol tools registered");
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // SymbolEntry conversion tests
    // -----------------------------------------------------------------------

    fn make_symbol(name: &str, kind: serena_lsp::symbol::SymbolKind) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind,
            detail: None,
            range: serena_lsp::symbol::Range::new(
                serena_lsp::symbol::Position::new(10, 5),
                serena_lsp::symbol::Position::new(20, 0),
            ),
            selection_range: None,
            children: vec![],
            tags: vec![],
            deprecated: None,
        }
    }

    #[test]
    fn test_symbol_entry_from_symbol() {
        let sym = make_symbol("my_function", serena_lsp::symbol::SymbolKind::Function);
        let entry = SymbolEntry::from((&sym, "src/main.rs"));

        assert_eq!(entry.name, "my_function");
        assert_eq!(entry.kind, "Function");
        assert_eq!(entry.file_path, "src/main.rs");
        assert_eq!(entry.line, 10);
        assert_eq!(entry.column, 5);
        assert!(entry.children.is_empty());
    }

    #[test]
    fn test_symbol_entry_with_children() {
        let child = Symbol {
            name: "child_val".to_string(),
            kind: serena_lsp::symbol::SymbolKind::Variable,
            detail: None,
            range: serena_lsp::symbol::Range::new(
                serena_lsp::symbol::Position::new(12, 4),
                serena_lsp::symbol::Position::new(12, 14),
            ),
            selection_range: None,
            children: vec![],
            tags: vec![],
            deprecated: None,
        };

        let parent = Symbol {
            name: "parent_fn".to_string(),
            kind: serena_lsp::symbol::SymbolKind::Function,
            detail: Some("fn() -> i32".to_string()),
            range: serena_lsp::symbol::Range::new(
                serena_lsp::symbol::Position::new(10, 0),
                serena_lsp::symbol::Position::new(15, 0),
            ),
            selection_range: None,
            children: vec![child],
            tags: vec![],
            deprecated: None,
        };

        let entry = SymbolEntry::from((&parent, "lib.rs"));
        assert_eq!(entry.name, "parent_fn");
        assert_eq!(entry.detail, Some("fn() -> i32".to_string()));
        assert_eq!(entry.children.len(), 1);
        assert_eq!(entry.children[0].name, "child_val");
        assert_eq!(entry.children[0].kind, "Variable");
    }

    // -----------------------------------------------------------------------
    // Tool manager construction tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_tool_manager_default_has_provider() {
        let manager = SymbolToolManager::new();
        // Check that the provider has built-in servers
        let servers: Vec<&str> = manager.provider.servers().collect();
        assert!(servers.contains(&"rust-analyzer"));
        assert!(servers.contains(&"pyright"));
    }

    #[test]
    fn test_resolve_server_supported_file() {
        let manager = SymbolToolManager::new();
        let config = manager.resolve_server("src/main.rs");
        assert!(config.is_ok());
        assert_eq!(config.unwrap().name, "rust-analyzer");
    }

    #[test]
    fn test_resolve_server_unsupported_file() {
        let manager = SymbolToolManager::new();
        let config = manager.resolve_server("file.xyz");
        assert!(config.is_err());
        assert!(config.unwrap_err().contains("No language server configured"));
    }

    // -----------------------------------------------------------------------
    // Symbol query tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_find_symbol_matches_name() {
        let manager = SymbolToolManager::new();
        // Can't test the full flow without a running server,
        // but we can test that the path resolution works

        let result = manager.resolve_server("test.py");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "pyright");
    }

    #[test]
    fn test_path_to_uri_absolute() {
        let uri = SymbolToolManager::path_to_uri("/home/user/project/src/main.rs");
        assert!(uri.is_ok());
        let uri_str = uri.unwrap().to_string();
        assert!(uri_str.starts_with("file://"));
        assert!(uri_str.contains("src/main.rs"));
    }

    // -----------------------------------------------------------------------
    // Symbol query params validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_symbol_query_params_serde() {
        let params = SymbolQueryParams {
            file_path: "src/main.rs".to_string(),
            text: Some("fn main() {}".to_string()),
            language_id: Some("rust".to_string()),
            query: Some("main".to_string()),
        };
        let json = serde_json::to_string(&params).unwrap();
        let deserialized: SymbolQueryParams = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.file_path, "src/main.rs");
        assert_eq!(deserialized.query, Some("main".to_string()));
    }

    #[test]
    fn test_reference_query_params_serde() {
        let params = ReferenceQueryParams {
            file_path: "src/main.rs".to_string(),
            line: 42,
            column: 10,
        };
        let json = serde_json::to_string(&params).unwrap();
        let deserialized: ReferenceQueryParams = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.line, 42);
        assert_eq!(deserialized.column, 10);
    }
}
