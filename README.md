# 🦀 Serena.rs — MCP Toolkit for Coding Agents

Rewrite of [Serena](https://github.com/oraios/serena) (Python) in Rust.

Serena.rs provides semantic code retrieval, editing and refactoring tools for AI coding agents via the [Model Context Protocol (MCP)](https://modelcontextprotocol.io). It integrates with any MCP client — **OpenCode**, Claude Code, Codex, Cursor, etc.

> **Your agent gets IDE-level understanding**: go-to-definition, find references, rename symbols, code actions, diagnostics — all at the symbol level, not line-level text search.

---

## Installation

### Option 1 — One-liner (recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/flobsx/serena-rs/main/scripts/install.sh | bash
```

Downloads a pre-built, fully static binary (musl, no glibc dependency) and installs it to `~/.local/bin/serena`. Optionally configures OpenCode MCP automatically.

### Option 2 — Cargo install

```bash
cargo install --git https://github.com/flobsx/serena-rs.git
```

Requires Rust toolchain. Binary goes to `~/.cargo/bin/serena`.

### Option 3 — Manual download

```bash
curl -fsSL https://github.com/flobsx/serena-rs/releases/download/v0.1.0/serena-x86_64-unknown-linux-gnu.tar.gz -o /tmp/serena.tar.gz
tar xzf /tmp/serena.tar.gz -C /tmp
chmod +x /tmp/serena
mv /tmp/serena ~/.local/bin/
```

---

## OpenCode configuration

Add the MCP server to `~/.config/opencode/opencode.json`:

```json
{
  "mcp": {
    "serena-rs": {
      "type": "local",
      "command": ["~/.local/bin/serena", "start-mcp-server"]
    }
  }
}
```

If you installed via `cargo install`, the path is `~/.cargo/bin/serena` instead.

Restart OpenCode, then type `/mcp` — you should see `serena-rs` connected.

> **Tip**: add this to your `~/.config/opencode/AGENTS.md` so OpenCode knows how to use Serena:
> ```
> ## MCP — Serena.rs
> Serena.rs provides semantic code tools via MCP. Available tools:
> - get_definition — Jump to symbol definition
> - find_references — Find all references to a symbol
> - get_diagnostics — Get code diagnostics from language server
> - rename_symbol — Rename a symbol across the project
> - get_symbols — List symbols in a file
> - get_hover — Get hover info for a symbol
> - get_completion — Get code completion suggestions
> - apply_code_action — Apply a code action from the language server
> - format_code — Format code using the language server
> Use these when the user asks about code structure, navigation, or refactoring.
> ```

---

## Usage

```bash
serena --help
serena start-mcp-server          # Start MCP server (stdio transport)
serena init                       # Initialize a new project
serena setup                      # Run setup wizard
serena config-schema              # Print JSON schema for config
```

---

## Build from source

```bash
git clone https://github.com/flobsx/serena-rs.git
cd serena-rs

# Build (static musl by default)
cargo build --release

# Run tests
cargo test --workspace --all-targets

# Start the MCP server
cargo run -- start-mcp-server
```

Prerequisites: `build-essential libssl-dev pkg-config` (Ubuntu/Debian) or equivalent.

---

## Workspace layout

```
serena-rs/
├── Cargo.toml               # Workspace root
├── src/
│   └── main.rs              # CLI entrypoint (clap)
├── crates/
│   ├── serena-core/         # Agent orchestration, context, modes
│   ├── serena-mcp/          # MCP server (rmcp, stdio transport)
│   ├── serena-tools/        # 9 MCP tools (symbol, file, shell, config…)
│   ├── serena-lsp/          # LSP client (tower-lsp, 12 builtin languages)
│   ├── serena-editor/       # Symbolic code editing (insert, replace, delete)
│   ├── serena-project/      # Project management, ignore spec, file scanning
│   ├── serena-memory/       # Persistent memory manager
│   ├── serena-config/       # YAML config loading & validation
│   └── serena-util/         # Shared utilities (tokens, path, fs, text)
```

---

## Project status

| Area | Status |
|------|--------|
| MCP transport (stdio + HTTP) | ✅ |
| LSP client (12 languages)    | ✅ |
| Semantic tools (9 tools)     | ✅ |
| CLI (init, setup, mcp)       | ✅ |
| Tests                        | ✅ 76 tests |
| Web dashboard (post-MVP)     | 📅 |

---

## License

MIT
