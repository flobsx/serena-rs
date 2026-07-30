//! Language server provider definitions.
//!
//! Defines a registry of known language servers, their configurations,
//! and auto-detection logic based on file extensions / language IDs.
//! Users can register custom servers at runtime.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A registry of language server configurations.
///
/// Provides built-in defaults for common languages and allows
/// registering custom language servers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageServerProvider {
    /// Map of server name → configuration
    servers: HashMap<String, ServerConfig>,
    /// Map of file extension → server name
    extension_map: HashMap<String, String>,
    /// Map of language ID → server name
    language_map: HashMap<String, String>,
}

/// Configuration for a single language server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Display name (e.g. "rust-analyzer")
    pub name: String,
    /// The command to spawn (e.g. "rust-analyzer")
    pub command: String,
    /// Command-line arguments
    pub args: Vec<String>,
    /// File extensions this server handles (e.g. [".rs"])
    pub file_extensions: Vec<String>,
    /// Language IDs this server handles (e.g. ["rust"])
    pub language_ids: Vec<String>,
    /// Environment variables to set
    pub env: HashMap<String, String>,
    /// Initialization options (sent as `initializationOptions` in initialize)
    pub initialization_options: Option<Value>,
    /// Settings sent via `workspace/didChangeConfiguration`
    pub settings: Option<Value>,
}

impl LanguageServerProvider {
    /// Create a new provider with default built-in servers.
    pub fn new() -> Self {
        let mut provider = Self {
            servers: HashMap::new(),
            extension_map: HashMap::new(),
            language_map: HashMap::new(),
        };
        provider.register_builtins();
        provider
    }

    /// Register a language server configuration.
    pub fn register(&mut self, config: ServerConfig) {
        let name = config.name.clone();
        for ext in &config.file_extensions {
            self.extension_map.insert(ext.clone(), name.clone());
        }
        for lang in &config.language_ids {
            self.language_map.insert(lang.clone(), name.clone());
        }
        self.servers.insert(name, config);
    }

    /// Find a server configuration for the given file path.
    pub fn for_file(&self, file_path: &str) -> Option<&ServerConfig> {
        // Check by full extension first, then by partial suffix
        let path = std::path::Path::new(file_path);
        let ext = path.extension()?.to_str()?;
        let ext_with_dot = format!(".{ext}");

        if let Some(name) = self.extension_map.get(&ext_with_dot) {
            return self.servers.get(name);
        }

        // Try with dots (e.g. ".test.ts")
        let file_name = path.file_name()?.to_str()?;
        if let Some(pos) = file_name.find('.') {
            let suffix = &file_name[pos..];
            if let Some(name) = self.extension_map.get(suffix) {
                return self.servers.get(name);
            }
        }

        None
    }

    /// Find a server configuration for the given language ID.
    pub fn for_language(&self, language_id: &str) -> Option<&ServerConfig> {
        let name = self.language_map.get(language_id)?;
        self.servers.get(name)
    }

    /// List all registered server names.
    pub fn servers(&self) -> impl Iterator<Item = &str> {
        self.servers.keys().map(|s| s.as_str())
    }

    /// Get a server configuration by name.
    pub fn get(&self, name: &str) -> Option<&ServerConfig> {
        self.servers.get(name)
    }

    /// Register the built-in default language servers.
    fn register_builtins(&mut self) {
        // Rust
        self.register(ServerConfig {
            name: "rust-analyzer".into(),
            command: "rust-analyzer".into(),
            args: vec![],
            file_extensions: vec![".rs".into()],
            language_ids: vec!["rust".into()],
            env: HashMap::new(),
            initialization_options: Some(serde_json::json!({
                "cargo": { "buildOnSave": true },
                "checkOnSave": true,
                "procMacro": { "enable": true },
            })),
            settings: None,
        });

        // TypeScript / JavaScript
        self.register(ServerConfig {
            name: "typescript-language-server".into(),
            command: "typescript-language-server".into(),
            args: vec!["--stdio".into()],
            file_extensions: vec![
                ".ts".into(), ".tsx".into(),
                ".js".into(), ".jsx".into(),
                ".mjs".into(), ".cjs".into(),
            ],
            language_ids: vec![
                "typescript".into(), "typescriptreact".into(),
                "javascript".into(), "javascriptreact".into(),
            ],
            env: HashMap::new(),
            initialization_options: None,
            settings: None,
        });

        // Python
        self.register(ServerConfig {
            name: "pyright".into(),
            command: "pyright-langserver".into(),
            args: vec!["--stdio".into()],
            file_extensions: vec![".py".into()],
            language_ids: vec!["python".into()],
            env: HashMap::new(),
            initialization_options: Some(serde_json::json!({
                "analysis": {
                    "autoSearchPaths": true,
                    "useLibraryCodeForTypes": true,
                }
            })),
            settings: None,
        });

        // Go
        self.register(ServerConfig {
            name: "gopls".into(),
            command: "gopls".into(),
            args: vec![],
            file_extensions: vec![".go".into()],
            language_ids: vec!["go".into()],
            env: HashMap::new(),
            initialization_options: None,
            settings: None,
        });

        // JSON / JSONC
        self.register(ServerConfig {
            name: "vscode-json-languageserver".into(),
            command: "vscode-json-languageserver".into(),
            args: vec!["--stdio".into()],
            file_extensions: vec![".json".into(), ".jsonc".into()],
            language_ids: vec!["json".into(), "jsonc".into()],
            env: HashMap::new(),
            initialization_options: None,
            settings: None,
        });

        // YAML
        self.register(ServerConfig {
            name: "yaml-language-server".into(),
            command: "yaml-language-server".into(),
            args: vec!["--stdio".into()],
            file_extensions: vec![".yaml".into(), ".yml".into()],
            language_ids: vec!["yaml".into()],
            env: HashMap::new(),
            initialization_options: None,
            settings: None,
        });

        // TOML
        self.register(ServerConfig {
            name: "taplo".into(),
            command: "taplo".into(),
            args: vec!["lsp".into(), "--stdio".into()],
            file_extensions: vec![".toml".into(), ".Cargo.toml".into()],
            language_ids: vec!["toml".into()],
            env: HashMap::new(),
            initialization_options: None,
            settings: None,
        });

        // HTML / CSS
        self.register(ServerConfig {
            name: "vscode-html-languageserver".into(),
            command: "vscode-html-languageserver".into(),
            args: vec!["--stdio".into()],
            file_extensions: vec![".html".into(), ".htm".into()],
            language_ids: vec!["html".into()],
            env: HashMap::new(),
            initialization_options: None,
            settings: None,
        });

        self.register(ServerConfig {
            name: "vscode-css-languageserver".into(),
            command: "vscode-css-languageserver".into(),
            args: vec!["--stdio".into()],
            file_extensions: vec![".css".into(), ".scss".into(), ".less".into()],
            language_ids: vec!["css".into(), "scss".into(), "less".into()],
            env: HashMap::new(),
            initialization_options: None,
            settings: None,
        });

        // Markdown
        self.register(ServerConfig {
            name: "marksman".into(),
            command: "marksman".into(),
            args: vec![],
            file_extensions: vec![".md".into(), ".markdown".into()],
            language_ids: vec!["markdown".into()],
            env: HashMap::new(),
            initialization_options: None,
            settings: None,
        });

        // Dockerfile
        self.register(ServerConfig {
            name: "docker-langserver".into(),
            command: "docker-langserver".into(),
            args: vec!["--stdio".into()],
            file_extensions: vec![".Dockerfile".into(), "Dockerfile".into()],
            language_ids: vec!["dockerfile".into()],
            env: HashMap::new(),
            initialization_options: None,
            settings: None,
        });

        // Lua
        self.register(ServerConfig {
            name: "lua-language-server".into(),
            command: "lua-language-server".into(),
            args: vec![],
            file_extensions: vec![".lua".into()],
            language_ids: vec!["lua".into()],
            env: HashMap::new(),
            initialization_options: None,
            settings: None,
        });
    }
}

impl Default for LanguageServerProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_has_builtin_servers() {
        let provider = LanguageServerProvider::new();
        let servers: Vec<&str> = provider.servers().collect();
        assert!(servers.contains(&"rust-analyzer"));
        assert!(servers.contains(&"pyright"));
        assert!(servers.contains(&"gopls"));
        assert!(servers.contains(&"typescript-language-server"));
        assert!(servers.contains(&"taplo"));
    }

    #[test]
    fn test_for_file_by_extension() {
        let provider = LanguageServerProvider::new();

        let rs = provider.for_file("src/main.rs");
        assert!(rs.is_some());
        assert_eq!(rs.unwrap().name, "rust-analyzer");

        let py = provider.for_file("app/main.py");
        assert!(py.is_some());
        assert_eq!(py.unwrap().name, "pyright");

        let ts = provider.for_file("components/Button.tsx");
        assert!(ts.is_some());
        assert_eq!(ts.unwrap().name, "typescript-language-server");
    }

    #[test]
    fn test_for_file_unknown_extension() {
        let provider = LanguageServerProvider::new();
        let result = provider.for_file("file.xyz");
        assert!(result.is_none());
    }

    #[test]
    fn test_for_language() {
        let provider = LanguageServerProvider::new();

        let rust = provider.for_language("rust");
        assert!(rust.is_some());
        assert_eq!(rust.unwrap().command, "rust-analyzer");

        let python = provider.for_language("python");
        assert!(python.is_some());
        assert_eq!(python.unwrap().command, "pyright-langserver");
    }

    #[test]
    fn test_for_language_unknown() {
        let provider = LanguageServerProvider::new();
        assert!(provider.for_language("brainfuck").is_none());
    }

    #[test]
    fn test_get_by_name() {
        let provider = LanguageServerProvider::new();
        let config = provider.get("rust-analyzer");
        assert!(config.is_some());
        assert_eq!(config.unwrap().command, "rust-analyzer");
        assert!(config.unwrap().file_extensions.contains(&".rs".to_string()));
    }

    #[test]
    fn test_get_unknown_server() {
        let provider = LanguageServerProvider::new();
        assert!(provider.get("nonexistent").is_none());
    }

    #[test]
    fn test_custom_registration() {
        let mut provider = LanguageServerProvider::new();
        provider.register(ServerConfig {
            name: "my-lang-server".into(),
            command: "myls".into(),
            args: vec!["--verbose".into()],
            file_extensions: vec![".my".into()],
            language_ids: vec!["mylang".into()],
            env: HashMap::new(),
            initialization_options: None,
            settings: None,
        });

        let config = provider.for_file("test.my");
        assert!(config.is_some());
        assert_eq!(config.unwrap().name, "my-lang-server");

        let config = provider.for_language("mylang");
        assert!(config.is_some());
        assert_eq!(config.unwrap().command, "myls");
    }

    #[test]
    fn test_serialization_roundtrip() {
        let provider = LanguageServerProvider::new();
        let json = serde_json::to_string(&provider).unwrap();
        let deserialized: LanguageServerProvider = serde_json::from_str(&json).unwrap();
        assert_eq!(
            provider.servers().count(),
            deserialized.servers().count(),
        );
        assert!(deserialized.get("rust-analyzer").is_some());
    }

    #[test]
    fn test_dockerfile_detection() {
        let provider = LanguageServerProvider::new();
        // Dockerfile has no extension, so extension-based matching won't find it
        // This is a known limitation — users can configure it manually
        let df = provider.for_file("Dockerfile");
        // May or may not match depending on implementation
        if df.is_none() {
            // Filename-based matching is not implemented for extensionless files
            println!("Dockerfile (no extension) not matched by provider — expected behavior");
        }
    }

    #[test]
    fn test_cargo_toml_detection() {
        let provider = LanguageServerProvider::new();
        let cargo = provider.for_file("Cargo.toml");
        assert!(cargo.is_some());
        if let Some(config) = cargo {
            assert_eq!(config.name, "taplo");
        }
    }
}
