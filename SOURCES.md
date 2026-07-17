# Available Price Sources

This document lists the price sources the oracle actually supports, and the exact
configuration format used to attach them to an asset.

> Source dispatch lives in [`sources/src/sources.rs`](sources/src/sources.rs)
> (`fetch_all_sources`); the config type is `ExchangeConfig` in
> [`sources/src/lib.rs`](sources/src/lib.rs). Adding a key that no source reads is
> silently ignored (unknown fields are dropped on parse), so keep this list in sync
> with the code.

## Per-asset configuration format

An asset's sources are configured as a single JSON object (an `ExchangeConfig`),
**not** a list of `{name, id}` entries. Each supported exchange is an optional field;
a source is queried only if its field is present. This is exactly the shape stored in
[`tokens.json`](tokens.json) and pushed on-chain via the `SetAssetExchangeConfig`
governance action.

```json
{
  "decimals": 24,
  "coingecko": "near",
  "binance": "NEARUSDT",
  "binance_us": "NEARUSD",
  "huobi": "nearusdt",
  "cryptocom": "NEAR_USDT",
  "kucoin": "NEAR-USDT",
  "gate": "near_usdt",
  "pyth": "0xc415de8d2eba7db216527dff4b60e8f3a5311c740dadb233e13e12547e226750",
  "chainlink": "0x5f4eC3Df9cbd43714FE2740f5E3616155c5b8419",
  "stablecoin": false
}
```

`decimals` and `stablecoin` are metadata (used by the UI / aggregation), not sources.

## Supported sources

| Field | Source | ID format | Example |
|-------|--------|-----------|---------|
| `coingecko` | CoinGecko API | CoinGecko coin id | `"near"`, `"bitcoin"`, `"tether"` |
| `binance` | Binance global | trading symbol | `"NEARUSDT"` |
| `binance_us` | Binance US | trading symbol | `"NEARUSD"` |
| `binance_alpha` | Binance Alpha (wallet token list) | BSC contract address | `"0x…"` |
| `pyth` | Pyth Network (Hermes) | price feed id (`0x` optional) | `"0xc415…6750"` |
| `chainlink` | Chainlink (Ethereum aggregator) | aggregator contract address | `"0x5f4eC3…8419"` |
| `huobi` | Huobi / HTX | lowercase symbol | `"nearusdt"` |
| `kucoin` | KuCoin | dash symbol | `"NEAR-USDT"` |
| `gate` | Gate.io | lowercase underscore pair | `"near_usdt"` |
| `cryptocom` | Crypto.com | underscore instrument | `"NEAR_USDT"` |

Notes:
- **CoinGecko** accepts an optional API key via the `API_KEY` secret (sent as
  `x_cg_pro_api_key`); all other exchanges are keyless.
- **Chainlink** reads `latestAnswer()` over Ethereum RPC with automatic multi-RPC
  failover; if every RPC fails, Chainlink is skipped for that run and an alert is sent.
- **Binance** may return HTTP 451 in geo-blocked regions — pair it with `binance_us`
  and other exchanges so a single blocked source never fails aggregation.

## Source-side freshness

Only **Pyth** exposes an upstream data timestamp that the oracle checks: prices whose
`publish_time` is older than 120 seconds are rejected
([`sources/src/sources.rs`](sources/src/sources.rs), `fetch_pyth`). Every other
endpoint above returns only a value with no data timestamp, so its reading is stamped
with the fetch time and is only as fresh as the moment it was read. See
[the platform freshness docs](https://outlayer.fastnear.com/docs/tee-attestation#data-freshness)
for how consumers should reason about age.

## Custom data sources

Arbitrary HTTP endpoints are **not** part of `ExchangeConfig`. They are fetched through
the `request_custom_data` / `custom_call` path using a `CustomSourceConfig`, which
supports GET/POST, request bodies, custom headers, and dot/array JSON-path extraction:

```json
{
  "custom": {
    "url": "https://api.example.com/price",
    "json_path": "data.price",
    "value_type": "number",
    "method": "GET",
    "headers": []
  }
}
```

- `value_type`: `"number"` (default), `"string"`, or `"boolean"`.
- `method`: `"GET"` (default) or `"POST"` (add a `body` object for POST).
- `json_path`: dot notation with array indexes, e.g. `"1245620.data.price_overview.final"`.
- Custom-source URLs are validated against local/private network access (SSRF guard).

See [CUSTOM_POST_BODY.md](CUSTOM_POST_BODY.md) for POST-body examples.
