# Serena.rs — MCP Toolkit for Coding Agents (Rust rewrite)

Rewrite of [Serena](https://github.com/oraios/serena) (Python) in Rust.

## Workspace layout

```
serena-rs/
├── Cargo.toml              # Workspace root
├── src/
│   └── main.rs             # CLI entrypoint
├── crates/
│   ├── serena-core/        # Agent orchestration, config, modes
│   ├── serena-mcp/         # MCP server (stdio transport)
│   ├── serena-tools/       # MCP tool implementations
│   ├── serena-lsp/         # LSP client + symbol retrieval
│   ├── serena-editor/      # Symbolic code editing
│   ├── serena-project/     # Project management, ignore spec, file scanning
│   ├── serena-memory/      # Persistent memory manager
│   ├── serena-config/      # YAML config loading & validation
│   ├── serena-dashboard/   # Web dashboard (post-MVP)
│   └── serena-util/        # Shared utilities
```

## Build

```bash
cargo build
cargo run -- start-mcp-server
```

## V1 Scope

See [architecture analysis](/home/maya/workspace/obsidian-vault/1-projects/Serena.rs.md).
