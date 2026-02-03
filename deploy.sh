#!/bin/bash
set -e

# =============================================================================
# CONFIGURATION - Edit these for your project
# =============================================================================

# Project paths (relative to this script's directory)
WASI_DIR="."
CONTRACT_DIR="./contract"
SCRIPTS_DIR="../../scripts"

# WASI build output
WASI_TARGET="wasm32-wasip2"
WASI_BINARY_NAME="oracle-ark"

# Contract build output
CONTRACT_WASM="res/price_oracle.wasm"

# NEAR accounts
DEPLOYER_ACCOUNT="zavodil2.testnet"
OUTLAYER_CONTRACT="outlayer.testnet"
CONTRACT_ACCOUNT="price-oracle.testnet"

# Project name in OutLayer
PROJECT_NAME="price-oracle"

# Network
NETWORK="testnet"
RPC_URL="https://rpc.testnet.near.org"

# Worker env file for FastFS upload
WORKER_ENV="../../worker/.env.dev.worker1"

# =============================================================================
# END CONFIGURATION
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "========================================"
echo "Deploying $PROJECT_NAME to $NETWORK"
echo "========================================"
echo ""

# Step 1: Build WASI
echo "[1/5] Building WASI..."
cd "$WASI_DIR"
./build.sh
WASI_WASM="target/${WASI_TARGET}/release/${WASI_BINARY_NAME}.wasm"

if [ ! -f "$WASI_WASM" ]; then
    echo "ERROR: WASI build failed - $WASI_WASM not found"
    exit 1
fi
echo "    Built: $WASI_WASM"
echo ""

# Step 2: Upload to FastFS
echo "[2/5] Uploading WASI to FastFS..."
cd "$SCRIPT_DIR"
UPLOAD_OUTPUT=$(python3 "$SCRIPTS_DIR/upload_wasm_fastfs.py" "$WASI_DIR/$WASI_WASM" "$WORKER_ENV" "$RPC_URL" 2>&1)
echo "$UPLOAD_OUTPUT"

# Extract hash from output
WASM_HASH=$(echo "$UPLOAD_OUTPUT" | grep "SHA256:" | awk '{print $2}')
FASTFS_URL=$(echo "$UPLOAD_OUTPUT" | grep "FastFS URL:" | awk '{print $3}')

if [ -z "$WASM_HASH" ]; then
    echo "ERROR: Failed to extract WASM hash from upload output"
    exit 1
fi

echo ""
echo "    Hash: $WASM_HASH"
echo "    URL: $FASTFS_URL"
echo ""

# Step 3: Verify FastFS URL is accessible
echo "[3/5] Verifying FastFS upload..."
sleep 2
HTTP_STATUS=$(curl -s -o /dev/null -w "%{http_code}" "$FASTFS_URL")
if [ "$HTTP_STATUS" != "200" ]; then
    echo "ERROR: FastFS URL not accessible (HTTP $HTTP_STATUS)"
    echo "    URL: $FASTFS_URL"
    exit 1
fi
echo "    FastFS URL accessible (HTTP 200)"
echo ""

# Step 4: Add version to OutLayer
echo "[4/5] Adding version to OutLayer..."
ADD_VERSION_ARGS=$(cat <<EOF
{
    "project_name": "$PROJECT_NAME",
    "source": {
        "WasmUrl": {
            "url": "$FASTFS_URL",
            "hash": "$WASM_HASH",
            "build_target": "$WASI_TARGET"
        }
    },
    "set_active": true
}
EOF
)

echo "    Calling add_version on $OUTLAYER_CONTRACT..."

near contract call-function as-transaction "$OUTLAYER_CONTRACT" add_version json-args "$ADD_VERSION_ARGS" \
    prepaid-gas '100.0 Tgas' attached-deposit '0.00433 NEAR' \
    sign-as "$DEPLOYER_ACCOUNT" network-config "$NETWORK" sign-with-keychain send

echo ""

# Step 5: Build and deploy contract
echo "[5/5] Building and deploying contract..."
cd "$SCRIPT_DIR/$CONTRACT_DIR"

# Build contract
./build_docker.sh 2>&1 | tee /tmp/contract_build.log

# Check build success
if ! grep -q "Build complete" /tmp/contract_build.log; then
    echo "ERROR: Contract build failed"
    cat /tmp/contract_build.log
    exit 1
fi

if [ ! -f "$CONTRACT_WASM" ]; then
    echo "ERROR: Contract WASM not found at $CONTRACT_WASM"
    exit 1
fi

echo "    Built: $CONTRACT_WASM"

# Deploy contract
echo "    Deploying to $CONTRACT_ACCOUNT..."
near contract deploy "$CONTRACT_ACCOUNT" \
    use-file "./$CONTRACT_WASM" \
    without-init-call \
    network-config "$NETWORK" \
    sign-with-keychain send

echo ""
echo "========================================"
echo "Deployment complete!"
echo "========================================"
echo ""
echo "WASI hash: $WASM_HASH"
echo "FastFS URL: $FASTFS_URL"
echo "Contract: $CONTRACT_ACCOUNT"
echo ""
