# Pyth-Compatible Wrapper Contract — Technical Specification

> **Legacy / optional.** This standalone wrapper contract is now legacy. The main Oracle-Ark contract `price-oracle.near` implements the Pyth receiver API **natively** (see `../contract/src/pyth.rs`). **Use the native Pyth methods on `price-oracle.near` unless you specifically need a separate contract address.**
>
> One behavioral difference: the native contract's `get_ema_price` returns a real EMA, whereas **this** wrapper's `get_ema_price` just returns the spot price (see this crate's `src/lib.rs`). Native Pyth price-feed mappings are managed via council actions `AddPriceMapping` / `RemovePriceMapping` / `SetPythStaleThreshold`.

NEAR smart contract that implements the same public API as the Pyth receiver contract (`pyth-oracle.near`), but internally uses Oracle-Ark (`price-oracle.near`) for price data.

This enables existing DeFi protocols using Pyth to switch to Oracle-Ark with **minimal code changes** — update the contract address and adjust the deposit model (see [Migration Guide](#migration-guide-for-defi-protocols)).

## Pyth Receiver Contract Interface (source of truth)

Reference: [pyth-crosschain/target_chains/near/receiver/src/ext.rs](https://github.com/pyth-network/pyth-crosschain/blob/main/target_chains/near/receiver/src/ext.rs)

### Data Types

```rust
/// 32-byte identifier for a price feed (hex string without 0x prefix on NEAR)
/// Example BTC/USD: "f9c0172ba10dfa4d19088d94f5bf61d3b54d5bd7483a322a982e1373ee8ea31b"
pub struct PriceIdentifier(pub [u8; 32]);

/// Price with uncertainty
pub struct Price {
    pub price: i64,           // Price value (signed!)
    pub conf: u64,            // Confidence interval (uncertainty)
    pub expo: i32,            // Exponent: actual_price = price * 10^expo
    pub publish_time: i64,    // Unix timestamp of last publish (UnixTimestamp = i64)
}
```

Example: BTC at $67,123.45 ± $12.50:
```
Price { price: 6712345, conf: 1250, expo: -2, publish_time: 1706900000 }
```

### Methods to Implement

#### View Methods (free, no gas deposit)

```rust
/// Get latest price. Returns None if price is stale (older than stale_threshold).
fn get_price(&self, price_identifier: PriceIdentifier) -> Option<Price>;

/// Get latest price WITHOUT staleness check. May return very old data.
fn get_price_unsafe(&self, price_identifier: PriceIdentifier) -> Option<Price>;

/// Get latest price only if published within `age` seconds.
fn get_price_no_older_than(&self, price_id: PriceIdentifier, age: u64) -> Option<Price>;

/// Get exponential moving average price. Staleness checked.
fn get_ema_price(&self, price_id: PriceIdentifier) -> Option<Price>;

/// Get EMA price without staleness check.
fn get_ema_price_unsafe(&self, price_id: PriceIdentifier) -> Option<Price>;

/// Get EMA price only if published within `age` seconds.
fn get_ema_price_no_older_than(&self, price_id: PriceIdentifier, age: u64) -> Option<Price>;

/// Check if a price feed exists.
fn price_feed_exists(&self, price_identifier: PriceIdentifier) -> bool;

/// Get staleness threshold in seconds.
fn get_stale_threshold(&self) -> u64;

/// Batch: get prices for multiple feeds.
fn list_prices(&self, price_ids: Vec<PriceIdentifier>) -> HashMap<PriceIdentifier, Option<Price>>;

/// Batch: get prices without staleness check.
fn list_prices_unsafe(&self, price_ids: Vec<PriceIdentifier>) -> HashMap<PriceIdentifier, Option<Price>>;

/// Batch: get prices no older than stale_threshold.
fn list_prices_no_older_than(&self, price_ids: Vec<PriceIdentifier>) -> HashMap<PriceIdentifier, Option<Price>>;
```

#### Mutating Methods

```rust
/// Update price feeds. In Pyth this accepts Wormhole VAA data.
/// In our wrapper, this is a NO-OP or triggers an oracle-ark update.
pub fn update_price_feeds(&mut self, _data: String);

/// Estimate fee for update. In Pyth this returns deposit needed for update_price_feeds.
fn get_update_fee_estimate(&self, data: String) -> U128;
```

## Implementation Plan

### Contract State

```rust
pub struct PythWrapper {
    /// Oracle-Ark contract to read prices from
    oracle_contract_id: AccountId,

    /// Mapping: Pyth PriceIdentifier (hex) -> Oracle-Ark asset_id
    /// Example: "f9c0172ba10d..." -> "wrap.near"
    price_id_to_asset: UnorderedMap<String, String>,

    /// Reverse mapping: Oracle-Ark asset_id -> Pyth PriceIdentifier (hex)
    asset_to_price_id: UnorderedMap<String, String>,

    /// Staleness threshold in seconds (matches Pyth default behavior)
    stale_threshold: u64,

    /// Contract owner
    owner_id: AccountId,
}
```

### Price Conversion Logic

Oracle-Ark price format:
```
Price { multiplier: 500000000, decimals: 8 }  // = $5.00
```

Pyth price format:
```
Price { price: 500000000, conf: 0, expo: -8, publish_time: 1706900000 }
```

Conversion:
```rust
fn oracle_ark_to_pyth(multiplier: u128, decimals: u8, timestamp: u64) -> pyth::Price {
    pyth::Price {
        price: multiplier as i64,    // Oracle-Ark prices are always positive
        conf: 0,                      // No confidence data from Oracle-Ark (single aggregated price)
        expo: -(decimals as i32),     // decimals=8 -> expo=-8
        publish_time: (timestamp / 1_000_000_000) as i64,  // nano -> seconds (if needed)
    }
}
```

### Method Implementation

**View methods** (`get_price`, `get_price_unsafe`, `get_price_no_older_than`, etc.):
1. Look up `price_id_to_asset` to find the Oracle-Ark asset_id
2. Call `get_price_data` (view) on Oracle-Ark contract to get cached price
3. Convert Oracle-Ark `Price` -> Pyth `Price`
4. Apply staleness check if needed

**Important**: View methods can only call other view methods. `get_price_data` on Oracle-Ark IS a view method, so this works. But NEAR view calls cannot do cross-contract view calls (no `ext_contract` in view context). Therefore:
- **Option A**: Replicate prices into wrapper state via scheduler/callback, then serve from local state (preferred — instant, no cross-contract)
- **Option B**: Read from Oracle-Ark public storage directly (via HTTPS from frontend, not from contract)

**Recommended: Option A** — Wrapper stores its own price cache, updated via callback from Oracle-Ark.

### Flow: How Prices Get Into the Wrapper

```
Oracle-Ark contract (price-oracle.near)
    │
    │  oracle_call(receiver_id=price-oracle-pyth.near, asset_ids=["wrap.near", ...])
    │
    ▼
PythWrapper.oracle_on_call(data: PriceData)
    │
    │  Convert prices to Pyth format
    │  Store in local UnorderedMap<PriceIdentifier, PythPrice>
    │
    ▼
DeFi protocol calls price-oracle-pyth.near
    │
    │  get_price(price_identifier) → reads from local cache → returns Pyth Price
    │
    ▼
Zero code changes needed in DeFi protocol
```

Update can be triggered by:
1. **Scheduler** (same as Oracle-Ark scheduler) calling `refresh_prices()` on wrapper
2. **Anyone** calling `refresh_prices()` (wrapper calls oracle_call internally, pays from its balance)
3. **Oracle-Ark contract** calling `oracle_on_call` as a callback

### Admin Methods

```rust
/// Add mapping: Pyth price_id <-> Oracle-Ark asset_id
fn add_price_mapping(&mut self, price_id_hex: String, asset_id: String);

/// Remove mapping
fn remove_price_mapping(&mut self, price_id_hex: String);

/// Set staleness threshold
fn set_stale_threshold(&mut self, threshold_sec: u64);

/// Set oracle contract ID
fn set_oracle_contract_id(&mut self, contract_id: AccountId);

/// Trigger a price refresh from Oracle-Ark
fn refresh_prices(&mut self) -> Promise;
```

### `update_price_feeds` Behavior

In real Pyth, this method accepts Wormhole VAA data and updates prices on-chain. In our wrapper:
- Accept the call (don't reject it — protocols may call it as part of their flow)
- Ignore the `data` parameter (we don't parse Wormhole VAAs)
- Optionally trigger a `refresh_prices()` from Oracle-Ark
- Return success

This ensures protocols that call `update_price_feeds` before `get_price` (standard Pyth pattern) still work.

### `get_update_fee_estimate` Behavior

In real Pyth, returns the deposit needed for `update_price_feeds`. In our wrapper:
- Return `U128(0)` or a minimal amount (1 yoctoNEAR)
- Oracle-Ark prices are already on-chain, no expensive update needed

## Known Price Feed IDs

Common Pyth price feed IDs on NEAR (without `0x` prefix):

| Asset | Pyth Price ID | Oracle-Ark asset_id |
|-------|--------------|---------------------|
| BTC/USD | `e62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43` | `nbtc.bridge.near` |
| ETH/USD | `ff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace` | `aurora` |
| NEAR/USD | `c415de8d2efa7db216527dff4b60e8f3a5311c740dadb233e13e12547e226750` | `wrap.near` |
| USDT/USD | `2b89b9dc8fdf9f34709a5b106b472f0f39bb6ca9ce04b0fd7f2e971688e2e53b` | `usdt.tether-token.near` |
| USDC/USD | `eaa020c61cc479712813461ce153894a96a6c00b21ed0cfc2798d1f9a9e9c94a` | `17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1` |

Full list: https://www.pyth.network/developers/price-feed-ids

## File Structure

```
wasi-examples/oracle-ark/pyth-compatible-wrapper/
├── SPEC.md              (this file)
├── Cargo.toml
├── rust-toolchain.toml  (channel = "1.85.0")
├── build_local.sh
├── res/                 (compiled WASM)
├── src/
│   └── lib.rs           (contract code)
└── README.md            (usage guide)
```

## Dependencies

```toml
[dependencies]
near-sdk = "5.9.0"
schemars = "0.8"
serde_json = "1"
```

## Testing

```bash
# Build
cd pyth-compatible-wrapper && cargo near build

# Deploy
near contract deploy price-oracle-pyth.testnet \
  use-file res/pyth_compatible_wrapper.wasm \
  with-init-call new \
  json-args '{"oracle_contract_id": "price-oracle.testnet", "stale_threshold": 60}' \
  prepaid-gas '10 Tgas' attached-deposit '0 NEAR' \
  network-config testnet sign-with-keychain send

# Add price mappings
near call price-oracle-pyth.testnet add_price_mapping '{
  "price_id_hex": "c415de8d2efa7db216527dff4b60e8f3a5311c740dadb233e13e12547e226750",
  "asset_id": "wrap.near"
}' --accountId OWNER --depositYocto 1 --networkId testnet

# Fund wrapper for oracle calls
near send OWNER price-oracle-pyth.testnet 1

# Trigger price refresh
near call price-oracle-pyth.testnet refresh_prices '{}' --accountId ANYONE --deposit 0.02 --gas 300000000000000 --networkId testnet

# Read price (view, free) — same API as pyth-oracle.near
near view price-oracle-pyth.testnet get_price '{
  "price_identifier": "c415de8d2efa7db216527dff4b60e8f3a5311c740dadb233e13e12547e226750"
}' --networkId testnet
```

## Migration Guide for DeFi Protocols

For a protocol currently using `pyth-oracle.near`:

1. Deploy `price-oracle-pyth.near` (or use shared deployment)
2. In your contract, change the Pyth contract address:
   ```rust
   // Before:
   const PYTH_CONTRACT: &str = "pyth-oracle.near";
   // After:
   const PYTH_CONTRACT: &str = "price-oracle-pyth.near";
   ```
3. Keep `get_price(price_identifier)` calls as-is — same API, same types
4. Remove `update_price_feeds` calls if desired (prices are auto-updated via scheduler)
5. **Fund the wrapper contract** — it pays ~0.02 NEAR per `refresh_prices` / `update_price_feeds` call from its own balance to cover OutLayer execution

### Key Differences from Pyth

| | Pyth | Oracle-Ark Wrapper |
|---|---|---|
| Price source | Wormhole VAA push from off-chain | OutLayer WASI fetches from exchanges |
| Update trigger | Caller must push VAA via `update_price_feeds` | Scheduler / anyone calls `refresh_prices` |
| Update cost | Caller pays Wormhole fee (~1 yoctoNEAR per feed) | **Wrapper contract** pays ~0.02 NEAR per refresh from its own balance |
| `update_price_feeds` | Required before each `get_price` | Optional — prices updated by scheduler; call is accepted but triggers OutLayer refresh |
| `get_update_fee_estimate` | Returns actual Wormhole fee | Returns 1 yoctoNEAR (effectively free for caller) |
| `conf` (confidence) | Non-zero (reflects source spread) | Always 0 (single aggregated price) |

### View methods — fully compatible

`get_price`, `get_price_unsafe`, `get_price_no_older_than`, `get_ema_price`, `list_prices` — identical signatures and return the same `Price` type. No code changes needed.

### `update_price_feeds` — behavior change

In Pyth, the caller pays a small Wormhole fee and pushes fresh price data on-chain. In the wrapper, `update_price_feeds` accepts the call (so protocols that call it as part of their flow don't break), ignores the VAA data, and triggers an Oracle-Ark refresh. The ~0.02 NEAR cost is paid by the wrapper contract from its own balance, not by the caller.

## References

- Pyth NEAR receiver contract: https://github.com/pyth-network/pyth-crosschain/tree/main/target_chains/near/receiver
- Pyth ext.rs (method signatures): https://github.com/pyth-network/pyth-crosschain/blob/main/target_chains/near/receiver/src/ext.rs
- Pyth NEAR integration guide: https://docs.pyth.network/price-feeds/core/use-real-time-data/pull-integration/near
- Pyth contract addresses: https://docs.pyth.network/price-feeds/contract-addresses/near
- Pyth price feed IDs: https://www.pyth.network/developers/price-feed-ids
- pyth-sdk Rust types: https://docs.rs/pyth-sdk/0.8.0/pyth_sdk/
- NEAR oracles page: https://docs.near.org/primitives/oracles
