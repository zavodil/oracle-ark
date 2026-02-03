# Oracle-Ark Deployment Guide

This guide covers deploying the complete oracle system to NEAR mainnet.

## Architecture Overview

```
┌─────────────────────┐     ┌─────────────────────┐     ┌─────────────────────┐
│     Scheduler       │────▶│   WASI (OutLayer)   │────▶│   Contract (NEAR)   │
│   (Docker/VPS)      │     │      (TEE)          │     │   oracle-ark.near   │
└─────────────────────┘     └─────────────────────┘     └─────────────────────┘
        │                           │                           │
   Triggers updates           Fetches prices              Stores prices
   every 60s or 1%           from APIs in TEE            for DeFi apps
```

## Prerequisites

- NEAR CLI: `npm install -g near-cli`
- Cargo Near: `cargo install cargo-near`
- Rust 1.85+ with `wasm32-wasip2` target
- Docker (for scheduler)
- OutLayer account at https://outlayer.fastnear.com

## Step 1: Deploy NEAR Contract

### 1.1 Build the contract

```bash
cd wasi-examples/oracle-ark/contract
./build_docker.sh
```

This creates `res/price_oracle.wasm`.

### 1.2 Deploy to mainnet

```bash
# Deploy contract
near contract deploy price-oracle.testnet use-file ./res/price_oracle.wasm without-init-call network-config testnet sign-with-keychain send

# Initialize contract
near call price-oracle.testnet new '{
  "recency_duration_sec": 300,
  "owner_id": "zavodil2.testnet",
  "near_claim_amount": "100000000000000000000000"
}' --accountId price-oracle.testnet --networkId testnet


# Upgrade contract
WASM_BASE64=$(base64 -i target/near/price_oracle.wasm)
near call price-oracle.testnet upgrade --base64 "$WASM_BASE64" --accountId zavodil2.testnet --gas 300000000000000 --networkId testnet
```


### 1.3 Configure assets


```bash
# Add all assets to track
# Testnet
./scripts/add_assets.sh price-oracle.testnet zavodil2.testnet testnet
# Mainnet
./scripts/add_assets.sh oracle-ark.near owner.near mainnet
```


```bash
# Add assets to track (must match tokens.json)
near call oracle-ark.your-account.near add_asset '{"asset_id": "wrap.near"}' \
  --accountId your-account.near --depositYocto 1

near call oracle-ark.your-account.near add_asset '{"asset_id": "usdt.tether-token.near"}' \
  --accountId your-account.near --depositYocto 1

# USDC on NEAR (note: long hash address)
near call oracle-ark.your-account.near add_asset '{"asset_id": "17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1"}' \
  --accountId your-account.near --depositYocto 1
```

### 1.4 Add WASI worker as oracle

The contract itself will be registered as an oracle (for WASI-provided prices):

```bash
near call oracle-ark.your-account.near add_oracle '{"account_id": "oracle-ark.your-account.near"}' \
  --accountId your-account.near --depositYocto 1
```

## Step 2: Deploy WASI to OutLayer

### 2.1 Build WASI binary

```bash
cd wasi-examples/oracle-ark
./build.sh
```

python3 upload_wasm_fastfs.py ../wasi-examples/oracle-ark/target/wasm32-wasip2/release/oracle-ark.wasm ../worker/.env.dev.worker1 https://rpc.testnet.near.org

This creates `target/wasm32-wasip2/release/oracle_ark.wasm`.

### 2.2 Create OutLayer project

1. Go to https://outlayer.fastnear.com
2. Connect your NEAR wallet
3. Create a new project:
   - **Name**: `oracle-ark`
   - **Repository**: Your GitHub repo URL
   - **Branch**: `main`
   - **Path**: `wasi-examples/oracle-ark`

4. Deploy the project and note your:
   - Project UUID (e.g., `p0000000000000001`)
   - Project owner (your NEAR account)

### 2.3 Create payment key

In the OutLayer dashboard:
1. Go to "Payment Keys"
2. Create a new key
3. Fund it with NEAR (recommended: 1-5 NEAR for testing)
4. Copy the full payment key string: `owner.near:0:secret...`

### 2.4 Configure contract to use OutLayer

```bash
near call price-oracle.testnet configure_outlayer '{
  "outlayer_contract_id": "outlayer.testnet",
  "code_source": "{\"Project\": {\"project_id\": \"zavodil2.testnet/price-oracle\"}}",   "secrets_profile": "default",
  "secrets_account_id": "zavodil2.testnet"
}' --accountId zavodil2.testnet --networkId testnet --depositYocto 1
```

Note: Omitting `version_key` uses the project's active version. To pin a specific version:
```bash
"code_source": "{\"Project\": {\"project_id\": \"owner/project\", \"version_key\": \"abc123\"}}"
```



### 2.5 (Optional) Enable subsidized calls

By default, users calling `oracle_call` must attach 0.01+ NEAR to cover OutLayer execution.
You can enable **subsidized mode** where the contract pays from its own balance:

```bash
# Enable subsidized OutLayer calls
near call oracle-ark.your-account.near set_subsidize_outlayer_calls '{"enabled": true}' \
  --accountId your-account.near --depositYocto 1
```

**Requirements for subsidized calls:**
- `subsidize_outlayer_calls` must be enabled (see above)
- Contract balance must be > 20 NEAR

When both conditions are met, `oracle_call` works without requiring user deposit.
The contract pays 0.02 NEAR per OutLayer call, and refunds return to the contract.

```bash
# Check if subsidization is currently active
near view oracle-ark.your-account.near can_subsidize_outlayer_calls

# Check if subsidization is enabled (regardless of balance)
near view oracle-ark.your-account.near get_subsidize_outlayer_calls
```

## Step 3: Deploy Scheduler

### 3.1 Configure environment

```bash
cd wasi-examples/oracle-ark/scheduler
cp .env.example .env
```

Edit `.env`:

```bash
# Required
COORDINATOR_URL=https://api.outlayer.fastnear.com
PROJECT_OWNER=your-account.near
PROJECT_NAME=oracle-ark
PROJECT_UUID=p0000000000000001
PAYMENT_KEY=your-account.near:0:your-secret-key

# Tokens config file (shared with WASI)
# Edit ../tokens.json to add/remove tokens
TOKENS_CONFIG=../tokens.json

# Update triggers
UPDATE_INTERVAL_SECS=60
PRICE_DIFF_THRESHOLD_PERCENT=1.0

# Optional: also update contract state
UPDATE_CONTRACT_ENABLED=false
ORACLE_CONTRACT_ID=oracle-ark.your-account.near

# Logging
RUST_LOG=info
```

The `tokens.json` file (at `oracle-ark/tokens.json`) defines which tokens to track:

```json
{
  "wrap.near": {
    "decimals": 24,
    "coingecko": "near",
    "binance": "NEARUSDT",
    "pyth": "0xc415de8d2efa7db216527dff4b60e8f3a5311c740dadb233e13e12547e226750"
  },
  "usdt.tether-token.near": {
    "decimals": 6,
    "stablecoin": true,
    "coingecko": "tether",
    "pyth": "0x2b89b9dc8fdf9f34709a5b106b472f0f39bb6ca9ce04b0fd7f2e971688e2e53b"
  }
}
```

### 3.2 Build and run with Docker

```bash
# Build image
cd /wasi-examples/oracle-ark
docker build -t oracle-scheduler -f scheduler/Dockerfile .
# reset cache
docker build --no-cache -t oracle-scheduler -f scheduler/Dockerfile .

# Run container in console
docker run --name oracle-scheduler --env-file scheduler/.env oracle-scheduler
# Run container on a background
docker run -d --name oracle-scheduler --restart unless-stopped --env-file .env oracle-scheduler

# Remove container
docker rm -f oracle-scheduler 2>/dev/null; 

```

Or use docker-compose:

```bash
docker-compose up -d
```

### 3.3 View logs

```bash
docker logs -f oracle-scheduler
```

## Step 4: Verify Deployment

### 4.1 Check WASI public storage

```bash
# Read cached price from WASI public storage
curl "https://api.outlayer.fastnear.com/storage/your-account.near/oracle-ark/price:wrap.near"
```

### 4.2 Check contract prices

```bash
near view price-oracle.testnet get_price_data '{"asset_ids": ["wrap.near"]}' --networkId testnet
```

### 4.3 Test oracle_call (for DeFi integration)

```bash
# This will fetch fresh prices if needed (costs gas + OutLayer fee)
near call oracle-ark.your-account.near oracle_call '{
  "receiver_id": "your-defi-contract.near",
  "asset_ids": ["wrap.near"],
  "msg": "test"
}' --accountId your-account.near --deposit 0.02
```

## Integration Guide for DeFi

### Callback interface

Your DeFi contract must implement:

```rust
pub fn oracle_on_call(&mut self, sender_id: AccountId, data: PriceData, msg: String);
```

Where `PriceData` is:

```rust
pub struct PriceData {
    pub timestamp: u64,
    pub recency_duration_sec: u32,
    pub prices: Vec<AssetOptionalPrice>,
}

pub struct AssetOptionalPrice {
    pub asset_id: String,
    pub price: Option<Price>,
}

pub struct Price {
    pub multiplier: u128,
    pub decimals: u8,
}
```

### Example integration

```rust
#[ext_contract(ext_oracle)]
trait Oracle {
    fn oracle_call(
        &mut self,
        receiver_id: AccountId,
        asset_ids: Option<Vec<String>>,
        msg: String,
    ) -> Promise;
}

// In your contract:
pub fn request_prices(&mut self) -> Promise {
    ext_oracle::ext("oracle-ark.your-account.near".parse().unwrap())
        .with_attached_deposit(NearToken::from_millinear(20)) // 0.02 NEAR
        .oracle_call(
            env::current_account_id(),
            Some(vec!["wrap.near".to_string()]),
            "swap".to_string(),
        )
}

pub fn oracle_on_call(&mut self, sender_id: AccountId, data: PriceData, msg: String) {
    // Process prices...
    for price_info in data.prices {
        if let Some(price) = price_info.price {
            let usd_price = price.multiplier as f64 / 10f64.powi(price.decimals as i32);
            // Use the price...
        }
    }
}
```

## Monitoring

### Scheduler health

```bash
# Check if container is running
docker ps | grep oracle-scheduler

# Check recent logs
docker logs --tail 100 oracle-scheduler
```

### Price freshness

```bash
# Check last update timestamp
curl -s "https://api.outlayer.fastnear.com/storage/your-account.near/oracle-ark/price:wrap.near" | jq '.timestamp'
```

### Payment key balance

Monitor your payment key balance in the OutLayer dashboard. Refill when low.

## Troubleshooting

### Scheduler not updating

1. Check logs: `docker logs oracle-scheduler`
2. Verify payment key has funds
3. Check COORDINATOR_URL is correct
4. Verify TOKENS list matches contract assets

### Contract returns stale prices

1. Check scheduler is running
2. Verify OutLayer deployment is active
3. Check `recency_duration_sec` setting
4. Ensure payment key is funded

### WASI execution fails

1. Check OutLayer dashboard for errors
2. Verify code_source commit exists
3. Check resource limits in contract call

## Costs

| Component | Cost |
|-----------|------|
| Contract deployment | ~2 NEAR |
| Each scheduler update | ~0.001 NEAR (via payment key) |
| Each oracle_call (fresh, user pays) | ~0.01-0.02 NEAR (paid by caller) |
| Each oracle_call (fresh, subsidized) | ~0.02 NEAR (paid by contract) |
| Each oracle_call (cached) | ~0.0001 NEAR (just gas) |

**Subsidized mode:** When enabled and contract has > 20 NEAR, the contract pays for OutLayer calls.
Users can call `oracle_call` without attaching deposit. Refunds return to contract balance.

## Security Notes

1. **Payment key**: Store securely, rotate periodically
2. **Owner key**: Use a multisig or DAO for production
3. **Price sources**: WASI fetches from multiple sources and uses median
4. **TEE verification**: All prices are generated inside Intel TDX enclave
