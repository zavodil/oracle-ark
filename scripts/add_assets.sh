#!/bin/bash
# Add all assets from tokens.json to the oracle contract
#
# Usage: ./add_assets.sh <contract_id> <owner_account> [network]
# Example: ./add_assets.sh price-oracle.testnet zavodil2.testnet testnet
# Example: ./add_assets.sh oracle-ark.near owner.near mainnet

set -e

CONTRACT_ID="${1:?Usage: $0 <contract_id> <owner_account> [network]}"
OWNER_ACCOUNT="${2:?Usage: $0 <contract_id> <owner_account> [network]}"
NETWORK="${3:-testnet}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOKENS_FILE="$SCRIPT_DIR/../tokens.json"

if [ ! -f "$TOKENS_FILE" ]; then
    echo "Error: tokens.json not found at $TOKENS_FILE"
    exit 1
fi

echo "Adding assets to $CONTRACT_ID on $NETWORK..."
echo "Owner: $OWNER_ACCOUNT"
echo ""

# Extract token IDs from tokens.json
TOKENS=$(jq -r 'keys[]' "$TOKENS_FILE")

for TOKEN in $TOKENS; do
    echo "Adding: $TOKEN"
    near call "$CONTRACT_ID" add_asset "{\"asset_id\": \"$TOKEN\"}" \
        --accountId "$OWNER_ACCOUNT" \
        --depositYocto 1 \
        --networkId "$NETWORK" \
        2>&1 || echo "  (may already exist)"
    echo ""
done

echo "Done! Added $(echo "$TOKENS" | wc -l | tr -d ' ') assets."
echo ""
echo "Verify with:"
echo "  near view $CONTRACT_ID get_assets '{}' --networkId $NETWORK"
