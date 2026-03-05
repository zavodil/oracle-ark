# Mainnet Initialization Commands

Contract: `price-oracle.near`
Owner: `owner.price-oracle.near`

---

## 1. Build

```bash
cd wasi-examples/oracle-ark/contract
cargo near build
```

## 2. Upgrade + migrate (last owner upgrade)

```bash
WASM_BASE64=$(base64 -i target/near/price_oracle.wasm)
near call price-oracle.near upgrade \
  --base64 "$WASM_BASE64" \
  --accountId owner.price-oracle.near \
  --gas 300000000000000 \
  --networkId mainnet
```

## 3. Verify

```bash
near view price-oracle.near get_version --networkId mainnet
```

## 4. Set council

```bash
near call price-oracle.near set_council '{
  "members": ["owner.price-oracle.near"]
}' --accountId owner.price-oracle.near --depositYocto 1 --networkId mainnet
```

## 5. Configure OutLayer

```bash
near call price-oracle.near create_proposal '{"action": {
  "action": "configure_outlayer",
  "outlayer_contract_id": "outlayer.near",
  "code_source": "{\"Project\":{\"project_id\":\"price-oracle.near/price-oracle\"}}",
  "secrets_profile": "default",
  "secrets_account_id": "price-oracle.near"
}}' --accountId owner.price-oracle.near --depositYocto 1 --networkId mainnet
```

## 6. Add all assets

```bash
near call price-oracle.near create_proposals '{"actions": [
  {"action": "add_asset", "asset_id": "xrp.omft.near", "push_signer_key": null},
  {"action": "add_asset", "asset_id": "doge.omft.near", "push_signer_key": null},
  {"action": "add_asset", "asset_id": "cardano.omft.near", "push_signer_key": null},
  {"action": "add_asset", "asset_id": "xlm", "push_signer_key": null},
  {"action": "add_asset", "asset_id": "ltc.omft.near", "push_signer_key": null}
]}' --accountId owner.price-oracle.near --depositYocto 1 --networkId mainnet
```

## 7. Add EMAs (wrap.near)

```bash
near call price-oracle.near create_proposals '{"actions": [
  {"action": "add_asset_ema", "asset_id": "wrap.near", "period_sec": 3600},
  {"action": "add_asset_ema", "asset_id": "wrap.near", "period_sec": 86400}
]}' --accountId owner.price-oracle.near --depositYocto 1 --networkId mainnet
```

# UPLOAD WASM TO FASTFS

python3 upload_wasm_fastfs.py ../wasi-examples/oracle-ark/oracle-ark.wasm ../worker/.env.mainnet.worker1

## 8. Register TEE push signer (PROTECTED_KEY_RHEA)

```bash
near call price-oracle.near propose_register_push_signer '{
  "push_signer_key": "PROTECTED_KEY_RHEA",
  "asset_ids": [
    "wrap.near",
    "aurora",
    "usdt.tether-token.near",
    "17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1",
    "nbtc.bridge.near",
    "2260fac5e5542a773aa44fbcfedf7c193bc2c599.factory.bridge.near",
    "6b175474e89094c44da98b954eedeac495271d0f.factory.bridge.near",
    "aaaaaa20d9e0e2461697782ef11675f668207961.factory.bridge.near",
    "4691937a7508860f876c9c0a2a617e7d9e945d4b.factory.bridge.near",
    "853d955acef822db058eb8505911ed77f175b99e.factory.bridge.near",
    "22.contract.portalbridge.near",
    "zec.omft.near",
    "token.rhealab.near"
  ],
  "secrets_profile": "default",
  "secrets_account_id": "price-oracle.near"
}' --accountId owner.price-oracle.near --depositYocto 1 --gas 300000000000000 --networkId mainnet
```

## 9. Check the proposal created by callback

```bash
near view price-oracle.near get_proposals '{"limit": 5}' --networkId mainnet
```

## 10. Fund the implicit account

```bash
# Get the implicit account ID from the proposal
near view price-oracle.near get_push_signer_accounts '{"asset_id": "wrap.near"}' --networkId mainnet

# Fund it (needs NEAR for gas to send report_prices transactions)
near send owner.price-oracle.near <implicit_account_id> 0.1 --networkId mainnet
```

## 11. Enable subsidized OutLayer calls

```bash
near call price-oracle.near create_proposal '{"action": {
  "action": "set_subsidize_outlayer_calls",
  "enabled": true
}}' --accountId owner.price-oracle.near --depositYocto 1 --networkId mainnet
```

## 12. Verify everything

```bash
near view price-oracle.near get_version --networkId mainnet
near view price-oracle.near get_council_members --networkId mainnet
near view price-oracle.near get_assets --networkId mainnet
near view price-oracle.near get_asset_oracle_keys --networkId mainnet
near view price-oracle.near get_oracles --networkId mainnet
near view price-oracle.near can_subsidize_outlayer_calls --networkId mainnet
```
