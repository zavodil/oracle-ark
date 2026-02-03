#!/bin/bash
set -e

# cargo generate-lockfile

echo "Building price-oracle contract in Docker..."

# Use the same Docker image as MPC/register contracts
DOCKER_IMAGE="sourcescan/cargo-near:0.17.0-rust-1.86.0"

# Run build in Docker container
docker run --rm \
  -v "$(pwd)":/contract \
  -w /contract \
  "$DOCKER_IMAGE" \
  cargo near build non-reproducible-wasm --features abi --no-embed-abi

# Create res directory if not exists
mkdir -p res

# Copy WASM file
cp target/near/price_oracle.wasm res/price_oracle.wasm

# Show file size
ls -lh res/price_oracle.wasm

echo "✅ Build complete: res/price_oracle.wasm"
echo "Built in Docker: $DOCKER_IMAGE"
