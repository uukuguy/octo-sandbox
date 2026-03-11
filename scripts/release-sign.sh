#!/usr/bin/env bash
# Tauri updater key generation and manifest helper
# Usage:
#   ./scripts/release-sign.sh generate-key   # Generate new signing keypair
#   ./scripts/release-sign.sh version         # Show current version

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cmd_generate_key() {
    echo "Generating Tauri updater signing key..."
    echo "Run: npx @tauri-apps/cli signer generate -w ~/.tauri/octo-sandbox.key"
    echo ""
    echo "After generating, add the PUBLIC key to tauri.conf.json plugins.updater.pubkey"
    echo "Set TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD as env vars or GitHub secrets"
}

cmd_version() {
    local version
    version=$(grep '^version' "$PROJECT_ROOT/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')
    echo "Current version: $version"
}

case "${1:-help}" in
    generate-key) cmd_generate_key ;;
    version) cmd_version ;;
    *)
        echo "Usage: $0 {generate-key|version}"
        exit 1
        ;;
esac
