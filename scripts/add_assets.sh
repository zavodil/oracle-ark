#!/bin/bash
# Add all assets from tokens.json to the oracle contract.
#
# Assets are registered via a council/DAO proposal (AddAsset action), not a direct
# owner call. The account passed below must be a COUNCIL MEMBER; each proposal
# auto-executes once the approval threshold is met (a single-member council executes
# immediately). This only registers the assets (warm-only, push_signer_key = null);
# their source configs are pushed separately via SetAssetExchangeConfig + sync_asset_configs.
#
# Usage: ./add_assets.sh <contract_id> <council_member> [network]
# Example: ./add_assets.sh price-oracle.testnet zavodil2.testnet testnet
# Example: ./add_assets.sh price-oracle.near council-member.near mainnet

set -e

CONTRACT_ID="${1:?Usage: $0 <contract_id> <council_member> [network]}"
COUNCIL_MEMBER="${2:?Usage: $0 <contract_id> <council_member> [network]}"
NETWORK="${3:-testnet}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOKENS_FILE="$SCRIPT_DIR/../tokens.json"

if [ ! -f "$TOKENS_FILE" ]; then
    echo "Error: tokens.json not found at $TOKENS_FILE"
    exit 1
fi

echo "Adding assets to $CONTRACT_ID on $NETWORK..."
echo "Council member: $COUNCIL_MEMBER"
echo ""

# Extract token IDs from tokens.json
TOKENS=$(jq -r 'keys[]' "$TOKENS_FILE")

for TOKEN in $TOKENS; do
    echo "Adding: $TOKEN"
    near call "$CONTRACT_ID" create_proposal \
        "{\"action\": {\"action\": \"add_asset\", \"asset_id\": \"$TOKEN\", \"push_signer_key\": null}}" \
        --accountId "$COUNCIL_MEMBER" \
        --deposit 0.1 \
        --networkId "$NETWORK" \
        2>&1 || echo "  (may already exist)"
    echo ""
done

echo "Done! Added $(echo "$TOKENS" | wc -l | tr -d ' ') assets."
echo ""
echo "Verify with:"
echo "  near view $CONTRACT_ID get_assets '{}' --networkId $NETWORK"
