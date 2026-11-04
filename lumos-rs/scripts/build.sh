#!/usr/bin/env bash
# Build the web UI, then the single-binary server (UI is embedded at compile time).
set -euo pipefail
cd "$(dirname "$0")/.."
(cd ui && npm ci && npm run build)
cargo build --release
echo "→ target/release/lumos"
