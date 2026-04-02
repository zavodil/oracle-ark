# Council Proposals

The oracle contract uses a council governance model. Proposals are created by council members
and auto-execute when the voting threshold is reached (threshold=1 means immediate execution).

## JSON Format

The contract uses `#[serde(tag = "action", rename_all = "snake_case")]` on the `ProposalAction` enum.
This means the action type is an **internally tagged enum** — the variant name goes into the `"action"` field
as a snake_case string, and the variant's fields are flattened into the same object.

```
near call <CONTRACT> create_proposal '{"action": {"action": "<variant>", ...fields}}' --deposit 0.1 --accountId <COUNCIL_MEMBER>
```

Note the double `"action"`: the outer one is the function parameter name, the inner one is the serde tag.

For large payloads, use a file to avoid shell escaping issues:

```bash
# Write JSON to file
cat > /tmp/proposal.json << 'EOF'
{"action": {"action": "add_oracle", "account_id": "new-oracle.near"}}
EOF

# Submit via file-args
near contract call-function as-transaction <CONTRACT> create_proposal \
  file-args /tmp/proposal.json \
  prepaid-gas '300.0 Tgas' attached-deposit '0.1 NEAR' \
  sign-as <COUNCIL_MEMBER> network-config mainnet sign-with-keychain send
```

## Deposit

- Minimum: 1 yoctoNEAR
- Proposals that increase storage (adding assets, configs) need ~0.03-0.1 NEAR
- Proposals that decrease storage (removing sources) need only 1 yoctoNEAR
- Excess deposit is refunded automatically

## All Proposal Actions

### Oracle Management

**add_oracle** — Add an oracle account that can report prices.
```json
{"action": {"action": "add_oracle", "account_id": "oracle1.near"}}
```

**remove_oracle** — Remove an oracle account.
```json
{"action": {"action": "remove_oracle", "account_id": "oracle1.near"}}
```

### Asset Management

**add_asset** — Add a new price-tracked asset. `push_signer_key` is the TEE secret name
for signing price push transactions. Use `null` for warm-only (fetch but don't push).
```json
{"action": {"action": "add_asset", "asset_id": "wrap.near", "push_signer_key": "PROTECTED_KEY_NEAR"}}
```

**remove_asset** — Remove an asset from tracking.
```json
{"action": {"action": "remove_asset", "asset_id": "wrap.near"}}
```

**add_asset_ema** — Add an EMA (exponential moving average) for an asset.
```json
{"action": {"action": "add_asset_ema", "asset_id": "wrap.near", "period_sec": 3600}}
```

**remove_asset_ema** — Remove an EMA period for an asset.
```json
{"action": {"action": "remove_asset_ema", "asset_id": "wrap.near", "period_sec": 3600}}
```

**set_push_signer_accounts** — Set which accounts can push prices for an asset.
`null` to clear (use default).
```json
{"action": {"action": "set_push_signer_accounts", "asset_id": "wrap.near", "push_signer_accounts": ["signer1.near"]}}
```

### Exchange Configs

Exchange configs are opaque JSON strings — the contract stores them but only WASI parses them.
Supported fields: `decimals`, `stablecoin`, `binance_us`, `huobi`, `cryptocom`, `kucoin`, `gate`, `pyth`, `binance_alpha`.

**set_asset_exchange_config** — Set config for one asset.
```json
{"action": {"action": "set_asset_exchange_config", "asset_id": "wrap.near", "config": "{\"decimals\":24,\"binance_us\":\"NEARUSD\",\"pyth\":\"0xc415...\"}"}}
```

**set_asset_exchange_configs** — Batch set configs for multiple assets.
```json
{"action": {"action": "set_asset_exchange_configs", "configs": [
  ["wrap.near", "{\"decimals\":24,\"binance_us\":\"NEARUSD\",\"pyth\":\"0xc415...\"}"],
  ["aurora", "{\"decimals\":18,\"binance_us\":\"ETHUSD\",\"pyth\":\"0xff61...\"}"]
]}}
```

**remove_asset_exchange_config** — Remove exchange config for an asset.
```json
{"action": {"action": "remove_asset_exchange_config", "asset_id": "wrap.near"}}
```

After updating exchange configs, call `sync_asset_configs` to push changes to WASI storage:
```bash
near call <CONTRACT> sync_asset_configs '{}' --deposit 0.05 --accountId <OWNER> --gas 300000000000000
```

### TEE Push Signers

**set_push_signer_keys** — Batch set/remove push signer keys.
Each entry: `[asset_id, key_name]` where `key_name` is `null` for warm-only.
```json
{"action": {"action": "set_push_signer_keys", "keys": [
  ["wrap.near", "PROTECTED_KEY_NEAR"],
  ["aurora", "PROTECTED_KEY_ETH"],
  ["token.rhealab.near", null]
]}}
```

**register_push_signer** — Register a TEE push signer (usually created via `propose_register_push_signer`).
```json
{"action": {"action": "register_push_signer", "push_signer_key": "PROTECTED_KEY_RHEA", "signer_account_id": "abc123...def", "asset_ids": ["token.rhealab.near"]}}
```

### Config

**set_recency_duration_sec** — Set how long prices are considered fresh.
```json
{"action": {"action": "set_recency_duration_sec", "recency_duration_sec": 300}}
```

**configure_outlayer** — Configure OutLayer integration (contract ID, code source, secrets).
```json
{"action": {"action": "configure_outlayer", "outlayer_contract_id": "outlayer.near", "code_source": "wasm_url:https://...", "secrets_profile": "oracle", "secrets_account_id": "price-oracle.near"}}
```

**set_subsidize_outlayer_calls** — Enable/disable subsidized OutLayer calls.
```json
{"action": {"action": "set_subsidize_outlayer_calls", "enabled": true}}
```

**update_near_claim_amount** — Set NEAR amount for claims.
```json
{"action": {"action": "update_near_claim_amount", "near_claim_amount": "50000000000000000000000"}}
```

### Pyth Config

**add_price_mapping** — Map a Pyth price feed ID to an asset.
```json
{"action": {"action": "add_price_mapping", "price_id_hex": "0xc415de8d...", "asset_id": "wrap.near"}}
```

**remove_price_mapping** — Remove a Pyth price feed mapping.
```json
{"action": {"action": "remove_price_mapping", "price_id_hex": "0xc415de8d..."}}
```

**set_pyth_stale_threshold** — Set how old Pyth prices can be before rejection.
```json
{"action": {"action": "set_pyth_stale_threshold", "threshold_sec": 120}}
```

### Council Governance

**add_council_member** — Add a council member.
```json
{"action": {"action": "add_council_member", "account_id": "new-member.near"}}
```

**remove_council_member** — Remove a council member (cannot remove last member).
```json
{"action": {"action": "remove_council_member", "account_id": "old-member.near"}}
```

### Owner & Contract

**update_owner** — Transfer ownership.
```json
{"action": {"action": "update_owner", "owner_id": "new-owner.near"}}
```

**upgrade_contract** — Deploy new contract code. Upload code first via `upload_upgrade_code`.
```json
{"action": {"action": "upgrade_contract", "code_hash": "abc123...", "migrate_method": "migrate_state2"}}
```

### Pause/Unpause

```json
{"action": {"action": "pause"}}
{"action": {"action": "unpause"}}
```

## View Methods

```bash
# List all proposals
near view <CONTRACT> get_proposals '{}'

# Get specific proposal
near view <CONTRACT> get_proposal '{"id": 7}'

# Get council members
near view <CONTRACT> get_council_members '{}'

# Get voting threshold
near view <CONTRACT> get_council_threshold '{}'

# Get owner
near view <CONTRACT> get_owner_id '{}'

# Get exchange configs
near view <CONTRACT> get_asset_exchange_configs '{}'
near view <CONTRACT> get_asset_exchange_config '{"asset_id": "wrap.near"}'
```

## Multiple Proposals

Use `create_proposals` (plural) to batch multiple actions in one transaction:

```json
{"actions": [
  {"action": "add_oracle", "account_id": "oracle1.near"},
  {"action": "add_oracle", "account_id": "oracle2.near"}
]}
```
