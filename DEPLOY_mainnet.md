### Deploy to mainnet

```bash
# Deploy contract
near contract deploy price-oracle.near use-file ./res/price_oracle.wasm without-init-call network-config mainnet sign-with-keychain send

# Initialize contract
near call price-oracle.near new '{
  "recency_duration_sec": 300,
  "owner_id": "owner.price-oracle.near",
  "near_claim_amount": "100000000000000000000000"
}' --accountId price-oracle.near --networkId mainnet


# Upgrade contract
WASM_BASE64=$(base64 -i target/near/price_oracle.wasm)
near call price-oracle.near upgrade --base64 "$WASM_BASE64" --accountId owner.price-oracle.near --gas 300000000000000 --networkId mainnet
```


### Configure assets


```bash
# Add all assets to track
./scripts/add_assets.sh price-oracle.near owner.price-oracle.near mainnet
```