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


# Upgrade contract (there is no `upgrade` method — redeploy directly with the
# account's full-access key and run the migration that matches the deployed state:
#   migrate_state  -> pre-Pyth/Council
#   migrate_state2 -> pre-oracle-keys
#   migrate_state3 -> pre-exchange-configs
near contract deploy price-oracle.near \
  use-file target/near/price_oracle.wasm \
  with-init-call migrate_state2 json-args '{}' \
  prepaid-gas '300.0 Tgas' attached-deposit '0 NEAR' \
  network-config mainnet sign-with-keychain send
```

### Configure contract to use OutLayer

`configure_outlayer` is now a DAO action. Bootstrap a council first (owner only):

```bash
near call price-oracle.near set_council '{
  "members": ["owner.price-oracle.near"]
}' --accountId owner.price-oracle.near --depositYocto 1 --networkId mainnet
```

Then propose the configuration (a single-member council auto-executes):

```bash
near call price-oracle.near create_proposal '{"action": {
  "action": "configure_outlayer",
  "outlayer_contract_id": "outlayer.near",
  "code_source": "{\"Project\": {\"project_id\": \"price-oracle.near/price-oracle\"}}",
  "secrets_profile": "default",
  "secrets_account_id": "price-oracle.near"
}}' --accountId owner.price-oracle.near --networkId mainnet --deposit 0.1
```

### Configure assets


```bash
# Add all assets to track
./scripts/add_assets.sh price-oracle.near owner.price-oracle.near mainnet
```