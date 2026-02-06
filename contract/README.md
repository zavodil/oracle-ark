# Oracle-Ark Contract

TEE-secured price oracle that **recreates the interface** of the original [NEAR Native Price Oracle](https://github.com/NearDeFi/price-oracle) (`priceoracle.near`) with enhancements:
- TEE-verified prices via [OutLayer](https://outlayer.fastnear.com) (Intel TDX)
- Custom data fetching from any HTTP API
- On-demand pricing with automatic fallback

**Drop-in replacement:** Existing integrations with `priceoracle.near` work without code changes — just update the contract address.

**Contract addresses:**
- Mainnet: `price-oracle.near`
- Testnet: `price-oracle.testnet`

## How It Works

Prices are fetched from multiple sources (CoinGecko, Binance, Pyth) inside a TEE (Intel TDX) and aggregated using median. When cached prices become stale, an OutLayer WASI call automatically fetches fresh data.

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

## New Methods (Oracle-Ark Only)

| Method | Type | Description |
|--------|------|-------------|
| `request_price_data` | call, payable | Get prices directly (no callback). Checks cache, fetches from OutLayer if stale |
| `request_custom_data` | call, payable | Fetch custom data directly (no callback). Any external source |
| `oracle_call` | call, payable | Extended with automatic OutLayer fallback when prices are stale |
| `custom_call` | call, payable | Fetch custom data with callback to receiver |
| `can_subsidize_outlayer_calls` | view | Check if contract will pay for OutLayer calls |

## Admin Methods

| Method | Description |
|--------|-------------|
| `configure_outlayer` | Set OutLayer contract, code source, secrets |
| `add_oracle` / `remove_oracle` | Manage registered oracles |
| `add_asset` / `remove_asset` | Manage tracked assets |
| `add_asset_ema` / `remove_asset_ema` | Manage EMA calculations |
| `set_recency_duration_sec` | Set max age for "fresh" prices |
| `set_subsidize_outlayer_calls` | Enable/disable contract-paid OutLayer calls |
| `update_near_claim_amount` | Set oracle NEAR reward |
| `update_owner_id` | Transfer ownership |

## Build

```bash
cargo near build
```

## Documentation

- [sdk.md](../sdk.md) - Full SDK reference
- [integration.md](../integration.md) - Integration guide (user-facing methods)
