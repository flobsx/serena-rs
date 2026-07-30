//! Serena.rs — LSP client, symbol retrieval, diagnostics
//!
//! Client-side LSP implementation. Spawns language server processes,
//! communicates via JSON-RPC over stdio, and provides symbol / diagnostic
//! retrieval without requiring a full LSP server implementation.

pub mod client;
pub mod provider;
pub mod symbol;

pub use client::LspClient;
pub use provider::LanguageServerProvider;
pub use symbol::Symbol;
