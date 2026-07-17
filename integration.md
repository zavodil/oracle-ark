# Oracle-Ark Integration Guide

How to integrate Oracle-Ark price oracle into your NEAR contract or dApp.

**Contract addresses:**
- Mainnet: `price-oracle.near`
- Testnet: `price-oracle.testnet`

## View Methods (Free)

### get_price_data

Returns cached prices for whitelisted assets. If any asset has `price: null` — call `request_price_data` with a deposit to fetch missing prices from OutLayer.

```bash
near view price-oracle.testnet get_price_data '{"asset_ids": ["wrap.near", "usdt.tether-token.near"]}' --networkId testnet
```

**Returns:** `PriceData`
```json
{
  "timestamp": "1706889600000000000",
  "recency_duration_sec": 300,
  "prices": [
    { "asset_id": "wrap.near", "price": { "multiplier": "500000000", "decimals": 8 } },
    { "asset_id": "usdt.tether-token.near", "price": { "multiplier": "100000000", "decimals": 8 } }
  ]
}
```

Price conversion: `multiplier / 10^decimals` = USD price. Example: `500000000 / 10^8 = $5.00`

EMA prices: append `#<period_sec>` to asset ID (e.g., `"wrap.near#3600"` for 1-hour EMA).

### get_oracle_price_data

Get prices from a specific oracle (not median).

```bash
near view price-oracle.testnet get_oracle_price_data '{
  "account_id": "oracle1.near",
  "asset_ids": ["wrap.near"],
  "recency_duration_sec": 600
}'
```

### can_subsidize_outlayer_calls

Check if contract will pay for OutLayer calls (returns `true` if subsidization enabled AND balance > 20 NEAR).

```bash
near view price-oracle.testnet can_subsidize_outlayer_calls
```

---

## request_price_data (Without Callback)

Get prices directly — no callback needed. Checks cache first, fetches from OutLayer if stale.

```bash
near call price-oracle.testnet request_price_data '{
  "asset_ids": ["wrap.near", "usdt.tether-token.near"]
}' --accountId zavodil2.testnet --deposit 0.02 --gas 200000000000000 --networkId testnet
```

**Arguments:**
- `asset_ids` (optional): Assets to get prices for. Default: all whitelisted assets.
- `resource_limits` (optional): Override OutLayer execution limits.

**Returns:** `PriceData` (same format as `get_price_data`)

**Deposit:**
- If prices are fresh in cache: no deposit needed
- If prices are stale (OutLayer fetch): 0.01+ NEAR
- If subsidized mode: free

**When to use:** Simple integrations, off-chain callers, or when you want to chain `.then()` on the result from another contract.

---

## oracle_call (With Callback)

Gets prices and invokes `oracle_on_call` callback on your contract. Use this when your contract needs to process prices atomically.

```bash
near call price-oracle.testnet oracle_call '{
  "receiver_id": "your-defi.testnet",
  "asset_ids": ["wrap.near", "usdt.tether-token.near"],
  "msg": "swap"
}' --accountId user.testnet --deposit 0.02 --gas 200000000000000 --networkId testnet
```

**Arguments:**
- `receiver_id`: Your contract that will receive the callback
- `asset_ids` (optional): Assets to get prices for
- `msg`: Arbitrary string passed through to callback
- `resource_limits` (optional): Override OutLayer execution limits

**Deposit:**
- If prices are fresh: no deposit required (any attached deposit is refunded)
- If prices are stale: 0.01+ NEAR
- If subsidized: free

**Flow:**
1. Check cache for fresh prices
2. If fresh → immediate callback to `receiver_id`
3. If stale → OutLayer WASI fetch → update cache → callback

Your contract must implement `oracle_on_call` (see [Callback Interfaces](#callback-interfaces) below).

---

## request_custom_data (Without Callback)

Fetch custom data from any external source — no callback needed.

```bash
near call price-oracle.testnet request_custom_data '{
  "custom_data_request": [
    { "id": "eur_usd", "token_id": "", "source": { "custom": { "url": "https://open.er-api.com/v6/latest/EUR", "json_path": "rates.USD" } } },
    { "id": "gold", "token_id": "gold", "source": "coingecko" }
  ]
}' --accountId user.testnet --deposit 0.02 --gas 200000000000000 --networkId testnet
```

**Returns:** `Vec<CustomDataResult>`
```json
[
  { "id": "eur_usd", "value": 1.08, "timestamp": 1706889600 },
  { "id": "gold", "value": 2650.50, "timestamp": 1706889600 }
]
```

**Deposit:** 0.01+ NEAR (or free if subsidized)

---

## custom_call (With Callback)

Fetch custom data and invoke `on_custom_data` callback on your contract.

```bash
near call price-oracle.testnet custom_call '{
  "receiver_id": "your-app.testnet",
  "custom_data_request": [
    { "id": "eur_usd", "token_id": "", "source": { "custom": { "url": "https://open.er-api.com/v6/latest/EUR", "json_path": "rates.USD" } } }
  ],
  "msg": "my_action"
}' --accountId user.testnet --deposit 0.02 --gas 200000000000000 --networkId testnet
```

Your contract must implement `on_custom_data` (see [Callback Interfaces](#callback-interfaces) below).

---

## Callback Interfaces

### oracle_on_call (for oracle_call)

```rust
#[near_bindgen]
impl Contract {
    pub fn oracle_on_call(
        &mut self,
        sender_id: AccountId,
        data: PriceData,
        msg: String,
    ) {
        assert_eq!(
            env::predecessor_account_id(),
            "price-oracle.near".parse::<AccountId>().unwrap(),
            "Only oracle can call"
        );

        for asset_price in data.prices {
            if let Some(price) = asset_price.price {
                let multiplier: u128 = price.multiplier.parse().unwrap();
                let price_usd = multiplier as f64 / 10f64.powi(price.decimals as i32);
                // Use price_usd for your logic
            }
        }
    }
}
```

### on_custom_data (for custom_call)

```rust
#[near_bindgen]
impl Contract {
    pub fn on_custom_data(
        &mut self,
        sender_id: AccountId,
        data: Vec<CustomDataResult>,
        msg: String,
    ) {
        assert_eq!(
            env::predecessor_account_id(),
            "price-oracle.near".parse::<AccountId>().unwrap(),
            "Only oracle can call"
        );

        for item in data {
            if let Some(value) = item.value {
                // Process item.id and value
            }
        }
    }
}
```

---

## Code Examples

### Rust: Using request_price_data (simple)

```rust
use near_sdk::{ext_contract, AccountId, Gas, NearToken, Promise};

#[ext_contract(ext_oracle)]
pub trait Oracle {
    fn request_price_data(
        &mut self,
        asset_ids: Option<Vec<String>>,
    );
}

impl Contract {
    pub fn get_near_price(&self) -> Promise {
        ext_oracle::ext("price-oracle.near".parse().unwrap())
            .with_attached_deposit(NearToken::from_millinear(20))
            .with_static_gas(Gas::from_tgas(100))
            .request_price_data(Some(vec!["wrap.near".to_string()]))
    }
}
```

### Rust: Using oracle_call (DeFi callback pattern)

```rust
use near_sdk::{ext_contract, AccountId, Gas, NearToken, Promise};

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
            .with_attached_deposit(NearToken::from_millinear(20))
            .with_static_gas(Gas::from_tgas(100))
            .oracle_call(
                env::current_account_id(),
                Some(vec!["wrap.near".to_string()]),
                "my_operation".to_string(),
            )
    }
}
```

### JavaScript/TypeScript

```typescript
import { connect, Contract } from 'near-api-js';

const oracle = new Contract(account, 'price-oracle.near', {
  viewMethods: ['get_price_data', 'can_subsidize_outlayer_calls'],
  changeMethods: ['request_price_data', 'request_custom_data', 'oracle_call', 'custom_call'],
});

// View cached prices (free)
const cached = await oracle.get_price_data({
  asset_ids: ['wrap.near'],
});

// Request fresh prices (payable, no callback)
const fresh = await oracle.request_price_data(
  { asset_ids: ['wrap.near', 'usdt.tether-token.near'] },
  '200000000000000', // 200 TGas
  '20000000000000000000000', // 0.02 NEAR
);

// Request custom data (payable, no callback)
const customData = await oracle.request_custom_data(
  {
    custom_data_request: [
      { id: 'eur_usd', token_id: '', source: { custom: { url: 'https://open.er-api.com/v6/latest/EUR', json_path: 'rates.USD' } } },
    ],
  },
  '200000000000000',
  '20000000000000000000000',
);
```

---

## Deposit Summary

| Method | Cached/Fresh | Stale/OutLayer | Subsidized |
|--------|-------------|----------------|------------|
| `get_price_data` (view) | Free | N/A | N/A |
| `request_price_data` | Free | 0.01+ NEAR | Free |
| `oracle_call` | None (refunded) | 0.01+ NEAR | Free |
| `request_custom_data` | N/A | 0.01+ NEAR | Free |
| `custom_call` | N/A | 0.01+ NEAR | Free |
