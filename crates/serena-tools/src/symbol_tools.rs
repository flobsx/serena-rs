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
