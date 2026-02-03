#!/bin/bash
set -e
cd "$(dirname "$0")"
cargo near build non-reproducible-wasm
cp target/near/*.wasm res/ 2>/dev/null || true
echo "Build complete. WASM files in res/"
