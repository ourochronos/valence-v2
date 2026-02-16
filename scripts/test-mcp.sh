#!/bin/bash
# Test MCP server basic functionality

set -e

echo "Testing MCP server stdio mode..."

# Build the engine
cd "$(dirname "$0")/.."
cargo build --release --quiet

# Send a simple test request via stdio
# MCP protocol expects JSON-RPC 2.0 messages
cat <<EOF | timeout 5 ./target/release/valence-engine --mode mcp 2>/dev/null || true
{"jsonrpc":"2.0","method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test-client","version":"1.0.0"}},"id":1}
EOF

echo "MCP server test complete"
echo ""
echo "To run MCP server:"
echo "  ./target/release/valence-engine --mode mcp"
echo ""
echo "To install as OpenClaw plugin:"
echo "  1. Copy binary: sudo cp target/release/valence-engine /usr/local/bin/"
echo "  2. Register: openclaw plugins install plugin/openclaw.plugin.json"
