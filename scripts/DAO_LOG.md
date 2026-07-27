# DAO Operations Log

Executed governance actions on `price-oracle.near` (mainnet), newest first.
Command reference lives in [PROPOSALS.md](PROPOSALS.md); this file records what was actually run.

---

## 2026-07-27 — Signing key for the Rhea feed

`PROTECTED_RHEA_FEED_KEY` was created in the OutLayer project `price-oracle.near/price-oracle`,
secrets profile **`oracle`**. Like every `PROTECTED_` secret it is generated inside the enclave, so
the private half has never been visible to anyone, including us.

It is deliberately a **separate key from the on-chain push signer**: the feed is signed with this
one, `report_prices` with the other, so the feed key can be rotated or revoked without touching the
on-chain push, and neither role can be impersonated using the other's key.

Public half read back with the `get_public_key` command
([tx 2ikGDGq6bKKCPhA2GkUWdvShXPmNue9zssownBBUPoU4](https://nearblocks.io/txns/2ikGDGq6bKKCPhA2GkUWdvShXPmNue9zssownBBUPoU4)):

```json
{"implicit_account_id":"d6f438902059e938b75ad1e18eb906c16c3f15ff2efbe19db91a525bc4f6effd",
 "public_key":"ed25519:FU6EnB4UaAiDCAxvQPkRUu5QQExgzvKQAX891wMEX3rU",
 "success":true}
```

**`ed25519:FU6EnB4UaAiDCAxvQPkRUu5QQExgzvKQAX891wMEX3rU`** is what a consumer pins to verify feed
signatures. The `implicit_account_id` is only a by-product of the same key — this key signs nothing
on-chain, so that account needs no funding.

Reading it back:

```bash
curl -sX POST https://api.outlayer.fastnear.com/call/price-oracle.near/price-oracle \
  -H "X-Payment-Key: $PAYMENT_KEY" -H "Content-Type: application/json" \
  -d '{"input":{"command":"get_public_key","key_name":"PROTECTED_RHEA_FEED_KEY"},
       "secrets_ref":{"profile":"oracle","account_id":"price-oracle.near"}}'
```

A consumer does not have to take our word for that key: the call itself is attested, and the TDX
quote's Task Hash covers `output_hash`, so the attestation proves the public key was produced by the
attested binary inside the enclave. Hand the consumer the attestation link alongside the key.

Consumers calling `get_signed_prices` must pass the same
`secrets_ref: {profile: "oracle", account_id: "price-oracle.near"}`, otherwise the key is not present
in the enclave and no signature can be produced.

---

## 2026-07-27 — Source expansion + dead-source cleanup

**Proposal:** `set_asset_exchange_configs` over all 17 assets, submitted by `owner.price-oracle.near`
(0.5 NEAR attached, 30 Tgas). Applied on-chain.

### Why

1. **USDT and FRAX had exactly one source each (Pyth).** A consumer that excludes Pyth — which is
   what Rhea's aggregated oracle does, since it already reads Pyth itself — got no price at all.
2. **DAI was mispriced at $0.50.** Gate's v2 endpoint answers `dai_usdt` with
   `{"last":"0", ..., "result":"true"}` — a *successful* response carrying a zero. The parser
   accepted it, and `median([0.0, 0.9998])` produced `0.4999`. On-chain `get_price_data` returned
   null for DAI (it is warm-only, never pushed), so lending consumers reading the contract were not
   affected, but public storage and any `get_prices` / `oracle_call` path served the bad value.
3. Several configured symbols were verified dead on their venue — they inflated the apparent source
   count without ever returning a price, which matters for `min_sources_num` quorum.

### Changes

| Asset | Added | Removed |
|---|---|---|
| USDT | coingecko, chainlink | — |
| FRAX | coingecko, chainlink | — |
| DAI | coingecko, chainlink | cryptocom `DAI_USDT`, huobi `daiusdt`, **gate `dai_usdt` (zero price)** |
| USDC | coingecko, chainlink | cryptocom `USDC_USDT` |
| WBTC | coingecko | cryptocom `WBTC_USDT`, huobi `wbtcusdt` |
| AURORA | coingecko | cryptocom `AURORA_USDT` |
| WOO | coingecko | cryptocom `WOO_USDT` |
| ETH (`aurora`), BTC (`nbtc.bridge.near`) | coingecko, chainlink | — |
| NEAR, SOL, XRP, DOGE, ADA, XLM, LTC, ZEC | coingecko | — |

Chainlink was deliberately NOT added for XRP, ADA and LTC: those feed addresses in `tokens.json`
(`0xCed2660c…`, `0xAE48c91d…`, `0x6AF09DF7…`) revert on both `latestAnswer()` and
`latestRoundData()`.

Note `set_asset_exchange_configs` **replaces** an asset's config wholesale (`insert`, not a merge),
so the payload was built from the live on-chain config read via `get_asset_exchange_configs`, not
from `tokens.json`.

### Resulting on-chain config (all 18 assets, after this proposal)

| Asset | asset_id | Sources | Configured |
|---|---|---|---|
| NEAR | `wrap.near` | 7 | binance_us, coingecko, cryptocom, gate, huobi, kucoin, pyth |
| ETH | `aurora` | 8 | binance_us, chainlink, coingecko, cryptocom, gate, huobi, kucoin, pyth |
| USDT | `usdt.tether-token.near` | 3 | chainlink, coingecko, pyth |
| USDC | `17208628f84f5d6ad…e36133a1` | 4 | chainlink, coingecko, kucoin, pyth |
| BTC | `nbtc.bridge.near` | 8 | binance_us, chainlink, coingecko, cryptocom, gate, huobi, kucoin, pyth |
| WBTC | `2260fac5e5…factory.bridge.near` | 4 | coingecko, gate, kucoin, pyth |
| DAI | `6b175474e8…factory.bridge.near` | 3 | chainlink, coingecko, pyth |
| AURORA | `aaaaaa20d9…factory.bridge.near` | 5 | coingecko, gate, huobi, kucoin, pyth |
| WOO | `4691937a75…factory.bridge.near` | 5 | coingecko, gate, huobi, kucoin, pyth |
| FRAX | `853d955ace…factory.bridge.near` | 3 | chainlink, coingecko, pyth |
| SOL | `22.contract.portalbridge.near` | 7 | binance_us, coingecko, cryptocom, gate, huobi, kucoin, pyth |
| ZEC | `zec.omft.near` | 5 | coingecko, gate, huobi, kucoin, pyth |
| RHEA | `token.rhealab.near` | 2 | binance_alpha, pyth |
| XRP | `xrp.omft.near` | 7 | binance_us, coingecko, cryptocom, gate, huobi, kucoin, pyth |
| DOGE | `doge.omft.near` | 7 | binance_us, coingecko, cryptocom, gate, huobi, kucoin, pyth |
| ADA | `cardano.omft.near` | 7 | binance_us, coingecko, cryptocom, gate, huobi, kucoin, pyth |
| XLM | `xlm` | 7 | binance_us, coingecko, cryptocom, gate, huobi, kucoin, pyth |
| LTC | `ltc.omft.near` | 7 | binance_us, coingecko, cryptocom, gate, huobi, kucoin, pyth |

FRAX carries no CEX source on purpose: after Frax's April-2025 rebrand the ticker `FRAX` on
exchanges is the *governance* token (~$0.27), while this asset is the stablecoin (~$0.99). It is
priced address-based only (chainlink + coingecko + pyth).

### Pending — six new venues

A follow-up `set_asset_exchange_configs` adds **kraken, coinbase, bitstamp, okx, bitget, mexc**
(83 symbols across 17 assets), which takes USDT from 3 to 7 sources and gives it its first CEX
pricing — four independent real-fiat-USD books. It must be submitted only **after** the batched
worker is deployed, since the current worker does not know those config fields.

### Follow-up

- `sync_asset_configs` must be called with `--gas 300000000000000`. The first attempt used the
  near-cli default (30 Tgas) and did not propagate: `config:assets` in public storage kept the old
  config, so the worker went on querying gate for DAI. Verify with a read of `config:assets`.
- The scheduler caches `config:assets` for 10 minutes, so a synced change takes up to that long to
  take effect.
- CoinGecko was added to all 17 assets. Fetched one-by-one this exceeds the free-tier limit
  (~10 req/min → HTTP 429); it is safe only once the batched worker is deployed.
