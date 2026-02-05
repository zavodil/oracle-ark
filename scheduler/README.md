# Oracle-Ark Scheduler

Background service that keeps TEE worker prices fresh by monitoring external price sources and triggering WASI updates when needed.

## Architecture

The scheduler solves a key problem: how to keep TEE worker prices up-to-date so that any incoming request gets an **instant response with fresh data**, without spending gas on every update.

### Components

```
External APIs                    Scheduler (VPS)                TEE Worker (Phala Cloud)
─────────────                    ───────────────                ────────────────────────
CoinGecko ─┐                     ┌─────────────┐               ┌──────────────────────┐
Binance   ─┤  compare prices     │ Poll loop   │  read stored  │ Public Storage       │
Pyth      ─┼──────────────────>  │ (every 10s) │ <──────────── │  price:wrap.near     │
KuCoin    ─┤                     │             │   prices      │  price:aurora        │
Gate.io   ─┤                     │  if delta > │               │  price:nbtc...       │
Huobi     ─┤                     │  threshold: │               │                      │
Crypto.com─┘                     │             │  trigger      │ WASI Binary          │
                                 │  call WASI  │ ────────────> │  fetches own prices  │
                                 │  (no data!) │  update       │  from all 9 sources  │
                                 └─────────────┘               │  aggregates (median) │
                                                               │  writes to storage   │
                                                               └──────────────────────┘
```

### How it works

1. **TEE worker** (WASI binary inside Intel TDX enclave) holds fresh prices in its public storage. When any user or contract requests a price, the worker returns the cached result immediately — no external API calls at request time.

2. **Scheduler** runs **outside TEE** on a separate VPS. Every 10 seconds it:
   - Fetches current prices from external sources (CoinGecko, Binance, Pyth, etc.) for comparison
   - Reads the TEE worker's stored prices via OutLayer public storage batch API
   - Compares the two and decides whether an update is needed

3. **Triggering an update**: the scheduler sends a `call` request to the OutLayer coordinator with command `update_prices` and the list of tokens to refresh. **The scheduler does NOT send price data** — it only tells the TEE worker *which* tokens need updating. The worker then fetches prices from all configured sources independently inside the enclave, aggregates them, and writes the result to public storage.

### Why this design?

- **Trust model preserved** — the scheduler never provides data that needs to be trusted; all price fetching and aggregation happens inside TEE
- **Gas-free** — prices stay fresh in TEE public storage without on-chain transactions
- **Instant responses** — any WASI call requesting prices gets pre-computed results
- **Efficient** — only tokens with significant changes or stale data get updated

## Update triggers

| Trigger | Condition | Default |
|---------|-----------|---------|
| **Price deviation** | `abs(current - stored) / stored > threshold` | 1% |
| **Time interval** | Time since last update exceeds interval | 60s |
| **Missing price** | Token has no stored price yet | Always triggers |

Either trigger is sufficient. If a price moves fast, it gets updated before the interval expires.

## Optional: on-chain contract push

Set `UPDATE_CONTRACT_ENABLED=true` to have the TEE worker also call `report_prices` on the oracle smart contract after updating public storage. This writes prices on-chain, making them available via contract view methods without an OutLayer call.

Each on-chain update costs gas, so this mode is **disabled by default**. Enable it only when on-chain price availability is required.

## Setup

### 1. Copy environment file

```bash
cp .env.example .env
```

### 2. Configure required variables

```env
# OutLayer coordinator
COORDINATOR_URL=https://api.outlayer.fastnear.com

# Project identification
PROJECT_OWNER=alice.near
PROJECT_NAME=oracle-ark
PROJECT_UUID=p0000000000000001

# Payment key for WASI calls (create via OutLayer dashboard)
PAYMENT_KEY=alice.near:0:your_secret_key_here
```

### 3. Configure tokens

The scheduler shares `tokens.json` with the WASI binary (default path: `../tokens.json`). Each token entry defines which exchanges to query:

```json
{
  "wrap.near": {
    "decimals": 24,
    "coingecko": "near",
    "binance": "NEARUSDT",
    "pyth": "0xc415de8d...",
    "huobi": "nearusdt",
    "kucoin": "NEAR-USDT",
    "gate": "near_usdt",
    "cryptocom": "NEAR_USDT"
  }
}
```

### 4. Run

**Docker (recommended):**

```bash
docker compose up -d
```

The Docker service is configured with `restart: unless-stopped` for automatic recovery.

**Direct:**

```bash
cargo run --release
```

## Configuration reference

| Variable | Default | Description |
|----------|---------|-------------|
| `COORDINATOR_URL` | `https://api.outlayer.fastnear.com` | OutLayer API URL |
| `PROJECT_OWNER` | required | NEAR account owning the project |
| `PROJECT_NAME` | required | OutLayer project name |
| `PROJECT_UUID` | required | Project UUID for public storage reads |
| `PAYMENT_KEY` | required | Payment key (format: `owner:nonce:secret`) |
| `TOKENS_CONFIG` | `../tokens.json` | Path to shared token configuration |
| `UPDATE_INTERVAL_SECS` | `60` | Max staleness before refresh (seconds) |
| `PRICE_DIFF_THRESHOLD_PERCENT` | `1.0` | Price change % that triggers immediate refresh |
| `UPDATE_CONTRACT_ENABLED` | `false` | Also push prices to on-chain contract (costs gas) |
| `ORACLE_CONTRACT_ID` | — | Contract to update (required if above is true) |
| `AGGREGATION_METHOD` | `median` | Aggregation: `median` / `average` / `weighted_average` |
| `MIN_SOURCES_NUM` | `1` | Minimum sources required for valid price |
| `API_KEY` | — | API key for premium price sources (CoinGecko Pro, etc.) |
| `TELEGRAM_BOT_TOKEN` | — | Telegram bot token for failure alerts |
| `TELEGRAM_CHAT_ID` | — | Chat ID to send alerts to |
| `RUST_LOG` | `info` | Log level: `trace` / `debug` / `info` / `warn` / `error` |

## Monitoring

### Built-in Telegram alerts

Configure `TELEGRAM_BOT_TOKEN` and `TELEGRAM_CHAT_ID` to receive alerts:

- **3+ consecutive poll failures** — error details and project info
- **No prices available** — all external sources failed
- **WASI update failure** — token list and error message

### Logs

The scheduler logs all decisions at `info` level:

```
INFO  Starting oracle scheduler
INFO  Tokens: 13 configured
INFO  Update interval: 60s, threshold: 1%
INFO  wrap.near: triggering update (reason: price change, current=5.1234)
INFO  Triggering WASI update for 3 tokens
INFO  Triggering WASI update successful
```

Set `RUST_LOG=debug` for per-token price comparisons:

```
DEBUG wrap.near: current=5.1234, stored=5.0100, diff=2.26%
DEBUG aurora: current=3456.78, stored=3450.12, diff=0.19%
```

## API interactions

### Reading TEE public storage

```
POST {COORDINATOR_URL}/public/storage/batch
{
  "project_uuid": "...",
  "keys": ["price:wrap.near", "price:aurora", ...]
}
```

Returns base64-encoded JSON for each key:
```json
{
  "price": 5.0123,
  "timestamp": 1706900000000000000,
  "sources": [{"name": "binance", "price": 5.01}, {"name": "coingecko", "price": 5.02}],
  "aggregation_method": "median"
}
```

### Triggering WASI update

```
POST {COORDINATOR_URL}/call/{PROJECT_OWNER}/{PROJECT_NAME}
X-Payment-Key: {PAYMENT_KEY}
{
  "input": {
    "command": "update_prices",
    "tokens": ["wrap.near", "aurora"],
    "update_contract": false,
    "aggregation_method": "median",
    "min_sources_num": 1
  },
  "async": false
}
```

Note: the `input` contains only the token list and configuration — **no price data**. The TEE worker fetches prices independently.
