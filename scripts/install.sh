#!/usr/bin/env bash
#
# Serena.rs — Install script
#
# Works with both public AND private GitHub repos.
#
# Usage:
#   # From a local clone (recommended for private repos):
#   git clone https://github.com/flobsx/serena-rs.git
#   cd serena-rs && bash scripts/install.sh
#
#   # Via curl (public repos only):
#   curl -fsSL https://raw.githubusercontent.com/flobsx/serena-rs/main/scripts/install.sh | bash
#
# What it does:
#   1. Builds from source (preferred) OR downloads a pre-built binary
#   2. Installs to ~/.local/bin/serena
#   3. Optionally configures OpenCode MCP
#
# NOTE: if you pipe to `sh` and get "Illegal option -o pipefail",
# re-run with `| bash` instead. This script prefers bash for pipefail.
if [ -n "${BASH_VERSION:-}" ]; then
  set -euo pipefail
else
  set -eu
fi

REPO="flobsx/serena-rs"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
CONFIGURE_OPENCODE="${CONFIGURE_OPENCODE:-yes}"

# ── helpers ────────────────────────────────────────────────────────
die()   { echo >&2 "❌ $*"; exit 1; }
info()  { echo "➡️  $*"; }
ok()    { echo "✅ $*"; }
warn()  { echo >&2 "⚠️  $*"; }

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

# ── detect if running from within the repo ─────────────────────────
is_within_repo() {
  # Check if we're in a scripts/ dir that has Cargo.toml one level up
  local self_dir
  self_dir="$(cd "$(dirname "$0")" && pwd 2>/dev/null || echo "")"
  if [ -n "$self_dir" ] && [ -f "${self_dir}/../Cargo.toml" ]; then
    return 0  # true
  fi
  # Also check if piped (no $0 pointing to a real file)
  if [ ! -f "$0" ] || [ "$0" = "bash" ] || [ "$0" = "sh" ]; then
    return 1  # false — piped
  fi
  return 1
}

# ── build from source ──────────────────────────────────────────────
build_from_source() {
  if ! command -v cargo >/dev/null 2>&1; then
    die "Rust toolchain not found. Install it first: https://rustup.rs"
  fi

  # Running from a local clone?
  local repo_root=""
  if [ -f "$(dirname "$0")/../Cargo.toml" ] 2>/dev/null; then
    repo_root="$(cd "$(dirname "$0")/.." && pwd)"
  fi

  if [ -z "$repo_root" ]; then
    # Piped via curl — clone the repo first
    if ! command -v git >/dev/null 2>&1; then
      die "git not found — needed to clone the repo for source build"
    fi
    repo_root="/tmp/serena-build"
    rm -rf "$repo_root"
    info "Cloning flobsx/serena-rs…"
    git clone --depth 1 "https://github.com/flobsx/serena-rs.git" "$repo_root" 2>&1 | tail -2
  fi

  info "Building Serena.rs from source (this may take a few minutes)…"
  cd "$repo_root"
  cargo build --release 2>&1 | tail -3 || {
    local rc=$?
    die "Source build failed (exit $rc). Ensure Rust is up to date: rustup update"
  }
  ok "Build complete"

  mkdir -p "$INSTALL_DIR"
  cp "target/release/serena" "$INSTALL_DIR/serena"
  ok "Installed to ${INSTALL_DIR}/serena"
}

# ── download pre-built binary ──────────────────────────────────────
download_binary() {
  local platform="$1"
  local tmp_dir="/tmp/serena-install"
  rm -rf "$tmp_dir"
  mkdir -p "$tmp_dir"

  local release_tag
  # Get latest release tag via gh or API
  # Try gh (handles auth for both public and private)
  if command -v gh >/dev/null 2>&1; then
    info "Downloading Serena.rs for ${platform} via GitHub CLI…"
    if gh release download --repo "$REPO" --pattern "serena-${platform}.tar.gz" --dir "$tmp_dir" 2>/dev/null; then
      :  # success
    else
      warn "gh download failed — trying direct curl…"
    fi
  fi

  # Fallback: direct curl (public repo)
  if [ ! -f "$tmp_dir/serena.tar.gz" ]; then
    # try to get the latest release tag
    local tag="v0.1.0"
    local api_tag
    api_tag=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p') || true
    [ -n "$api_tag" ] && tag="$api_tag"
    local url="https://github.com/${REPO}/releases/download/${tag}/serena-${platform}.tar.gz"
    info "Downloading from ${url}…"
    if command -v curl >/dev/null 2>&1; then
      curl -fsSL "$url" -o "$tmp_dir/serena.tar.gz"
    elif command -v wget >/dev/null 2>&1; then
      wget -q "$url" -O "$tmp_dir/serena.tar.gz"
    fi
  fi

  if [ ! -f "$tmp_dir/serena.tar.gz" ]; then
    # Download failed — fall back to source build
    warn "Pre-built binary download failed. Trying source build…"
    cd "$(dirname "$0")/.." 2>/dev/null || die "Cannot find repo root"
    build_from_source
    return
  fi

  cd "$tmp_dir"
  tar xzf serena.tar.gz
  [ -f serena ] || die "archive doesn't contain 'serena' binary"
  chmod +x serena

  mkdir -p "$INSTALL_DIR"
  mv serena "$INSTALL_DIR/serena"
  ok "Installed to ${INSTALL_DIR}/serena"
  rm -rf "$tmp_dir"
}

# ── configure OpenCode MCP ─────────────────────────────────────────
configure_opencode() {
  local config_dir="${HOME}/.config/opencode"
  local config_file="${config_dir}/opencode.json"

  [ -d "$config_dir" ] || { info "OpenCode config dir not found — skipping MCP config"; return; }

  local tmp; tmp=$(mktemp)
  [ -f "$config_file" ] && cp "$config_file" "$tmp" || echo '{}' > "$tmp"

  if command -v python3 >/dev/null 2>&1; then
    BINARY_PATH="${INSTALL_DIR}/serena"
    python3 -c "
import json
with open('$tmp') as f: cfg = json.load(f)
cfg.setdefault('mcp', {})['serena-rs'] = {'type': 'local', 'command': ['${BINARY_PATH}', 'start-mcp-server']}
with open('$tmp', 'w') as f: json.dump(cfg, f, indent=2, default=str)
" && cp "$tmp" "$config_file" && ok "OpenCode MCP 'serena-rs' configured"
  else
    warn "Add to ${config_file} manually:

  \"serena-rs\": {
    \"type\": \"local\",
    \"command\": [\"${INSTALL_DIR}/serena\", \"start-mcp-server\"]
  }"
  fi
  rm -f "$tmp"
}

# ── print summary ──────────────────────────────────────────────────
print_summary() {
  local version=""
  version=$("$INSTALL_DIR/serena" --version 2>&1 | head -1) || true

  echo ""
  echo "┌──────────────────────────────────────────┐"
  echo "│         🦀  Serena.rs  installed         │"
  echo "├──────────────────────────────────────────┤"
  echo "│  Binary:  ${INSTALL_DIR}/serena"
  [ -n "$version" ] && echo "│  Version: ${version}"
  echo "│                                          │"
  echo "│  Usage:                                   │"
  echo "│    serena --help                          │"
  echo "│    serena start-mcp-server                │"
  echo "│                                          │"
  if [ "$CONFIGURE_OPENCODE" = "yes" ]; then
    echo "│  Restart OpenCode and type /mcp to check │"
  fi
  echo "└──────────────────────────────────────────┘"
}

# ── check for Rust toolchain and suggest cargo install ─────────────
suggest_cargo_install() {
  if command -v cargo >/dev/null 2>&1; then
    echo ""
    info "Alternatively, install directly from GitHub:"
    echo "  cargo install --git https://github.com/${REPO}.git"
    echo ""
  fi
}

# ── main ───────────────────────────────────────────────────────────
main() {
  echo "  🦀  Serena.rs — MCP Toolkit for Coding Agents"
  echo ""

  if is_within_repo; then
    info "Running from local clone — building from source"
    build_from_source
  else
    local platform
    platform=$(detect_platform)
    download_binary "$platform"

    # Verify binary works
    if ! "$INSTALL_DIR/serena" --version >/dev/null 2>&1; then
      die "installed binary doesn't run"
    fi
  fi

  # PATH hint
  case ":$PATH:" in
    *":${INSTALL_DIR}:"*) ;;
    *)
      info "Add ${INSTALL_DIR} to your PATH (~/.bashrc / ~/.zshrc):"
      echo "  export PATH=\"\$PATH:${INSTALL_DIR}\""
      ;;
  esac

  if [ "$CONFIGURE_OPENCODE" = "yes" ]; then
    configure_opencode
  fi

  print_summary
  suggest_cargo_install
}

main
