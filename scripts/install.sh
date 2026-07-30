#!/usr/bin/env bash
#
# Serena.rs — One-shot install script
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/maya-bsx/serena-rs/main/scripts/install.sh | sh
#
# What it does:
#   1. Downloads the pre-built binary for your platform from GitHub Releases
#   2. Installs it to ~/.local/bin/serena
#   3. Optionally configures OpenCode MCP
#   4. Prints a summary
#
set -euo pipefail

REPO="flobsx/serena-rs"
VERSION="${1:-latest}"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
CONFIGURE_OPENCODE="${CONFIGURE_OPENCODE:-yes}"

# ── helpers ────────────────────────────────────────────────────────
die() { echo >&2 "❌ $*"; exit 1; }
info() { echo "➡️  $*"; }
ok()   { echo "✅ $*"; }

# ── platform detection ─────────────────────────────────────────────
detect_platform() {
  local os arch
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  arch="$(uname -m)"

  case "$os" in
    linux)  os="unknown-linux-gnu" ;;
    darwin) os="apple-darwin" ;;
    *)      die "unsupported OS: $os (only Linux/macOS)" ;;
  esac

  case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
    *) die "unsupported arch: $arch (only x86_64 / aarch64)" ;;
  esac

  echo "${arch}-${os}"
}

# ── download pre-built binary ──────────────────────────────────────
download_binary() {
  local platform="$1"
  local url

  if [ "$VERSION" = "latest" ]; then
    url="https://github.com/${REPO}/releases/latest/download/serena-${platform}.tar.gz"
  else
    url="https://github.com/${REPO}/releases/download/${VERSION}/serena-${platform}.tar.gz"
  fi

  info "Downloading Serena.rs for ${platform}…"
  mkdir -p /tmp/serena-install
  cd /tmp/serena-install

  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o serena.tar.gz
  elif command -v wget >/dev/null 2>&1; then
    wget -q "$url" -O serena.tar.gz
  else
    die "neither curl nor wget found — install one of them first"
  fi

  tar xzf serena.tar.gz
  if [ ! -f serena ]; then
    die "downloaded archive doesn't contain 'serena' binary"
  fi

  chmod +x serena
}

# ── install binary ─────────────────────────────────────────────────
install_binary() {
  mkdir -p "$INSTALL_DIR"

  # Move binary
  mv /tmp/serena-install/serena "$INSTALL_DIR/serena"

  # Test
  if ! "$INSTALL_DIR/serena" --version >/dev/null 2>&1; then
    die "installed binary doesn't run — corrupted download?"
  fi

  ok "Installed to ${INSTALL_DIR}/serena"
  rm -rf /tmp/serena-install
}

# ── configure OpenCode MCP ─────────────────────────────────────────
configure_opencode() {
  local config_dir="${HOME}/.config/opencode"
  local config_file="${config_dir}/opencode.json"

  if [ ! -d "$config_dir" ]; then
    info "OpenCode config dir not found — skipping MCP configuration"
    return
  fi

  # Read existing config or create new one
  local tmp
  tmp=$(mktemp)

  if [ -f "$config_file" ]; then
    cp "$config_file" "$tmp"
  else
    echo '{}' > "$tmp"
  fi

  # Inject serena-rs MCP server using python3 (available everywhere)
  # If python3 is not available, fall back to a simpler jq-less approach
  if command -v python3 >/dev/null 2>&1; then
    BINARY_PATH="${INSTALL_DIR}/serena"
    python3 -c "
import json, sys
with open('$tmp') as f:
    cfg = json.load(f)
cfg.setdefault('mcp', {})['serena-rs'] = {
    'type': 'local',
    'command': ['${BINARY_PATH}', 'start-mcp-server']
}
with open('$tmp', 'w') as f:
    json.dump(cfg, f, indent=2)
"
    cp "$tmp" "$config_file"
    ok "OpenCode MCP server 'serena-rs' configured in ${config_file}"
  else
    warn "python3 not found — add this to ${config_file} manually:

  \"serena-rs\": {
    \"type\": \"local\",
    \"command\": [\"${INSTALL_DIR}/serena\", \"start-mcp-server\"]
  }
"
  fi
  rm -f "$tmp"
}

# ── print summary ──────────────────────────────────────────────────
print_summary() {
  local version
  version=$("$INSTALL_DIR/serena" --version 2>&1 | head -1)

  echo ""
  echo "┌──────────────────────────────────────────┐"
  echo "│         🦀  Serena.rs  installed         │"
  echo "├──────────────────────────────────────────┤"
  echo "│  Binary:  ${INSTALL_DIR}/serena"
  echo "│  Version: ${version}"
  if [ "$CONFIGURE_OPENCODE" = "yes" ]; then
    echo "│  MCP:     configured for OpenCode         │"
  fi
  echo "│                                          │"
  echo "│  Usage:                                   │"
  echo "│    serena --help                          │"
  echo "│    serena start-mcp-server                │"
  echo "│                                          │"
  echo "│  Restart OpenCode and type /mcp to verify │"
  echo "└──────────────────────────────────────────┘"
}

# ── main ───────────────────────────────────────────────────────────
main() {
  echo "  🦀  Serena.rs — MCP Toolkit for Coding Agents"
  echo ""

  local platform
  platform=$(detect_platform)

  download_binary "$platform"
  install_binary

  # Add to PATH if needed
  if [[ ":$PATH:" != *":${INSTALL_DIR}:"* ]]; then
    info "Add ${INSTALL_DIR} to your PATH (e.g. in ~/.bashrc / ~/.zshrc):"
    echo "  export PATH=\"\$PATH:${INSTALL_DIR}\""
  fi

  if [ "$CONFIGURE_OPENCODE" = "yes" ]; then
    configure_opencode
  fi

  print_summary
}

main
