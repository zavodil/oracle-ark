# Oracle Example SDK

Integration guide for the Oracle Example price oracle contract on NEAR Protocol.

## Overview

Oracle Example provides verifiable price feeds for DeFi applications. Prices are fetched from multiple sources (CoinGecko, Binance, Pyth) inside a TEE (Trusted Execution Environment) and aggregated using median.

**Key features:**
- **TEE-verified prices** - All price fetching happens inside Intel TDX enclave
- **Multiple sources** - Median aggregation from CoinGecko, Binance, Pyth
- **Backward compatible** - API compatible with [NEAR Native Price Oracle](https://github.com/NearDeFi/price-oracle)
- **Auto-refresh** - Stale prices automatically refreshed via OutLayer WASI
- **Subsidized mode** - Contract can pay for OutLayer calls if funded

## Data Types

### Price

```rust
pub struct Price {
    pub multiplier: u128,  // Price value; serialized as a decimal STRING in JSON (e.g. "500000000")
    pub decimals: u8,      // Decimal places (e.g., 8 for USD)
}
```

**Example:** NEAR at $5.00 USD
```json
{ "multiplier": "500000000", "decimals": 8 }
```

**Converting to float:**
```javascript
const priceUsd = multiplier / Math.pow(10, decimals);
// 500000000 / 10^8 = 5.00
```

### PriceData

Response from `get_price_data` and `oracle_call` callback:

```rust
pub struct PriceData {
    pub timestamp: u64,              // Block timestamp in ns; serialized as a STRING in JSON
    pub recency_duration_sec: u32,   // Max age for fresh prices
    pub prices: Vec<AssetOptionalPrice>,
}

pub struct AssetOptionalPrice {
    pub asset_id: String,
    pub price: Option<Price>,  // None if stale/unavailable
}
```

### PriceSource (for external queries)

```rust
pub enum PriceSource {
    CoinGecko,
    Binance,
    Pyth,
    Custom(CustomSourceConfig),  // Fetch from any URL
}

pub struct CustomSourceConfig {
    pub url: String,           // HTTP URL to fetch
    pub json_path: String,     // Dot notation path (e.g., "data.price")
    pub value_type: String,    // "number", "string", "boolean"
    pub method: String,        // "GET" or "POST"
    pub headers: Vec<(String, String)>,  // Optional headers
}
```

## View Methods

These methods are free to call (no gas cost for caller).

### get_price_data

Get current prices for whitelisted assets.

```bash
near view price-oracle.testnet get_price_data '{"asset_ids": ["wrap.near", "usdt.tether-token.near"]}' --networkId testnet 
```

**Arguments:**
- `asset_ids` (optional): List of asset IDs. If omitted, returns all assets.

**Returns:** `PriceData`

**Response example:**
```json
{
  "timestamp": "1706889600000000000",
  "recency_duration_sec": 300,
  "prices": [
    {
      "asset_id": "wrap.near",
      "price": { "multiplier": "500000000", "decimals": 8 }
    },
    {
      "asset_id": "usdt.tether-token.near",
      "price": { "multiplier": "100000000", "decimals": 8 }
    }
  ]
}
```

> **Compatibility:** This method is compatible with [NEAR Native Price Oracle](https://github.com/NearDeFi/price-oracle).

### get_oracle_price_data

Get prices from a specific oracle (not median).

```bash
near view price-oracle.testnet get_oracle_price_data '{
  "account_id": "oracle1.near",
  "asset_ids": ["wrap.near"],
  "recency_duration_sec": 600
}' --networkId testnet 
```

**Arguments:**
- `account_id`: Oracle account ID
- `asset_ids` (optional): List of assets
- `recency_duration_sec` (optional): Override default recency

> **Compatibility:** From NEAR Native Price Oracle.

### get_asset

Get asset configuration and reports.

```bash
near view price-oracle.near get_asset '{"asset_id": "wrap.near"}'
```

### get_assets

List all registered assets with pagination.

```bash
near view price-oracle.near get_assets '{"from_index": 0, "limit": 10}'
```

### get_oracle

Get oracle information.

```bash
near view price-oracle.near get_oracle '{"account_id": "oracle1.near"}'
```

### get_oracles

List all registered oracles.

```bash
near view price-oracle.near get_oracles '{"from_index": 0, "limit": 10}'
```

### can_subsidize_outlayer_calls

Check if contract will pay for OutLayer calls.

```bash
near view price-oracle.near can_subsidize_outlayer_calls
```

Returns `true` if:
1. `subsidize_outlayer_calls` flag is enabled
2. Contract balance > 20 NEAR

## Change Methods

### oracle_call

**Primary method for DeFi integrations.** Gets prices and invokes callback on your contract.

```bash
near call price-oracle.testnet oracle_call '{
  "receiver_id": "zavodil.testnet",
  "asset_ids": ["wrap.near", "usdt.tether-token.near"],
  "msg": "swap"
}' --accountId zavodil2.testnet --deposit 0.02 --gas 200000000000000 --networkId testnet 
```

**Arguments:**
- `receiver_id`: Your contract that will receive the callback
- `asset_ids` (optional): Assets to get prices for (default: all)
- `msg`: Arbitrary string passed to callback
- `resource_limits` (optional): OutLayer execution limits (see below)

**Deposit requirements:**
- If prices are **fresh** (in cache): no deposit required (any attached deposit is refunded)
- If prices are **stale** (need OutLayer): 0.01+ NEAR
- If **subsidized mode** active: no deposit required

**Flow:**
1. Check cache for fresh prices
2. If fresh → immediate callback to `receiver_id`
3. If stale → call OutLayer WASI → update cache → callback

> **Compatibility:** Interface matches NEAR Native Price Oracle, but adds automatic OutLayer fallback.

**Custom Resource Limits:**

If default limits (10B instructions, 128MB, 60s) are insufficient, you can override:

```bash
near call price-oracle.testnet oracle_call '{
  "receiver_id": "defi.testnet",
  "asset_ids": ["wrap.near"],
  "msg": "swap",
  "resource_limits": {
    "max_instructions": 20000000000,
    "max_memory_mb": 256,
    "max_execution_seconds": 120
  }
}' --accountId user.testnet --deposit 0.05 --gas 300000000000000 --networkId testnet
```

### request_price_data

**Get prices directly — no callback needed.** Checks cache first, fetches from OutLayer if stale.

```bash
near call price-oracle.testnet request_price_data '{
  "asset_ids": ["wrap.near", "usdt.tether-token.near"]
}' --accountId user.testnet --deposit 0.02 --gas 200000000000000 --networkId testnet
```

**Arguments:**
- `asset_ids` (optional): Assets to get prices for (default: all)
- `resource_limits` (optional): OutLayer execution limits

**Deposit requirements:**
- If prices are **fresh** (in cache): no deposit required
- If prices are **stale** (need OutLayer): 0.01+ NEAR
- If **subsidized mode** active: no deposit required

**Returns:** `PriceData` (same format as `get_price_data`)

**Flow:**
1. Check cache for fresh prices
2. If fresh → return immediately
3. If stale → call OutLayer WASI → update cache → return fresh prices

> Unlike `oracle_call`, this method returns data directly. No need to implement a callback on your contract.

### request_custom_data

**Fetch custom data directly — no callback needed.** Fetches data from external sources via OutLayer WASI.

```bash
near call price-oracle.testnet request_custom_data '{
  "custom_data_request": [
    { "id": "eur_usd", "token_id": "", "source": { "custom": { "url": "https://open.er-api.com/v6/latest/EUR", "json_path": "rates.USD" } } },
    { "id": "gold", "token_id": "gold", "source": "coingecko" }
  ]
}' --accountId user.testnet --deposit 0.02 --gas 200000000000000 --networkId testnet
```

**Arguments:**
- `custom_data_request`: List of data to fetch (same format as `custom_call`)
- `resource_limits` (optional): OutLayer execution limits

**Returns:** `Vec<CustomDataResult>`
```json
[
  { "id": "eur_usd", "value": 1.08, "timestamp": 1706889600 },
  { "id": "gold", "value": 2650.50, "timestamp": 1706889600 }
]
```

**Deposit:** 0.01+ NEAR (or free if subsidized)

> Unlike `custom_call`, this method returns data directly. No need to implement `on_custom_data` callback.

### custom_call

Fetch custom data with callback to your contract. Your contract must implement `on_custom_data`.

```bash
near call price-oracle.testnet custom_call '{
  "receiver_id": "zavodil.testnet",
  "custom_data_request": [
    { "id": "eur_usd_rate", "token_id": "", "source": { "custom": { "url": "https://open.er-api.com/v6/latest/EUR", "json_path": "rates.USD" } } },
    { "id": "elden_ring", "token_id": "elden_ring", "source": { "custom": { "url": "https://store.steampowered.com/api/appdetails/?appids=1245620&cc=us", "json_path": "1245620.data.price_overview.final" } } }
  ],
  "msg": "my_action"
}' --accountId zavodil2.testnet --deposit 0.02 --gas 200000000000000 --networkId testnet
```

**Arguments:**
- `receiver_id`: Your contract that will receive the callback
- `custom_data_request`: List of data to fetch
- `msg`: Arbitrary string passed to callback
- `resource_limits` (optional): OutLayer execution limits

**Custom source config:**
| Field | Type | Description |
|-------|------|-------------|
| `url` | string | HTTP URL to fetch |
| `json_path` | string | Dot notation path (e.g., `"data.price"`, `"items.0.value"`) |
| `value_type` | string | `"number"`, `"string"`, or `"boolean"` (default: `"number"`) |
| `method` | string | `"GET"` or `"POST"` (default: `"GET"`) |
| `headers` | array | Optional headers as `[["Key", "Value"], ...]` |

**Deposit:** 0.01+ NEAR (or free if subsidized)

### report_prices

Submit prices (for registered oracles only).

```bash
near call price-oracle.near report_prices '{
  "prices": [
    {"asset_id": "wrap.near", "price": {"multiplier": "500000000", "decimals": 8}}
  ],
  "claim_near": true
}' --accountId oracle1.near
```

**Arguments:**
- `prices`: Array of `AssetPrice`
- `claim_near` (optional): Claim NEAR reward if eligible

> **Compatibility:** From NEAR Native Price Oracle.

## Callback Interface

Your contract must implement `oracle_on_call` to receive price data:

```rust
use near_sdk::{near_bindgen, AccountId};
use near_sdk::serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct Price {
    pub multiplier: String,  // u128 as string
    pub decimals: u8,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct AssetOptionalPrice {
    pub asset_id: String,
    pub price: Option<Price>,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct PriceData {
    pub timestamp: String,  // u64 as string
    pub recency_duration_sec: u32,
    pub prices: Vec<AssetOptionalPrice>,
}

#[near_bindgen]
impl Contract {
    pub fn oracle_on_call(
        &mut self,
        sender_id: AccountId,
        data: PriceData,
        msg: String,
    ) {
        // Verify caller is the oracle contract
        assert_eq!(
            env::predecessor_account_id(),
            "price-oracle.near".parse().unwrap(),
            "Only oracle can call"
        );

        // Process prices
        for asset_price in data.prices {
            if let Some(price) = asset_price.price {
                let multiplier: u128 = price.multiplier.parse().unwrap();
                let price_usd = multiplier as f64 / 10f64.powi(price.decimals as i32);

                // Use the price for your DeFi logic
                match msg.as_str() {
                    "swap" => self.execute_swap(&asset_price.asset_id, price_usd),
                    "liquidate" => self.check_liquidation(&asset_price.asset_id, price_usd),
                    _ => {}
                }
            }
        }
    }
}
```

## Integration Examples

### JavaScript/TypeScript

```typescript
import { connect, Contract, keyStores } from 'near-api-js';

// View prices (free)
const config = {
  networkId: 'mainnet',
  nodeUrl: 'https://rpc.mainnet.near.org',
};
const near = await connect(config);
const account = await near.account('your-account.near');

const oracle = new Contract(account, 'price-oracle.near', {
  viewMethods: ['get_price_data', 'can_subsidize_outlayer_calls'],
  changeMethods: ['oracle_call', 'request_price_data', 'request_custom_data', 'custom_call'],
});

// Get cached prices
const priceData = await oracle.get_price_data({
  asset_ids: ['wrap.near', 'usdt.tether-token.near'],
});

console.log('NEAR price:', priceData.prices[0].price);

// Request prices with callback (requires signing)
await oracle.oracle_call(
  {
    receiver_id: 'your-defi.near',
    asset_ids: ['wrap.near'],
    msg: 'swap',
  },
  '100000000000000', // 100 TGas
  '20000000000000000000000', // 0.02 NEAR
);

// Request fresh prices (no callback)
const freshPrices = await oracle.request_price_data(
  { asset_ids: ['wrap.near', 'usdt.tether-token.near'] },
  '200000000000000', // 200 TGas
  '20000000000000000000000', // 0.02 NEAR
);

// Request custom data (no callback)
const customData = await oracle.request_custom_data(
  {
    custom_data_request: [
      { id: 'eur_usd', token_id: '', source: { custom: { url: 'https://open.er-api.com/v6/latest/EUR', json_path: 'rates.USD' } } },
    ],
  },
  '200000000000000', // 200 TGas
  '20000000000000000000000', // 0.02 NEAR
);
```

### Rust Contract Integration

```rust
use near_sdk::{ext_contract, AccountId, NearToken, Promise};

#[ext_contract(ext_oracle)]
pub trait Oracle {
    fn oracle_call(
        &mut self,
        receiver_id: AccountId,
        asset_ids: Option<Vec<String>>,
        msg: String,
    ) -> Promise;
}

impl Contract {
    pub fn request_prices(&self) -> Promise {
        ext_oracle::ext("price-oracle.near".parse().unwrap())
            .with_attached_deposit(NearToken::from_millinear(20)) // 0.02 NEAR
            .with_static_gas(Gas::from_tgas(100))
            .oracle_call(
                env::current_account_id(),
                Some(vec!["wrap.near".to_string()]),
                "my_operation".to_string(),
            )
    }
}
```

## EMA (Exponential Moving Average)

Request EMA prices by appending `#<period_sec>` to asset ID:

```bash
# Get 1-hour EMA for wrap.near
near view price-oracle.near get_price_data '{"asset_ids": ["wrap.near#3600"]}'

# Get 24-hour EMA
near view price-oracle.near get_price_data '{"asset_ids": ["wrap.near#86400"]}'
```

EMA must be configured via a council/DAO proposal first — there is no direct owner method:
```bash
# A council member proposes AddAssetEma; it executes once the vote threshold is met
near call price-oracle.near create_proposal '{"action": {"action": "add_asset_ema", "asset_id": "wrap.near", "period_sec": 3600}}' \
  --accountId council-member.near --deposit 0.1
```

> **Compatibility:** EMA feature from NEAR Native Price Oracle.

## Public Storage (Direct Access)

Prices are also cached in OutLayer public storage for direct HTTP access. Reads go
through the batch endpoint (`project_uuid` = the OutLayer project, `p0000000000000003`
for `price-oracle.near/price-oracle`):

```bash
# Read one or more keys without a blockchain call
curl -X POST "https://api.outlayer.fastnear.com/public/storage/batch" \
  -H "Content-Type: application/json" \
  -d '{"project_uuid": "p0000000000000003", "keys": ["price:wrap.near"]}'
```

Response — values are base64-encoded JSON:
```json
{
  "results": {
    "price:wrap.near": { "exists": true, "value": "<base64 of the object below>" }
  }
}
```

Decoded value (a `StoredPrice` — note `price` is a plain number, not a multiplier/decimals pair):
```json
{
  "price": 5.00,
  "timestamp": 1706889600,
  "sources": [
    { "name": "coingecko", "price": 5.01, "timestamp": 1706889600 },
    { "name": "binance", "price": 4.99, "timestamp": 1706889600 },
    { "name": "pyth", "price": 5.00, "timestamp": 1706889598 }
  ],
  "aggregation_method": "median"
}
```

## Error Handling

| Error | Cause | Solution |
|-------|-------|----------|
| `Requires at least 0.01 NEAR` | Insufficient deposit for stale prices | Attach 0.01+ NEAR or wait for subsidized mode |
| `OutLayer not configured` | Contract not set up for WASI | Contact oracle owner |
| `Not an oracle` | Caller not registered as oracle | Only for `report_prices` |
| `Unknown asset` | Asset not whitelisted | Use `request_custom_data` for non-whitelisted tokens |

## Costs

| Operation | Cost |
|-----------|------|
| `get_price_data` (view) | Free |
| `oracle_call` (cached) | ~0.0001 NEAR (gas only) |
| `oracle_call` (stale, user pays) | 0.01-0.02 NEAR |
| `oracle_call` (subsidized) | Free for caller |
| `request_price_data` (cached) | Free |
| `request_price_data` (stale, user pays) | 0.01-0.02 NEAR |
| `request_price_data` (subsidized) | Free for caller |
| `request_custom_data` | 0.01-0.02 NEAR |
| `custom_call` | 0.01-0.02 NEAR |

## Supported Assets

Default whitelisted assets (defined in `tokens.json`):
- `wrap.near` - Wrapped NEAR
- `usdt.tether-token.near` - Tether USD
- `17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1` - USD Coin (USDC)
- `aurora` - Aurora (ETH)
- `nbtc.bridge.near` - Bitcoin (nBTC)

Check current assets:
```bash
near view price-oracle.near get_assets '{}'
```

## Compatibility with NEAR Native Oracle

Oracle Example maintains full backward compatibility with [NearDeFi/price-oracle](https://github.com/NearDeFi/price-oracle):

| Method | Status | Notes |
|--------|--------|-------|
| `get_price_data` | Compatible | Same interface |
| `get_oracle_price_data` | Compatible | Same interface |
| `oracle_call` | Compatible | Extended with OutLayer fallback |
| `report_prices` | Compatible | Same interface |
| `get_oracle` / `get_oracles` | Compatible | Same interface |
| `get_asset` / `get_assets` | Compatible | Same interface |
| EMA queries (`asset#period`) | Compatible | Same format |

**New methods** (Oracle Example only):
- `request_price_data` - Get prices directly without callback (checks cache, fetches from OutLayer if stale)
- `request_custom_data` - Fetch custom data directly without callback
- `custom_call` - Fetch custom data with callback
- `can_subsidize_outlayer_calls` - Check subsidization status
- `ConfigureOutlayer` - a council/DAO proposal action (not a direct owner method)

## Links

- [Deployment Guide](DEPLOY.md)
- [NEAR Native Price Oracle](https://github.com/NearDeFi/price-oracle) (original contract)
- [OutLayer Documentation](https://outlayer.fastnear.com)
