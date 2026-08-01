# Pyth-Compatible Wrapper for Oracle Example

> **Legacy / optional.** This standalone wrapper contract is now legacy. The main Oracle Example contract `price-oracle.near` implements the Pyth receiver API **natively** (see `../contract/src/pyth.rs`). **Use the native Pyth methods on `price-oracle.near` unless you specifically need a separate contract address.**
>
> One behavioral difference: the native contract's `get_ema_price` returns a real EMA, whereas **this** wrapper's `get_ema_price` just returns the spot price (see this crate's `src/lib.rs`). Native Pyth price-feed mappings are managed via council actions `AddPriceMapping` / `RemovePriceMapping` / `SetPythStaleThreshold`.

NEAR smart contract that implements the [Pyth receiver contract](https://github.com/pyth-network/pyth-crosschain/tree/main/target_chains/near/receiver) API, but internally uses [Oracle Example](https://github.com/out-layer/oracle-example/tree/main/contract) (`price-oracle.near`) for price data.

DeFi protocols currently using Pyth can switch to Oracle Example with **zero code changes** — just update the contract address.

## Build

```bash
cargo near build
```

## Deploy & Initialize

```bash
# Deploy with initialization
near contract deploy price-oracle-pyth.testnet \
  use-file res/pyth_compatible_wrapper.wasm \
  with-init-call new \
  json-args '{"oracle_contract_id": "price-oracle.testnet", "stale_threshold": 60}' \
  prepaid-gas '10 Tgas' attached-deposit '0 NEAR' \
  network-config testnet sign-with-keychain send
```

Parameters:
- `oracle_contract_id` — Oracle Example contract to read prices from
- `stale_threshold` — max age of prices in seconds (60 = prices older than 60s return `null`)

The contract initializes with default mainnet price feed mappings (NEAR, ETH, BTC, USDT, USDC). You can add/remove mappings after deployment.

## Fund the Contract

The contract pays 0.02 NEAR per oracle refresh call from its own balance:

```bash
near send OWNER price-oracle-pyth.testnet 1 --networkId testnet
```

## Configure Price Mappings

Link Pyth price feed IDs to Oracle Example asset IDs:

```bash
# NEAR/USD
near call price-oracle-pyth.testnet add_price_mapping '{
  "price_id_hex": "c415de8d2efa7db216527dff4b60e8f3a5311c740dadb233e13e12547e226750",
  "asset_id": "wrap.near"
}' --accountId price-oracle-pyth.testnet --depositYocto 1 --gas 10000000000000 --networkId testnet

# ETH/USD
near call price-oracle-pyth.testnet add_price_mapping '{
  "price_id_hex": "ff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace",
  "asset_id": "aurora"
}' --accountId price-oracle-pyth.testnet --depositYocto 1 --gas 10000000000000 --networkId testnet

# BTC/USD
near call price-oracle-pyth.testnet add_price_mapping '{
  "price_id_hex": "e62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43",
  "asset_id": "nbtc.bridge.near"
}' --accountId OWNER --depositYocto 1 --gas 10000000000000 --networkId testnet

# USDT/USD
near call price-oracle-pyth.testnet add_price_mapping '{
  "price_id_hex": "2b89b9dc8fdf9f34709a5b106b472f0f39bb6ca9ce04b0fd7f2e971688e2e53b",
  "asset_id": "usdt.tether-token.near"
}' --accountId OWNER --depositYocto 1 --gas 10000000000000 --networkId testnet

# USDC/USD
near call price-oracle-pyth.testnet add_price_mapping '{
  "price_id_hex": "eaa020c61cc479712813461ce153894a96a6c00b21ed0cfc2798d1f9a9e9c94a",
  "asset_id": "17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1"
}' --accountId OWNER --depositYocto 1 --gas 10000000000000 --networkId testnet
```

Remove a mapping:

```bash
near call price-oracle-pyth.testnet remove_price_mapping '{
  "price_id_hex": "c415de8d2efa7db216527dff4b60e8f3a5311c740dadb233e13e12547e226750"
}' --accountId OWNER --depositYocto 1 --gas 10000000000000 --networkId testnet
```

## Refresh Prices

Trigger Oracle Example to fetch fresh prices and send them to the wrapper via `oracle_on_call` callback:

```bash
near call price-oracle-pyth.testnet refresh_prices '{}' \
  --accountId zavodil2.testnet --deposit 0.02 --gas 300000000000000 --networkId testnet
```

## Read Prices (View Calls — Free)

These methods are identical to the Pyth receiver contract API:

```bash
# Get price with staleness check (returns null if stale)
near view price-oracle-pyth.testnet get_price '{
  "price_identifier": "c415de8d2efa7db216527dff4b60e8f3a5311c740dadb233e13e12547e226750"
}' --networkId testnet

# Get price without staleness check (may return old data)
near view price-oracle-pyth.testnet get_price_unsafe '{
  "price_identifier": "c415de8d2efa7db216527dff4b60e8f3a5311c740dadb233e13e12547e226750"
}' --networkId testnet

# Get price only if published within 30 seconds
near view price-oracle-pyth.testnet get_price_no_older_than '{
  "price_id": "c415de8d2efa7db216527dff4b60e8f3a5311c740dadb233e13e12547e226750",
  "age": 30
}' --networkId testnet

# EMA price (same as get_price — Oracle Example has no separate EMA)
near view price-oracle-pyth.testnet get_ema_price '{
  "price_id": "ff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace"
}' --networkId testnet

# Check if a feed exists
near view price-oracle-pyth.testnet price_feed_exists '{
  "price_identifier": "c415de8d2efa7db216527dff4b60e8f3a5311c740dadb233e13e12547e226750"
}' --networkId testnet

# Batch: multiple feeds at once
near view price-oracle-pyth.testnet list_prices '{
  "price_ids": [
    "c415de8d2efa7db216527dff4b60e8f3a5311c740dadb233e13e12547e226750",
    "ff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace"
  ]
}' --networkId testnet

# Get staleness threshold
near view price-oracle-pyth.testnet get_stale_threshold --networkId testnet
```

### Response Format

```json
{
  "price": 450000000,
  "conf": 0,
  "expo": -8,
  "publish_time": 1706900000
}
```

Actual USD price = `price * 10^expo` = `450000000 * 10^(-8)` = **$4.50**

`conf` is always 0 (Oracle Example provides a single aggregated price, no confidence interval).

## Pyth-Compatible Mutating Methods

These methods exist for compatibility with protocols that call them as part of the standard Pyth flow:

```bash
# update_price_feeds — triggers a refresh (ignores Wormhole VAA data)
near call price-oracle-pyth.testnet update_price_feeds '{"data": ""}' \
  --accountId ANYONE --deposit 0.02 --gas 300000000000000 --networkId testnet

# get_update_fee_estimate — returns 1 yoctoNEAR (effectively free)
near view price-oracle-pyth.testnet get_update_fee_estimate '{"data": ""}' --networkId testnet
```

## Introspection

```bash
# View all configured mappings
near view price-oracle-pyth.testnet get_all_mappings --networkId testnet

# Look up a specific mapping
near view price-oracle-pyth.testnet get_price_mapping '{
  "price_id_hex": "c415de8d2efa7db216527dff4b60e8f3a5311c740dadb233e13e12547e226750"
}' --networkId testnet

# View oracle contract ID
near view price-oracle-pyth.testnet get_oracle_contract_id --networkId testnet

# View owner
near view price-oracle-pyth.testnet get_owner --networkId testnet
```

## Admin Methods

All require owner account + 1 yoctoNEAR deposit:

```bash
# Change staleness threshold
near call price-oracle-pyth.testnet set_stale_threshold '{"threshold_sec": 120}' \
  --accountId OWNER --depositYocto 1 --gas 10000000000000 --networkId testnet

# Change oracle contract
near call price-oracle-pyth.testnet set_oracle_contract_id '{"contract_id": "new-oracle.near"}' \
  --accountId OWNER --depositYocto 1 --gas 10000000000000 --networkId testnet
```

## Migration Guide for DeFi Protocols

For a protocol currently using `pyth-oracle.near`:

**1. Change the contract address:**

```rust
// Before:
const PYTH_CONTRACT: &str = "pyth-oracle.near";
// After:
const PYTH_CONTRACT: &str = "price-oracle-pyth.testnet";
```

**2. View methods — no changes needed.**

`get_price`, `get_price_unsafe`, `get_price_no_older_than`, `get_ema_price`, `list_prices`, etc. have identical signatures and return the same `Price` type.

**3. `update_price_feeds` — deposit difference.**

In real Pyth, `update_price_feeds` requires a deposit to cover Wormhole VAA verification (typically ~1-2 yoctoNEAR per feed, queried via `get_update_fee_estimate`).

In this wrapper, `update_price_feeds` triggers an OutLayer oracle call which costs **0.02 NEAR**. The wrapper contract pays this from its own balance, so the caller's attached deposit is not strictly required — but the wrapper contract must be funded.

If your protocol attaches the Pyth fee estimate before calling `update_price_feeds`, it will still work (the small deposit goes to the wrapper's balance). However, if your protocol does **not** call `update_price_feeds` at all, prices are still updated via the scheduler or manual `refresh_prices` calls — so you can safely remove `update_price_feeds` calls from your flow.

**4. Price freshness model is different.**

| | Pyth | Oracle Example Wrapper |
|---|---|---|
| Price source | Wormhole VAA push from off-chain | OutLayer WASI fetches from exchanges |
| Update trigger | Caller must push VAA via `update_price_feeds` | Scheduler / anyone calls `refresh_prices` |
| Update cost | Caller pays Wormhole fee | Wrapper contract pays 0.02 NEAR per refresh |
| `conf` (confidence) | Non-zero (reflects source spread) | Always 0 (single aggregated price) |

**5. Fund the wrapper contract.**

The wrapper needs NEAR balance to pay for oracle calls. Send NEAR to the wrapper contract so it can call Oracle Example:

```bash
near send FUNDER price-oracle-pyth.testnet 5 --networkId testnet
```

Each `refresh_prices` / `update_price_feeds` call costs ~0.02 NEAR from the wrapper's balance.

## Known Price Feed IDs

| Asset    | Pyth Price ID                                                      | Oracle Example asset_id                                        |
|----------|--------------------------------------------------------------------|------------------------------------------------------------|
| NEAR/USD | `c415de8d2efa7db216527dff4b60e8f3a5311c740dadb233e13e12547e226750` | `wrap.near`                                                |
| ETH/USD  | `ff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace` | `aurora`                                                   |
| BTC/USD  | `e62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43` | `nbtc.bridge.near`                                         |
| USDT/USD | `2b89b9dc8fdf9f34709a5b106b472f0f39bb6ca9ce04b0fd7f2e971688e2e53b` | `usdt.tether-token.near`                                   |
| USDC/USD | `eaa020c61cc479712813461ce153894a96a6c00b21ed0cfc2798d1f9a9e9c94a` | `17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1` |

Full Pyth price feed IDs list: https://www.pyth.network/developers/price-feed-ids
