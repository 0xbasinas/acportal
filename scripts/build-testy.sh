#!/usr/bin/env sh
# Build `testy`, the deterministic ACP test agent from the official Rust SDK, at a pinned
# revision and print the binary path. The checkout lives outside this repository.
#
#   ACPD_TESTY_BIN="$(scripts/build-testy.sh)" cargo test -p acpd --test testy
#
# Pin: rust-sdk v3.2.0 uses agent-client-protocol-schema 1.10.2, the version acpd pins.
# Override the cache directory with ACPD_TESTY_CACHE. Progress goes to stderr; only the
# binary path goes to stdout.
set -eu
REPO="https://github.com/agentclientprotocol/rust-sdk"
REV="5c41d62297eb74cce06daac8b3a487c6c453d552" # tag v3.2.0
CACHE="${ACPD_TESTY_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/acpd-testy}"
SRC="$CACHE/rust-sdk"

mkdir -p "$CACHE"
if [ ! -d "$SRC/.git" ]; then
  git clone --quiet "$REPO" "$SRC" >&2
fi
if [ "$(git -C "$SRC" rev-parse HEAD)" != "$REV" ]; then
  git -C "$SRC" fetch --quiet origin "$REV" >&2 || git -C "$SRC" fetch --quiet --tags origin >&2
  git -C "$SRC" -c advice.detachedHead=false checkout --quiet "$REV" >&2
fi
# The SDK's own lockfile keeps the build reproducible.
# mcp-echo-server is the SDK's stdio MCP test server; the MCP test finds it next to testy.
cargo build --locked --manifest-path "$SRC/Cargo.toml" -p agent-client-protocol-test \
  --bin testy --bin mcp-echo-server >&2
# mcp-http-echo (tools/mcp-http-echo, this repository) is a minimal Streamable HTTP MCP
# server for the HTTP MCP test; it is built into the same directory so the test finds it.
HERE="$(cd "$(dirname "$0")/.." && pwd)"
cargo build --locked --manifest-path "$HERE/tools/mcp-http-echo/Cargo.toml" \
  --target-dir "$SRC/target" >&2
BIN="$SRC/target/debug/testy"
test -x "$BIN"
test -x "$SRC/target/debug/mcp-echo-server"
test -x "$SRC/target/debug/mcp-http-echo"
echo "$BIN"
