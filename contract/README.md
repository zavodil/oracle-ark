# Oracle Example Contract

TEE-secured price oracle that **recreates the interface** of the original [NEAR Native Price Oracle](https://github.com/NearDeFi/price-oracle) (`priceoracle.near`) with enhancements:
- TEE-verified prices via [OutLayer](https://app.outlayer.ai) (Intel TDX)
- Custom data fetching from any HTTP API
- On-demand pricing with automatic fallback

**Drop-in replacement:** Existing integrations with `priceoracle.near` work without code changes — just update the contract address.

**Contract addresses:**
- Mainnet: `price-oracle.near`
- Testnet: `price-oracle.testnet`

## How It Works

Prices are fetched from up to 10 sources (CoinGecko, Binance, Binance US, Binance Alpha, Pyth, Chainlink, Huobi, KuCoin, Gate.io, Crypto.com) inside a TEE (Intel TDX) and aggregated using median. When cached prices become stale, an OutLayer WASI call automatically fetches fresh data.

## Methods Preserved from NEAR Native Oracle

These methods are fully backward-compatible with `priceoracle.near`:

| Method | Type | Description |
|--------|------|-------------|
| `get_price_data` | view | Get cached median prices for whitelisted assets |
| `get_oracle_price_data` | view | Get prices from a specific oracle |
| `oracle_call` | call | Get prices with callback to receiver contract |
| `report_prices` | call | Submit prices (registered oracles only) |
| `get_oracle` / `get_oracles` | view | Oracle info and listing |
| `get_asset` / `get_assets` | view | Asset info and listing |
| EMA queries (`asset#period`) | view | Exponential moving average prices |

Existing integrations with `priceoracle.near` work without code changes (only update the contract address).

## New Methods (Oracle Example Only)

| Method | Type | Description |
|--------|------|-------------|
| `request_price_data` | call, payable | Get prices directly (no callback). Checks cache, fetches from OutLayer if stale |
| `request_custom_data` | call, payable | Fetch custom data directly (no callback). Any external source |
| `oracle_call` | call, payable | Extended with automatic OutLayer fallback when prices are stale |
| `custom_call` | call, payable | Fetch custom data with callback to receiver |
| `can_subsidize_outlayer_calls` | view | Check if contract will pay for OutLayer calls |

## Governance (DAO / council)

All state-changing operations go through a **council** (DAO) — there are no direct
owner-only setters. A council member calls `create_proposal '{"action": {...}}'`; the
proposal auto-executes once the approval threshold (>50% of members) is met (a
single-member council self-executes).

| Proposal action | Description |
|-----------------|-------------|
| `AddOracle` / `RemoveOracle` | Manage registered oracles |
| `AddAsset` / `RemoveAsset` | Manage tracked assets (optional `push_signer_key`) |
| `AddAssetEma` / `RemoveAssetEma` | Manage EMA calculations |
| `ConfigureOutlayer` | Set OutLayer contract, code source, secrets |
| `SetRecencyDurationSec` | Set max age for "fresh" prices |
| `SetSubsidizeOutlayerCalls` | Enable/disable contract-paid OutLayer calls |
| `UpdateNearClaimAmount` | Set oracle NEAR reward |
| `AddPriceMapping` / `RemovePriceMapping` / `SetPythStaleThreshold` | Pyth feed config |
| `RegisterPushSigner` / `SetPushSignerAccounts` | TEE push-signer management |
| `SetAssetExchangeConfig(s)` / `RemoveAssetExchangeConfig` | Per-asset source config |
| `UpdateOwner` | Transfer ownership |
| `UpgradeContract` | Deploy new code (after `upload_upgrade_code`) |
| `Pause` / `Unpause` | Halt / resume price operations |

Bootstrap helpers that are not proposals: `set_council` (initial council setup) and
`upload_upgrade_code` (stage upgrade bytes before an `UpgradeContract` proposal).

## Pyth-compatible methods (native)

The main contract implements the Pyth receiver API directly (see [src/pyth.rs](src/pyth.rs)),
so the standalone wrapper contract is no longer required: `get_price`,
`get_price_unsafe`, `get_price_no_older_than`, `get_ema_price` (+ `_unsafe` /
`_no_older_than`), `list_prices` (+ variants), `price_feed_exists`,
`get_stale_threshold`, `get_update_fee_estimate`, `get_price_mapping`,
`get_all_price_mappings`. Default staleness threshold is 60s.

## Build

```bash
cargo near build
```

## Documentation

- [sdk.md](../sdk.md) - Full SDK reference
- [integration.md](../integration.md) - Integration guide (user-facing methods)
