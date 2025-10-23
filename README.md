# Oracle Ark - On-Demand Price Oracle

Decentralized price oracle that fetches cryptocurrency and commodity prices from multiple sources, aggregates them, and returns validated data.

## Features

- ✅ **Multiple sources**: CoinGecko, CoinMarketCap, TwelveData
- ✅ **Aggregation methods**: Average, Median, Weighted Average
- ✅ **Price validation**: Configurable max deviation between sources
- ✅ **Encrypted API keys**: Via WASI environment variables
- ✅ **Batch requests**: Up to 10 tokens per request
- ✅ **WASI P2**: Uses `wasi-http-client` for real HTTP requests

## Supported Sources

| Source | Type | API Key | Token Format | Examples |
|--------|------|---------|--------------|----------|
| **CoinGecko** | Crypto | Optional | `"bitcoin"`, `"ethereum"` | BTC, ETH, NEAR |
| **CoinMarketCap** | Crypto | Required | `"BTC"`, `"ETH"` | BTC, ETH, SOL |
| **TwelveData** | Commodities, Forex | Optional | `"XAU/USD"`, `"BRENT/USD"` | Gold, Oil, EUR/USD |

## Quick Start

### 1. Build

```bash
./build.sh

# Or manually:
rustup target add wasm32-wasip2
cargo build --target wasm32-wasip2 --release
```

### 2. Test Locally

```bash
# Use wasi-test-runner (handles WASI HTTP component properly)
cd ../wasi-test-runner
cargo build --release

# Test single token (Bitcoin from CoinGecko)
./target/release/wasi-test \
  --wasm ../oracle-ark/target/wasm32-wasip2/release/oracle-ark.wasm \
  --input-file ../oracle-ark/example_request.json \
  --max-instructions 50000000000

# Test multiple tokens (Bitcoin + Gold + EUR/USD)
./target/release/wasi-test \
  --wasm .../oracle-ark/target/wasm32-wasip2/release/oracle-ark.wasm \
  --input-file ../oracle-ark/example_multi.json \
  --max-instructions 50000000000 \
  --verbose
```

**Note**: Direct `wasmtime` CLI won't work because WASI HTTP requires component model linking. Use `wasi-test-runner` which handles this correctly.

### 3. Deploy to NEAR OutLayer

```bash
# 1. Push to GitHub
git init
git add .
git commit -m "Oracle implementation"
git push

# 2. Encrypt API keys (optional)
cd ../../keystore-worker
./scripts/encrypt_secrets.py '{
  "COINGECKO_API_KEY": "your-key",
  "COINMARKETCAP_API_KEY": "your-key",
  "TWELVEDATA_API_KEY": "your-key"
}'

# 3. Call contract
near call offchainvm.testnet request_execution '{
  "code_source": {
    "repo": "https://github.com/YOUR_USERNAME/YOUR_REPO",
    "commit": "main",
    "build_target": "wasm32-wasip2"
  },
  "resource_limits": {
    "max_instructions": 50000000000,
    "max_memory_mb": 128,
    "max_execution_seconds": 30
  },
  "input_data": "{\"tokens\":[{\"token_id\":\"bitcoin\",\"sources\":[{\"name\":\"coingecko\",\"token_id\":null}],\"aggregation_method\":\"average\",\"min_sources_num\":1}],\"max_price_deviation_percent\":10.0}",
  "secrets_ref": {
    "profile": "default",
    "account_id": "dev.testnet"
  }
}' --accountId your.testnet --deposit 0.1
```

## Request Format

```json
{
  "tokens": [
    {
      "token_id": "bitcoin",
      "sources": [
        {
          "name": "coingecko",
          "token_id": null
        },
        {
          "name": "coinmarketcap",
          "token_id": "BTC"
        }
      ],
      "aggregation_method": "median",
      "min_sources_num": 2
    },
    {
      "token_id": "gold",
      "sources": [
        {
          "name": "twelvedata",
          "token_id": "XAU/USD"
        }
      ],
      "aggregation_method": "average",
      "min_sources_num": 1
    }
  ],
  "max_price_deviation_percent": 5.0
}
```

### Fields

- `token_id`: Main identifier
- `sources[].name`: `"coingecko"` | `"coinmarketcap"` | `"twelvedata"`
- `sources[].token_id`: Source-specific ID (null = use main `token_id`)
- `aggregation_method`: `"average"` | `"median"` | `"weighted_avg"`
- `min_sources_num`: Minimum successful sources required
- `max_price_deviation_percent`: Max allowed % deviation

## Response Format

```json
{
  "tokens": [
    {
      "token": "bitcoin",
      "data": {
        "price": 110836.0,
        "timestamp": 1729447200,
        "sources": ["coingecko", "coinmarketcap"]
      },
      "message": null
    },
    {
      "token": "ethereum",
      "data": null,
      "message": "Not enough sources responded (1/2). Errors: coingecko: HTTP 429, coinmarketcap: HTTP 401"
    }
  ]
}
```

## Examples

All examples use `wasi-test-runner` (see [Quick Start](#quick-start) for setup).

### Single Token (Bitcoin)

```bash
cd ../wasi-test-runner

./target/release/wasi-test \
  --wasm ../oracle-ark/oracle-ark.wasm \
  --input-file ../oracle-ark/example_request.json \
  --max-instructions 50000000000
```

### Multiple Tokens (Bitcoin + Ethereum + NEAR)

```bash
cd ../wasi-test-runner

./target/release/wasi-test \
  --wasm ../oracle-ark/oracle-ark.wasm \
  --input-file ../oracle-ark/example_multi.json \
  --max-instructions 50000000000 \
  --verbose

# Expected output (3 tokens):
# ✅ Execution successful!
# Output: {
#   "tokens": [
#     {"token":"bitcoin","data":{"price":110804.0,"timestamp":1760959996,"sources":["coingecko"]},"message":null},
#     {"token":"ethereum","data":{"price":3456.78,"timestamp":1760959997,"sources":["coingecko"]},"message":null},
#     {"token":"near","data":{"price":8.92,"timestamp":1760959998,"sources":["coingecko"]},"message":null}
#   ]
# }
```

### Custom Request (Commodities)

```bash
cd ../wasi-test-runner

./target/release/wasi-test \
  --wasm ../oracle-ark/oracle-ark.wasm \
  --input '{
    "tokens": [
      {
        "token_id": "gold",
        "sources": [{"name": "twelvedata", "token_id": "XAU/USD"}],
        "aggregation_method": "average",
        "min_sources_num": 1
      },
      {
        "token_id": "oil_brent",
        "sources": [{"name": "twelvedata", "token_id": "BRENT/USD"}],
        "aggregation_method": "average",
        "min_sources_num": 1
      }
    ],
    "max_price_deviation_percent": 10.0
  }' \
  --max-instructions 50000000000
```

## Configuration

### API Keys (Optional)

Set via encrypted secrets:

```json
{
  "COINGECKO_API_KEY": "your-key-here",
  "COINMARKETCAP_API_KEY": "your-key-here",
  "TWELVEDATA_API_KEY": "your-key-here"
}
```

**Note**: CoinGecko and TwelveData work without API keys (free tier). CoinMarketCap requires API key.

## Architecture

```
main.rs
  ├─ Read JSON from stdin
  ├─ Validate (max 10 tokens)
  ├─ Get API keys from env vars
  └─ For each token:
      ├─ Fetch from each source (sources.rs)
      ├─ Check min_sources_num
      ├─ Validate price deviation
      ├─ Aggregate prices (aggregation.rs)
      └─ Build response

sources.rs
  ├─ fetch_coingecko() - HTTP GET to CoinGecko API
  ├─ fetch_coinmarketcap() - HTTP GET with API key header
  └─ fetch_twelvedata() - HTTP GET to TwelveData API

aggregation.rs
  ├─ calculate_average() - Arithmetic mean
  ├─ calculate_median() - Median (protection from outliers)
  └─ calculate_price_deviation() - Validate consistency
```

## Error Handling

### Not Enough Sources

```json
{
  "token": "bitcoin",
  "data": null,
  "message": "Not enough sources responded (1/2). Errors: coingecko: HTTP 429, coinmarketcap: HTTP 401"
}
```

### Price Deviation Too High

```json
{
  "token": "bitcoin",
  "data": null,
  "message": "Price deviation too high: 12.50% (max: 5.00%)"
}
```

### Partial Success

```json
{
  "token": "bitcoin",
  "data": {
    "price": 110836.0,
    "timestamp": 1729447200,
    "sources": ["coingecko"]
  },
  "message": "coinmarketcap: HTTP 429"
}
```

## Limitations

- Max 10 tokens per request
- Sequential processing (not parallel)
- 10 second timeout per source
- Output must be ≤900 bytes (NEAR limit)

## Technical Details

- **Target**: `wasm32-wasip2` (WASI Preview 2)
- **HTTP Client**: `wasi-http-client` 0.2
- **Binary Size**: ~500-800KB (depends on optimizations)
- **Dependencies**: serde, serde_json, wasi-http-client

## License

MIT
