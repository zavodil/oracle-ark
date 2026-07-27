//! High-level source fetch functions
//!
//! This module provides both sync (WASI) and async (scheduler) implementations.

use crate::{parsers, ExchangeConfig, SourcePrice};
#[cfg(feature = "wasi")]
use crate::{CustomSourceConfig, HttpResponse};
use anyhow::Result;
use std::time::{SystemTime, UNIX_EPOCH};

/// Connect timeout for every source request.
///
/// Measured from the production worker host (Dallas): the slowest configured source completes in
/// ~0.57s end-to-end, and the worst TLS handshake observed was ~0.42s — so 3s is a wide margin
/// while still cutting a dead endpoint loose quickly. This matters because sources are fetched
/// sequentially: every second spent on an unreachable host is a second added to the whole cycle.
///
/// NOTE: `wasi-http-client` 0.2 exposes ONLY a connect timeout — there is no read/total timeout.
/// A server that accepts the connection and then stalls cannot be bounded here; the backstop is
/// the per-call `max_execution_seconds` limit the scheduler sets on each WASI invocation.
/// Some providers reject requests without a descriptive User-Agent — CoinGecko answers HTTP 403
/// ("Please add a descriptive User-Agent to your request"). The WASI HTTP client sends none by
/// default, so every outbound request sets one explicitly.
pub const USER_AGENT: &str = "oracle-ark/1.0 (+https://github.com/zavodil/oracle-ark)";

/// WASI-only: the async (scheduler) build drives reqwest, which carries its own 30s total
/// timeout set where the client is built.
#[cfg(feature = "wasi")]
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Why a single-feed Chainlink read failed.
///
/// A revert is deterministic — every RPC executes the same bytecode — so a delisted feed has
/// to stop the RPC rotation instead of replaying the identical failure seven times and then
/// reporting it as "all RPCs failed".
#[cfg(any(feature = "wasi", feature = "async"))]
enum ChainlinkError {
    /// The feed itself is dead, or the address is not a price feed
    Feed(anyhow::Error),
    /// This RPC failed; the next one may still answer
    Rpc(anyhow::Error),
}

// ============================================================================
// WASI (sync) implementation
// ============================================================================

// ----------------------------------------------------------------------------
// Batch plumbing shared by both backends
//
// The WASI worker and the scheduler differ only in how they issue an HTTP GET; grouping
// tokens by symbol and fanning a venue's answer back out is identical work. Keeping one copy
// means a venue cannot end up batched correctly on one side and per-token on the other.
// ----------------------------------------------------------------------------

/// Symbols of one source mapped to the tokens that asked for them.
///
/// A `BTreeMap` keeps the request order stable across runs (a `HashMap` would reshuffle the
/// symbol list on every invocation, which makes logs and responses harder to diff), and
/// several tokens may legitimately share a symbol.
#[cfg(any(feature = "wasi", feature = "async"))]
pub(crate) type SymbolIndex<'a> = std::collections::BTreeMap<&'a str, Vec<&'a str>>;

/// Group the requested tokens by the symbol each of them uses for one source
#[cfg(any(feature = "wasi", feature = "async"))]
pub(crate) fn index_symbols<'a, F>(
    configs: &'a std::collections::HashMap<String, ExchangeConfig>,
    symbol_of: F,
) -> SymbolIndex<'a>
where
    F: Fn(&'a ExchangeConfig) -> Option<&'a str>,
{
    let mut index = SymbolIndex::new();
    for (token, config) in configs {
        if let Some(symbol) = symbol_of(config) {
            index.entry(symbol).or_default().push(token.as_str());
        }
    }
    index
}

/// Fan one source's batch result back out to every token that requested those symbols.
///
/// `resolve_timestamp` produces the `SourcePrice.timestamp` and may return `None` to drop the
/// price — Pyth uses that for its per-feed staleness rule.
#[cfg(any(feature = "wasi", feature = "async"))]
pub(crate) fn fan_out<F>(
    out: &mut std::collections::HashMap<String, Vec<SourcePrice>>,
    index: &SymbolIndex<'_>,
    prices: &parsers::BatchPrices,
    source_name: &str,
    resolve_timestamp: F,
) where
    F: Fn(&str, &parsers::BatchPrice) -> Option<u64>,
{
    for (symbol, tokens) in index {
        if let Some(entry) = prices.get(*symbol) {
            if let Some(timestamp) = resolve_timestamp(symbol, entry) {
                for token in tokens {
                    if let Some(collected) = out.get_mut(*token) {
                        collected.push(SourcePrice {
                            source_name: source_name.to_string(),
                            price: entry.price,
                            timestamp,
                        });
                    }
                }
            }
        }
    }
}

/// Per-feed Pyth staleness rule, applied identically by both backends.
#[cfg(any(feature = "wasi", feature = "async"))]
pub(crate) fn pyth_publish_time(feed: &str, entry: &parsers::BatchPrice) -> Option<u64> {
    let publish_time = entry.timestamp?;
    let age = current_timestamp().saturating_sub(publish_time);
    if age > parsers::PYTH_MAX_AGE_SECS {
        eprintln!("Pyth price {} is stale (published {} seconds ago)", feed, age);
        return None;
    }
    Some(publish_time)
}

#[cfg(feature = "wasi")]
pub mod sync {
    use super::*;
    use crate::{LAST_CHAINLINK_RPC, CHAINLINK_DISABLED};
    use std::collections::HashMap;
    use wasi_http_client::Client;

    fn http_get(url: &str) -> Result<HttpResponse> {
        let response = Client::new()
            .get(url)
            .header("User-Agent", USER_AGENT)
            .connect_timeout(CONNECT_TIMEOUT)
            .send()?;

        let status = response.status();
        if status < 200 || status >= 300 {
            anyhow::bail!("HTTP {}", status);
        }

        Ok(HttpResponse {
            status,
            body: response.body()?,
        })
    }

    pub fn fetch_coingecko(token_id: &str, api_key: Option<&str>) -> Result<SourcePrice> {
        let url = parsers::coingecko_url(token_id, api_key);
        let response = http_get(&url)?;
        let json: serde_json::Value = response.json()?;
        let price = parsers::parse_coingecko(&json, token_id)?;

        Ok(SourcePrice {
            source_name: "coingecko".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub fn fetch_binance(symbol: &str) -> Result<SourcePrice> {
        let url = parsers::binance_url(symbol);
        let response = http_get(&url)?;
        let json: serde_json::Value = response.json()?;
        let price = parsers::parse_binance(&json)?;

        Ok(SourcePrice {
            source_name: "binance".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub fn fetch_binance_us(symbol: &str) -> Result<SourcePrice> {
        let url = parsers::binance_us_url(symbol);
        let response = http_get(&url)?;
        let json: serde_json::Value = response.json()?;
        let price = parsers::parse_binance_us(&json)?;

        Ok(SourcePrice {
            source_name: "binance_us".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub fn fetch_binance_alpha(contract_address: &str) -> Result<SourcePrice> {
        let url = parsers::binance_alpha_url();
        // Binance API returns gzip-compressed data by default, request identity encoding
        let response = Client::new()
            .get(&url)
            .header("User-Agent", USER_AGENT)
            .header("Accept-Encoding", "identity")
            .connect_timeout(CONNECT_TIMEOUT)
            .send()?;

        let status = response.status();
        if status < 200 || status >= 300 {
            anyhow::bail!("HTTP {}", status);
        }

        let json: serde_json::Value = serde_json::from_slice(&response.body()?)?;
        let price = parsers::parse_binance_alpha(&json, contract_address)?;

        Ok(SourcePrice {
            source_name: "binance_alpha".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub fn fetch_pyth(price_id: &str) -> Result<SourcePrice> {
        let url = parsers::pyth_url(price_id);
        let response = http_get(&url)?;
        let json: serde_json::Value = response.json()?;
        let (price, publish_time) = parsers::parse_pyth(&json)?;

        // Check freshness
        let now = current_timestamp();
        if now.saturating_sub(publish_time) > parsers::PYTH_MAX_AGE_SECS {
            anyhow::bail!(
                "Pyth price is stale (published {} seconds ago)",
                now.saturating_sub(publish_time)
            );
        }

        Ok(SourcePrice {
            source_name: "pyth".to_string(),
            price,
            timestamp: publish_time,
        })
    }

    /// Try a single Chainlink RPC, returns Ok(price) or a classified error
    fn try_chainlink_rpc(
        rpc_url: &str,
        body_str: &str,
    ) -> std::result::Result<f64, ChainlinkError> {
        let response = Client::new()
            .post(rpc_url)
            .header("User-Agent", USER_AGENT)
            .header("Content-Type", "application/json")
            .connect_timeout(CONNECT_TIMEOUT)
            .body(body_str.as_bytes())
            .send()
            .map_err(|e| ChainlinkError::Rpc(anyhow::anyhow!("{}: {}", rpc_url, e)))?;

        let status = response.status();
        if status < 200 || status >= 300 {
            return Err(ChainlinkError::Rpc(anyhow::anyhow!(
                "{}: HTTP {}",
                rpc_url,
                status
            )));
        }

        let body = response
            .body()
            .map_err(|e| ChainlinkError::Rpc(anyhow::anyhow!("{}: {}", rpc_url, e)))?;

        let json: serde_json::Value = serde_json::from_slice(&body)
            .map_err(|e| ChainlinkError::Rpc(anyhow::anyhow!("{}: parse error: {}", rpc_url, e)))?;

        if let Some(error) = json.get("error") {
            let reported = anyhow::anyhow!("{}: RPC error: {}", rpc_url, error);
            return Err(if parsers::is_execution_revert(error) {
                ChainlinkError::Feed(reported)
            } else {
                ChainlinkError::Rpc(reported)
            });
        }

        // The call itself succeeded, so anything unreadable in the payload is the feed's
        // doing (missing/empty/zero answer) and will look the same on every other RPC
        parsers::parse_chainlink(&json).map_err(ChainlinkError::Feed)
    }

    pub fn fetch_chainlink(feed_address: &str) -> Result<SourcePrice> {
        if CHAINLINK_DISABLED.load(std::sync::atomic::Ordering::Relaxed) {
            anyhow::bail!("Chainlink disabled (all RPCs failed)");
        }

        let body = parsers::chainlink_request_body(feed_address);
        let body_str = serde_json::to_string(&body)?;

        // Start from the last working RPC index
        let last_idx = LAST_CHAINLINK_RPC.load(std::sync::atomic::Ordering::Relaxed);

        // Build order: start from last_idx, then cycle through the rest
        let rpcs = parsers::CHAINLINK_RPC_URLS;
        let n = rpcs.len();
        let mut errors = Vec::new();

        for i in 0..n {
            let idx = (last_idx + i) % n;
            let rpc_url = rpcs[idx];

            match try_chainlink_rpc(rpc_url, &body_str) {
                Ok(price) => {
                    // Save working RPC index
                    LAST_CHAINLINK_RPC.store(idx, std::sync::atomic::Ordering::Relaxed);
                    return Ok(SourcePrice {
                        source_name: "chainlink".to_string(),
                        price,
                        timestamp: current_timestamp(),
                    });
                }
                // The feed is dead, not the RPC: report it instead of replaying the same
                // revert against the remaining endpoints
                Err(ChainlinkError::Feed(e)) => {
                    anyhow::bail!("Chainlink feed {} unavailable: {}", feed_address, e)
                }
                Err(ChainlinkError::Rpc(e)) => {
                    eprintln!("Chainlink RPC failed: {}", e);
                    errors.push(e.to_string());
                }
            }
        }

        // All RPCs failed — disable Chainlink for this run
        CHAINLINK_DISABLED.store(true, std::sync::atomic::Ordering::Relaxed);

        anyhow::bail!(
            "All {} Chainlink RPCs failed: {}",
            n,
            errors.join("; ")
        )
    }

    pub fn fetch_huobi(symbol: &str) -> Result<SourcePrice> {
        let url = parsers::huobi_url(symbol);
        let response = http_get(&url)?;
        let json: serde_json::Value = response.json()?;
        let price = parsers::parse_huobi(&json)?;

        Ok(SourcePrice {
            source_name: "huobi".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub fn fetch_kucoin(symbol: &str) -> Result<SourcePrice> {
        let url = parsers::kucoin_url(symbol);
        let response = http_get(&url)?;
        let json: serde_json::Value = response.json()?;
        let price = parsers::parse_kucoin(&json)?;

        Ok(SourcePrice {
            source_name: "kucoin".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub fn fetch_gate(pair: &str) -> Result<SourcePrice> {
        let url = parsers::gate_url(pair);
        let response = http_get(&url)?;
        let json: serde_json::Value = response.json()?;
        let price = parsers::parse_gate(&json)?;

        Ok(SourcePrice {
            source_name: "gate".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub fn fetch_cryptocom(instrument: &str) -> Result<SourcePrice> {
        let url = parsers::cryptocom_url(instrument);
        let response = http_get(&url)?;
        let json: serde_json::Value = response.json()?;
        let price = parsers::parse_cryptocom(&json)?;

        Ok(SourcePrice {
            source_name: "cryptocom".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub fn fetch_kraken(pair: &str) -> Result<SourcePrice> {
        let url = parsers::kraken_url(pair);
        let response = http_get(&url)?;
        let json: serde_json::Value = response.json()?;
        let price = parsers::parse_kraken(&json)?;

        Ok(SourcePrice {
            source_name: "kraken".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub fn fetch_coinbase(product_id: &str) -> Result<SourcePrice> {
        let url = parsers::coinbase_url(product_id);
        let response = http_get(&url)?;
        let json: serde_json::Value = response.json()?;
        let price = parsers::parse_coinbase(&json)?;

        Ok(SourcePrice {
            source_name: "coinbase".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub fn fetch_bitstamp(pair: &str) -> Result<SourcePrice> {
        let url = parsers::bitstamp_url(pair);
        let response = http_get(&url)?;
        let json: serde_json::Value = response.json()?;
        let price = parsers::parse_bitstamp(&json)?;

        Ok(SourcePrice {
            source_name: "bitstamp".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub fn fetch_okx(inst_id: &str) -> Result<SourcePrice> {
        let url = parsers::okx_url(inst_id);
        let response = http_get(&url)?;
        let json: serde_json::Value = response.json()?;
        let price = parsers::parse_okx(&json)?;

        Ok(SourcePrice {
            source_name: "okx".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub fn fetch_bitget(symbol: &str) -> Result<SourcePrice> {
        let url = parsers::bitget_url(symbol);
        let response = http_get(&url)?;
        let json: serde_json::Value = response.json()?;
        let price = parsers::parse_bitget(&json)?;

        Ok(SourcePrice {
            source_name: "bitget".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub fn fetch_mexc(symbol: &str) -> Result<SourcePrice> {
        let url = parsers::mexc_url(symbol);
        let response = http_get(&url)?;
        let json: serde_json::Value = response.json()?;
        let price = parsers::parse_mexc(&json)?;

        Ok(SourcePrice {
            source_name: "mexc".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    /// Fetch a price from a caller-defined URL.
    ///
    /// # This function sends no credential of ours
    ///
    /// It used to attach `Authorization: Bearer $API_KEY` to every request. The URL is chosen
    /// by the caller and `API_KEY` is a credential the enclave holds on OUR behalf (CoinGecko
    /// Pro, Alchemy), so one call naming `https://attacker.tld/` handed that credential over.
    /// The twin in the `oracle-ark` binary answers this with a host allowlist; here the header
    /// is simply gone, and there is no `api_key` parameter to pass one in. A custom source
    /// that needs authentication carries its own credential in `config.headers`, where the
    /// caller is spending their own secret rather than ours.
    ///
    /// # The URL is validated here, not by the caller
    ///
    /// `security::validate_url` runs before the request is built rather than being a
    /// precondition documented for callers to honour. A "pass me a pre-validated URL" contract
    /// is only as good as every future call site remembering it, and this function is `pub`:
    /// the next caller inherits the guard instead of having to know about it. The check costs
    /// one string parse against a request that is about to cross the network anyway.
    pub fn fetch_custom(config: &CustomSourceConfig) -> Result<SourcePrice> {
        // SSRF guard: the WASI worker sits inside a TEE with its own network namespace, so an
        // unvalidated URL is a request to fetch whatever the worker can reach — link-local
        // metadata endpoints, loopback services, private ranges — and return the body.
        crate::security::validate_url(&config.url).map_err(|e| anyhow::anyhow!(e))?;

        let mut request = match config.method.to_uppercase().as_str() {
            "GET" => Client::new().get(&config.url)
            .header("User-Agent", USER_AGENT),
            "POST" => {
                let mut req = Client::new().post(&config.url)
            .header("User-Agent", USER_AGENT);
                if let Some(body) = &config.body {
                    let body_str = serde_json::to_string(body)?;
                    req = req.body(body_str.as_bytes());
                    if !config.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")) {
                        req = req.header("Content-Type", "application/json");
                    }
                }
                req
            }
            _ => anyhow::bail!("Unsupported HTTP method: {}", config.method),
        };

        // Add custom headers — the caller's own credentials, if the source needs any
        for (key, value) in &config.headers {
            request = request.header(key.as_str(), value.as_str());
        }

        let response = request.connect_timeout(CONNECT_TIMEOUT).send()?;

        let status = response.status();
        if status < 200 || status >= 300 {
            anyhow::bail!("HTTP {}", status);
        }

        let body = response.body()?;
        let json: serde_json::Value = serde_json::from_slice(&body)?;
        let value = parsers::extract_json_path(&json, &config.json_path)?;
        let price = parsers::parse_value(&value, &config.value_type)?;

        Ok(SourcePrice {
            source_name: "custom".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    /// Fetch price from all available sources for a token using exchange config
    pub fn fetch_all_sources(config: &ExchangeConfig, api_key: Option<&str>) -> Vec<SourcePrice> {
        let mut prices = Vec::new();

        if let Some(ref cg_id) = config.coingecko {
            if let Ok(p) = fetch_coingecko(cg_id, api_key) {
                prices.push(p);
            }
        }

        if let Some(ref symbol) = config.binance {
            if let Ok(p) = fetch_binance(symbol) {
                prices.push(p);
            }
        }

        if let Some(ref symbol) = config.binance_us {
            if let Ok(p) = fetch_binance_us(symbol) {
                prices.push(p);
            }
        }

        if let Some(ref address) = config.binance_alpha {
            if let Ok(p) = fetch_binance_alpha(address) {
                prices.push(p);
            }
        }

        if let Some(price_id) = config.pyth_id() {
            if let Ok(p) = fetch_pyth(price_id) {
                prices.push(p);
            }
        }

        if let Some(ref feed_address) = config.chainlink {
            if let Ok(p) = fetch_chainlink(feed_address) {
                prices.push(p);
            }
        }

        if let Some(ref symbol) = config.huobi {
            if let Ok(p) = fetch_huobi(symbol) {
                prices.push(p);
            }
        }

        if let Some(ref symbol) = config.kucoin {
            if let Ok(p) = fetch_kucoin(symbol) {
                prices.push(p);
            }
        }

        if let Some(ref pair) = config.gate {
            if let Ok(p) = fetch_gate(pair) {
                prices.push(p);
            }
        }

        if let Some(ref instrument) = config.cryptocom {
            if let Ok(p) = fetch_cryptocom(instrument) {
                prices.push(p);
            }
        }

        if let Some(ref pair) = config.kraken {
            if let Ok(p) = fetch_kraken(pair) {
                prices.push(p);
            }
        }

        if let Some(ref product_id) = config.coinbase {
            if let Ok(p) = fetch_coinbase(product_id) {
                prices.push(p);
            }
        }

        if let Some(ref pair) = config.bitstamp {
            if let Ok(p) = fetch_bitstamp(pair) {
                prices.push(p);
            }
        }

        if let Some(ref inst_id) = config.okx {
            if let Ok(p) = fetch_okx(inst_id) {
                prices.push(p);
            }
        }

        if let Some(ref symbol) = config.bitget {
            if let Ok(p) = fetch_bitget(symbol) {
                prices.push(p);
            }
        }

        if let Some(ref symbol) = config.mexc {
            if let Ok(p) = fetch_mexc(symbol) {
                prices.push(p);
            }
        }

        prices
    }

    // ------------------------------------------------------------------------
    // Batch fetching: ONE request per source for the whole token set
    // ------------------------------------------------------------------------

    /// Status Binance answers with when a single symbol in a batch is unknown
    const HTTP_BAD_REQUEST: u16 = 400;

    /// GET a batch endpoint, returning the status alongside the raw body so a caller can
    /// react to a specific status instead of only to "not 2xx".
    fn http_get_text_with_status(url: &str) -> Result<(u16, String)> {
        let response = Client::new()
            .get(url)
            .header("User-Agent", USER_AGENT)
            .connect_timeout(CONNECT_TIMEOUT)
            .send()?;

        let status = response.status();
        Ok((status, String::from_utf8(response.body()?)?))
    }

    /// GET a batch endpoint and hand the raw body to a parser.
    ///
    /// The all-ticker responses run 129-536 KB. The body is moved into the parser, which
    /// deserializes only the fields it needs, and both are dropped before the next source is
    /// requested — so a cycle never holds more than one raw response at a time.
    fn http_get_text(url: &str) -> Result<String> {
        let (status, body) = http_get_text_with_status(url)?;
        if status < 200 || status >= 300 {
            anyhow::bail!("HTTP {}", status);
        }
        Ok(body)
    }

    /// Fall back to one request per symbol, used only when a venue rejects a whole batch
    /// because of a single bad symbol. Each symbol then costs only itself.
    fn fetch_per_symbol(
        symbols: &[&str],
        fetch: fn(&str) -> Result<SourcePrice>,
    ) -> parsers::BatchPrices {
        let mut prices = parsers::BatchPrices::new();
        for symbol in symbols {
            match fetch(symbol) {
                Ok(price) => {
                    prices.insert(
                        (*symbol).to_string(),
                        parsers::BatchPrice {
                            price: price.price,
                            timestamp: None,
                        },
                    );
                }
                Err(e) => eprintln!("{} failed: {}", symbol, e),
            }
        }
        prices
    }

    pub fn fetch_coingecko_batch(
        ids: &[&str],
        api_key: Option<&str>,
    ) -> Result<parsers::BatchPrices> {
        let url = parsers::coingecko_batch_url(ids, api_key);
        let body = http_get_text(&url)?;
        parsers::parse_coingecko_batch(&body, ids)
    }

    /// Fetch several Binance symbols in one request.
    ///
    /// Binance rejects the WHOLE batch with HTTP 400 (`{"code":-1121,"msg":"Invalid
    /// symbol."}`) when one symbol is unknown, so a single stale entry in the asset config
    /// would otherwise drop Binance for every token. Only in that case do we pay for
    /// per-symbol requests, which isolates the bad symbol and keeps the good ones.
    pub fn fetch_binance_batch(symbols: &[&str]) -> Result<parsers::BatchPrices> {
        let url = parsers::binance_batch_url(symbols);
        let (status, body) = http_get_text_with_status(&url)?;
        if status == HTTP_BAD_REQUEST {
            eprintln!("Binance rejected the batch ({}), retrying per symbol", body.trim());
            return Ok(fetch_per_symbol(symbols, fetch_binance));
        }
        if status < 200 || status >= 300 {
            anyhow::bail!("HTTP {}", status);
        }
        parsers::parse_binance_batch(&body, symbols)
    }

    /// Fetch several Binance US symbols in one request (same invalid-symbol rule as Binance)
    pub fn fetch_binance_us_batch(symbols: &[&str]) -> Result<parsers::BatchPrices> {
        let url = parsers::binance_us_batch_url(symbols);
        let (status, body) = http_get_text_with_status(&url)?;
        if status == HTTP_BAD_REQUEST {
            eprintln!("Binance US rejected the batch ({}), retrying per symbol", body.trim());
            return Ok(fetch_per_symbol(symbols, fetch_binance_us));
        }
        if status < 200 || status >= 300 {
            anyhow::bail!("HTTP {}", status);
        }
        parsers::parse_binance_batch(&body, symbols)
    }

    /// Match several contract addresses against one Binance Alpha listing.
    /// Binance serves this endpoint gzipped by default, hence the identity encoding.
    pub fn fetch_binance_alpha_batch(
        contract_addresses: &[&str],
    ) -> Result<parsers::BatchPrices> {
        let url = parsers::binance_alpha_url();
        let response = Client::new()
            .get(&url)
            .header("User-Agent", USER_AGENT)
            .header("Accept-Encoding", "identity")
            .connect_timeout(CONNECT_TIMEOUT)
            .send()?;

        let status = response.status();
        if status < 200 || status >= 300 {
            anyhow::bail!("HTTP {}", status);
        }

        let body = String::from_utf8(response.body()?)?;
        parsers::parse_binance_alpha_batch(&body, contract_addresses)
    }

    pub fn fetch_pyth_batch(price_ids: &[&str]) -> Result<parsers::BatchPrices> {
        let url = parsers::pyth_batch_url(price_ids);
        let body = http_get_text(&url)?;
        parsers::parse_pyth_batch(&body, price_ids)
    }

    /// Try one RPC with the Multicall3 body
    fn try_chainlink_multicall(
        rpc_url: &str,
        body_str: &str,
        feed_addresses: &[&str],
    ) -> Result<parsers::ChainlinkBatch> {
        let response = Client::new()
            .post(rpc_url)
            .header("User-Agent", USER_AGENT)
            .header("Content-Type", "application/json")
            .connect_timeout(CONNECT_TIMEOUT)
            .body(body_str.as_bytes())
            .send()
            .map_err(|e| anyhow::anyhow!("{}: {}", rpc_url, e))?;

        let status = response.status();
        if status < 200 || status >= 300 {
            anyhow::bail!("{}: HTTP {}", rpc_url, status);
        }

        let json: serde_json::Value = serde_json::from_slice(&response.body()?)
            .map_err(|e| anyhow::anyhow!("{}: parse error: {}", rpc_url, e))?;

        parsers::parse_chainlink_multicall(&json, feed_addresses)
            .map_err(|e| anyhow::anyhow!("{}: {}", rpc_url, e))
    }

    /// Read every configured Chainlink feed in ONE eth_call through Multicall3.
    ///
    /// Besides collapsing N round-trips into one, this fixes the way a delisted feed used to
    /// cost seven requests: the revert repeats on every RPC, so the single-feed path rotated
    /// through the whole list before giving up. With `allowFailure = true` a dead feed comes
    /// back as a failed inner call while the rest still price.
    pub fn fetch_chainlink_batch(feed_addresses: &[&str]) -> Result<parsers::BatchPrices> {
        if CHAINLINK_DISABLED.load(std::sync::atomic::Ordering::Relaxed) {
            anyhow::bail!("Chainlink disabled (all RPCs failed)");
        }

        let body = parsers::chainlink_multicall_body(feed_addresses)?;
        let body_str = serde_json::to_string(&body)?;

        // Start from the last working RPC index, then cycle through the rest
        let last_idx = LAST_CHAINLINK_RPC.load(std::sync::atomic::Ordering::Relaxed);
        let rpcs = parsers::CHAINLINK_RPC_URLS;
        let n = rpcs.len();
        let mut errors = Vec::new();

        for i in 0..n {
            let idx = (last_idx + i) % n;
            let rpc_url = rpcs[idx];

            match try_chainlink_multicall(rpc_url, &body_str, feed_addresses) {
                Ok(batch) => {
                    LAST_CHAINLINK_RPC.store(idx, std::sync::atomic::Ordering::Relaxed);
                    for (address, reason) in &batch.failures {
                        eprintln!("Chainlink feed {} unavailable: {}", address, reason);
                    }
                    return Ok(batch.prices);
                }
                Err(e) => {
                    eprintln!("Chainlink RPC failed: {}", e);
                    errors.push(e.to_string());
                }
            }
        }

        // All RPCs failed — disable Chainlink for this run
        CHAINLINK_DISABLED.store(true, std::sync::atomic::Ordering::Relaxed);

        anyhow::bail!("All {} Chainlink RPCs failed: {}", n, errors.join("; "))
    }

    pub fn fetch_huobi_batch(symbols: &[&str]) -> Result<parsers::BatchPrices> {
        let body = http_get_text(&parsers::huobi_batch_url())?;
        parsers::parse_huobi_batch(&body, symbols)
    }

    pub fn fetch_kucoin_batch(symbols: &[&str]) -> Result<parsers::BatchPrices> {
        let body = http_get_text(&parsers::kucoin_batch_url())?;
        parsers::parse_kucoin_batch(&body, symbols)
    }

    pub fn fetch_gate_batch(pairs: &[&str]) -> Result<parsers::BatchPrices> {
        let body = http_get_text(&parsers::gate_batch_url())?;
        parsers::parse_gate_batch(&body, pairs)
    }

    pub fn fetch_cryptocom_batch(instruments: &[&str]) -> Result<parsers::BatchPrices> {
        let body = http_get_text(&parsers::cryptocom_batch_url())?;
        parsers::parse_cryptocom_batch(&body, instruments)
    }

    /// Fetch several Kraken pairs in one request.
    ///
    /// Kraken drops the WHOLE batch when a single pair is unknown — HTTP 200 with
    /// `{"error":["EQuery:Unknown asset pair"],"result":{}}` — so one stale entry in the
    /// asset config would otherwise cost us Kraken for every token. Same isolation as
    /// Binance's HTTP 400 path, but keyed off the error body since the status stays 200.
    pub fn fetch_kraken_batch(pairs: &[&str]) -> Result<parsers::BatchPrices> {
        let url = parsers::kraken_batch_url(pairs);
        let body = http_get_text(&url)?;
        if parsers::is_kraken_unknown_pair(&body) {
            eprintln!("Kraken rejected the batch ({}), retrying per pair", body.trim());
            return Ok(fetch_per_symbol(pairs, fetch_kraken));
        }
        parsers::parse_kraken_batch(&body, pairs)
    }

    pub fn fetch_coinbase_batch(product_ids: &[&str]) -> Result<parsers::BatchPrices> {
        let body = http_get_text(&parsers::coinbase_batch_url())?;
        parsers::parse_coinbase_batch(&body, product_ids)
    }

    pub fn fetch_bitstamp_batch(pairs: &[&str]) -> Result<parsers::BatchPrices> {
        let body = http_get_text(&parsers::bitstamp_batch_url())?;
        parsers::parse_bitstamp_batch(&body, pairs)
    }

    pub fn fetch_okx_batch(inst_ids: &[&str]) -> Result<parsers::BatchPrices> {
        let body = http_get_text(&parsers::okx_batch_url())?;
        parsers::parse_okx_batch(&body, inst_ids)
    }

    pub fn fetch_bitget_batch(symbols: &[&str]) -> Result<parsers::BatchPrices> {
        let body = http_get_text(&parsers::bitget_batch_url())?;
        parsers::parse_bitget_batch(&body, symbols)
    }

    pub fn fetch_mexc_batch(symbols: &[&str]) -> Result<parsers::BatchPrices> {
        let body = http_get_text(&parsers::mexc_batch_url())?;
        parsers::parse_mexc_batch(&body, symbols)
    }

    /// Fetch every configured source for a whole set of tokens with ONE request per source.
    ///
    /// The per-token path issues `tokens x sources` requests — ~66 for the 11 assets we
    /// publish — and that sequential round-trip count dominates the cycle: measured from the
    /// production worker host, 31.3s against 4.2s for the batched form.
    ///
    /// Semantics are unchanged from `fetch_all_sources`: sources are queried in the same
    /// order, a source that fails contributes nothing and never aborts the others, and
    /// `SourcePrice.timestamp` is the fetch time except for Pyth, where it is the feed's own
    /// `publish_time` and the 120s staleness rule still applies per feed.
    ///
    /// Returns an entry for every token in `configs`, empty when nothing priced.
    pub fn fetch_all_sources_batch(
        configs: &HashMap<String, ExchangeConfig>,
        api_key: Option<&str>,
    ) -> HashMap<String, Vec<SourcePrice>> {
        let mut out: HashMap<String, Vec<SourcePrice>> = configs
            .keys()
            .map(|token| (token.clone(), Vec::new()))
            .collect();

        // Fetch time, evaluated per price, for the sources that report no timestamp of their
        // own — the same value `fetch_all_sources` stores today
        let fetched_now = |_: &str, _: &parsers::BatchPrice| Some(current_timestamp());

        let coingecko = index_symbols(configs, |c| c.coingecko.as_deref());
        if !coingecko.is_empty() {
            let ids: Vec<&str> = coingecko.keys().copied().collect();
            match fetch_coingecko_batch(&ids, api_key) {
                Ok(prices) => fan_out(&mut out, &coingecko, &prices, "coingecko", fetched_now),
                Err(e) => eprintln!("coingecko batch failed: {}", e),
            }
        }

        let binance = index_symbols(configs, |c| c.binance.as_deref());
        if !binance.is_empty() {
            let symbols: Vec<&str> = binance.keys().copied().collect();
            match fetch_binance_batch(&symbols) {
                Ok(prices) => fan_out(&mut out, &binance, &prices, "binance", fetched_now),
                Err(e) => eprintln!("binance batch failed: {}", e),
            }
        }

        let binance_us = index_symbols(configs, |c| c.binance_us.as_deref());
        if !binance_us.is_empty() {
            let symbols: Vec<&str> = binance_us.keys().copied().collect();
            match fetch_binance_us_batch(&symbols) {
                Ok(prices) => fan_out(&mut out, &binance_us, &prices, "binance_us", fetched_now),
                Err(e) => eprintln!("binance_us batch failed: {}", e),
            }
        }

        let binance_alpha = index_symbols(configs, |c| c.binance_alpha.as_deref());
        if !binance_alpha.is_empty() {
            let addresses: Vec<&str> = binance_alpha.keys().copied().collect();
            match fetch_binance_alpha_batch(&addresses) {
                Ok(prices) => {
                    fan_out(&mut out, &binance_alpha, &prices, "binance_alpha", fetched_now)
                }
                Err(e) => eprintln!("binance_alpha batch failed: {}", e),
            }
        }

        let pyth = index_symbols(configs, |c| c.pyth_id());
        if !pyth.is_empty() {
            let price_ids: Vec<&str> = pyth.keys().copied().collect();
            match fetch_pyth_batch(&price_ids) {
                // Same per-feed freshness rule as the single-feed path
                Ok(prices) => fan_out(&mut out, &pyth, &prices, "pyth", pyth_publish_time),
                Err(e) => eprintln!("pyth batch failed: {}", e),
            }
        }

        let chainlink = index_symbols(configs, |c| c.chainlink.as_deref());
        if !chainlink.is_empty() {
            let feeds: Vec<&str> = chainlink.keys().copied().collect();
            match fetch_chainlink_batch(&feeds) {
                Ok(prices) => fan_out(&mut out, &chainlink, &prices, "chainlink", fetched_now),
                Err(e) => eprintln!("chainlink batch failed: {}", e),
            }
        }

        let huobi = index_symbols(configs, |c| c.huobi.as_deref());
        if !huobi.is_empty() {
            let symbols: Vec<&str> = huobi.keys().copied().collect();
            match fetch_huobi_batch(&symbols) {
                Ok(prices) => fan_out(&mut out, &huobi, &prices, "huobi", fetched_now),
                Err(e) => eprintln!("huobi batch failed: {}", e),
            }
        }

        let kucoin = index_symbols(configs, |c| c.kucoin.as_deref());
        if !kucoin.is_empty() {
            let symbols: Vec<&str> = kucoin.keys().copied().collect();
            match fetch_kucoin_batch(&symbols) {
                Ok(prices) => fan_out(&mut out, &kucoin, &prices, "kucoin", fetched_now),
                Err(e) => eprintln!("kucoin batch failed: {}", e),
            }
        }

        let gate = index_symbols(configs, |c| c.gate.as_deref());
        if !gate.is_empty() {
            let pairs: Vec<&str> = gate.keys().copied().collect();
            match fetch_gate_batch(&pairs) {
                Ok(prices) => fan_out(&mut out, &gate, &prices, "gate", fetched_now),
                Err(e) => eprintln!("gate batch failed: {}", e),
            }
        }

        let cryptocom = index_symbols(configs, |c| c.cryptocom.as_deref());
        if !cryptocom.is_empty() {
            let instruments: Vec<&str> = cryptocom.keys().copied().collect();
            match fetch_cryptocom_batch(&instruments) {
                Ok(prices) => fan_out(&mut out, &cryptocom, &prices, "cryptocom", fetched_now),
                Err(e) => eprintln!("cryptocom batch failed: {}", e),
            }
        }

        let kraken = index_symbols(configs, |c| c.kraken.as_deref());
        if !kraken.is_empty() {
            let pairs: Vec<&str> = kraken.keys().copied().collect();
            match fetch_kraken_batch(&pairs) {
                Ok(prices) => fan_out(&mut out, &kraken, &prices, "kraken", fetched_now),
                Err(e) => eprintln!("kraken batch failed: {}", e),
            }
        }

        let coinbase = index_symbols(configs, |c| c.coinbase.as_deref());
        if !coinbase.is_empty() {
            let product_ids: Vec<&str> = coinbase.keys().copied().collect();
            match fetch_coinbase_batch(&product_ids) {
                Ok(prices) => fan_out(&mut out, &coinbase, &prices, "coinbase", fetched_now),
                Err(e) => eprintln!("coinbase batch failed: {}", e),
            }
        }

        let bitstamp = index_symbols(configs, |c| c.bitstamp.as_deref());
        if !bitstamp.is_empty() {
            let pairs: Vec<&str> = bitstamp.keys().copied().collect();
            match fetch_bitstamp_batch(&pairs) {
                Ok(prices) => fan_out(&mut out, &bitstamp, &prices, "bitstamp", fetched_now),
                Err(e) => eprintln!("bitstamp batch failed: {}", e),
            }
        }

        let okx = index_symbols(configs, |c| c.okx.as_deref());
        if !okx.is_empty() {
            let inst_ids: Vec<&str> = okx.keys().copied().collect();
            match fetch_okx_batch(&inst_ids) {
                Ok(prices) => fan_out(&mut out, &okx, &prices, "okx", fetched_now),
                Err(e) => eprintln!("okx batch failed: {}", e),
            }
        }

        let bitget = index_symbols(configs, |c| c.bitget.as_deref());
        if !bitget.is_empty() {
            let symbols: Vec<&str> = bitget.keys().copied().collect();
            match fetch_bitget_batch(&symbols) {
                Ok(prices) => fan_out(&mut out, &bitget, &prices, "bitget", fetched_now),
                Err(e) => eprintln!("bitget batch failed: {}", e),
            }
        }

        let mexc = index_symbols(configs, |c| c.mexc.as_deref());
        if !mexc.is_empty() {
            let symbols: Vec<&str> = mexc.keys().copied().collect();
            match fetch_mexc_batch(&symbols) {
                Ok(prices) => fan_out(&mut out, &mexc, &prices, "mexc", fetched_now),
                Err(e) => eprintln!("mexc batch failed: {}", e),
            }
        }

        out
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn custom(url: &str) -> CustomSourceConfig {
            CustomSourceConfig {
                url: url.to_string(),
                json_path: "price".to_string(),
                value_type: "number".to_string(),
                method: "GET".to_string(),
                headers: Vec::new(),
                body: None,
            }
        }

        /// `fetch_custom` refuses a blocked destination itself, before it builds a request.
        ///
        /// The guard used to live only in the `oracle-ark` binary's copy of this code, so this
        /// `pub fn` would have connected to any of these. No network is touched by this test:
        /// validation runs first and returns, which is exactly the property being pinned.
        ///
        /// There is no assertion about the API key here because there is nothing left to
        /// assert — the parameter that carried it is gone from the signature, so no caller can
        /// hand this function a credential of ours to leak.
        #[test]
        fn fetch_custom_refuses_internal_targets() {
            for url in [
                "http://169.254.169.254/latest/meta-data/",
                "http://127.0.0.1:8081/",
                "http://10.1.2.3/internal",
                "file:///etc/passwd",
                "http://api.coingecko.com@127.0.0.1/x",
            ] {
                let error = fetch_custom(&custom(url))
                    .expect_err(&format!("{} must be refused", url))
                    .to_string();
                assert!(
                    error.contains("blocked"),
                    "{} failed with '{}' instead of the SSRF guard",
                    url,
                    error
                );
            }

            // A malformed URL is refused too, rather than being handed to the HTTP client
            assert!(fetch_custom(&custom("not-a-url")).is_err());
        }
    }
}

// ============================================================================
// Async (scheduler) implementation
// ============================================================================

#[cfg(feature = "async")]
pub mod r#async {
    use super::*;
    use crate::{CHAINLINK_DISABLED, LAST_CHAINLINK_RPC};

    pub async fn fetch_coingecko(
        client: &reqwest::Client,
        token_id: &str,
        api_key: Option<&str>,
    ) -> Result<SourcePrice> {
        let url = parsers::coingecko_url(token_id, api_key);
        let response = client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP {}", response.status());
        }

        let json: serde_json::Value = response.json().await?;
        let price = parsers::parse_coingecko(&json, token_id)?;

        Ok(SourcePrice {
            source_name: "coingecko".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub async fn fetch_binance(client: &reqwest::Client, symbol: &str) -> Result<SourcePrice> {
        let url = parsers::binance_url(symbol);
        let response = client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP {}", response.status());
        }

        let json: serde_json::Value = response.json().await?;
        let price = parsers::parse_binance(&json)?;

        Ok(SourcePrice {
            source_name: "binance".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub async fn fetch_binance_us(client: &reqwest::Client, symbol: &str) -> Result<SourcePrice> {
        let url = parsers::binance_us_url(symbol);
        let response = client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP {}", response.status());
        }

        let json: serde_json::Value = response.json().await?;
        let price = parsers::parse_binance_us(&json)?;

        Ok(SourcePrice {
            source_name: "binance_us".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub async fn fetch_binance_alpha(client: &reqwest::Client, contract_address: &str) -> Result<SourcePrice> {
        let url = parsers::binance_alpha_url();
        let response = client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP {}", response.status());
        }

        let json: serde_json::Value = response.json().await?;
        let price = parsers::parse_binance_alpha(&json, contract_address)?;

        Ok(SourcePrice {
            source_name: "binance_alpha".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub async fn fetch_pyth(client: &reqwest::Client, price_id: &str) -> Result<SourcePrice> {
        let url = parsers::pyth_url(price_id);
        let response = client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP {}", response.status());
        }

        let json: serde_json::Value = response.json().await?;
        let (price, publish_time) = parsers::parse_pyth(&json)?;

        let now = current_timestamp();
        if now.saturating_sub(publish_time) > parsers::PYTH_MAX_AGE_SECS {
            anyhow::bail!(
                "Pyth price is stale (published {} seconds ago)",
                now.saturating_sub(publish_time)
            );
        }

        Ok(SourcePrice {
            source_name: "pyth".to_string(),
            price,
            timestamp: publish_time,
        })
    }

    /// Try a single Chainlink RPC (async), returns Ok(price) or a classified error
    async fn try_chainlink_rpc_async(
        client: &reqwest::Client,
        rpc_url: &str,
        body: &serde_json::Value,
    ) -> std::result::Result<f64, ChainlinkError> {
        let response = client
            .post(rpc_url)
            .json(body)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| ChainlinkError::Rpc(anyhow::anyhow!("{}: {}", rpc_url, e)))?;

        if !response.status().is_success() {
            return Err(ChainlinkError::Rpc(anyhow::anyhow!(
                "{}: HTTP {}",
                rpc_url,
                response.status()
            )));
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ChainlinkError::Rpc(anyhow::anyhow!("{}: parse error: {}", rpc_url, e)))?;

        if let Some(error) = json.get("error") {
            let reported = anyhow::anyhow!("{}: RPC error: {}", rpc_url, error);
            return Err(if parsers::is_execution_revert(error) {
                ChainlinkError::Feed(reported)
            } else {
                ChainlinkError::Rpc(reported)
            });
        }

        // The call itself succeeded, so anything unreadable in the payload is the feed's
        // doing (missing/empty/zero answer) and will look the same on every other RPC
        parsers::parse_chainlink(&json).map_err(ChainlinkError::Feed)
    }

    pub async fn fetch_chainlink(client: &reqwest::Client, feed_address: &str) -> Result<SourcePrice> {
        use crate::{CHAINLINK_DISABLED, LAST_CHAINLINK_RPC};
        use std::sync::atomic::Ordering;

        if CHAINLINK_DISABLED.load(Ordering::Relaxed) {
            anyhow::bail!("Chainlink disabled (all RPCs failed)");
        }

        let body = parsers::chainlink_request_body(feed_address);
        let last_idx = LAST_CHAINLINK_RPC.load(Ordering::Relaxed);
        let rpcs = parsers::CHAINLINK_RPC_URLS;
        let n = rpcs.len();
        let mut errors = Vec::new();

        for i in 0..n {
            let idx = (last_idx + i) % n;
            let rpc_url = rpcs[idx];

            match try_chainlink_rpc_async(client, rpc_url, &body).await {
                Ok(price) => {
                    LAST_CHAINLINK_RPC.store(idx, Ordering::Relaxed);
                    return Ok(SourcePrice {
                        source_name: "chainlink".to_string(),
                        price,
                        timestamp: current_timestamp(),
                    });
                }
                // The feed is dead, not the RPC: report it instead of replaying the same
                // revert against the remaining endpoints
                Err(ChainlinkError::Feed(e)) => {
                    anyhow::bail!("Chainlink feed {} unavailable: {}", feed_address, e)
                }
                Err(ChainlinkError::Rpc(e)) => {
                    eprintln!("Chainlink RPC failed: {}", e);
                    errors.push(e.to_string());
                }
            }
        }

        CHAINLINK_DISABLED.store(true, Ordering::Relaxed);

        anyhow::bail!(
            "All {} Chainlink RPCs failed: {}",
            n,
            errors.join("; ")
        )
    }

    pub async fn fetch_huobi(client: &reqwest::Client, symbol: &str) -> Result<SourcePrice> {
        let url = parsers::huobi_url(symbol);
        let response = client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP {}", response.status());
        }

        let json: serde_json::Value = response.json().await?;
        let price = parsers::parse_huobi(&json)?;

        Ok(SourcePrice {
            source_name: "huobi".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub async fn fetch_kucoin(client: &reqwest::Client, symbol: &str) -> Result<SourcePrice> {
        let url = parsers::kucoin_url(symbol);
        let response = client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP {}", response.status());
        }

        let json: serde_json::Value = response.json().await?;
        let price = parsers::parse_kucoin(&json)?;

        Ok(SourcePrice {
            source_name: "kucoin".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub async fn fetch_gate(client: &reqwest::Client, pair: &str) -> Result<SourcePrice> {
        let url = parsers::gate_url(pair);
        let response = client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP {}", response.status());
        }

        let json: serde_json::Value = response.json().await?;
        let price = parsers::parse_gate(&json)?;

        Ok(SourcePrice {
            source_name: "gate".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub async fn fetch_cryptocom(client: &reqwest::Client, instrument: &str) -> Result<SourcePrice> {
        let url = parsers::cryptocom_url(instrument);
        let response = client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP {}", response.status());
        }

        let json: serde_json::Value = response.json().await?;
        let price = parsers::parse_cryptocom(&json)?;

        Ok(SourcePrice {
            source_name: "cryptocom".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub async fn fetch_kraken(client: &reqwest::Client, pair: &str) -> Result<SourcePrice> {
        let url = parsers::kraken_url(pair);
        let response = client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP {}", response.status());
        }

        let json: serde_json::Value = response.json().await?;
        let price = parsers::parse_kraken(&json)?;

        Ok(SourcePrice {
            source_name: "kraken".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub async fn fetch_coinbase(client: &reqwest::Client, product_id: &str) -> Result<SourcePrice> {
        let url = parsers::coinbase_url(product_id);
        let response = client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP {}", response.status());
        }

        let json: serde_json::Value = response.json().await?;
        let price = parsers::parse_coinbase(&json)?;

        Ok(SourcePrice {
            source_name: "coinbase".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub async fn fetch_bitstamp(client: &reqwest::Client, pair: &str) -> Result<SourcePrice> {
        let url = parsers::bitstamp_url(pair);
        let response = client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP {}", response.status());
        }

        let json: serde_json::Value = response.json().await?;
        let price = parsers::parse_bitstamp(&json)?;

        Ok(SourcePrice {
            source_name: "bitstamp".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub async fn fetch_okx(client: &reqwest::Client, inst_id: &str) -> Result<SourcePrice> {
        let url = parsers::okx_url(inst_id);
        let response = client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP {}", response.status());
        }

        let json: serde_json::Value = response.json().await?;
        let price = parsers::parse_okx(&json)?;

        Ok(SourcePrice {
            source_name: "okx".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub async fn fetch_bitget(client: &reqwest::Client, symbol: &str) -> Result<SourcePrice> {
        let url = parsers::bitget_url(symbol);
        let response = client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP {}", response.status());
        }

        let json: serde_json::Value = response.json().await?;
        let price = parsers::parse_bitget(&json)?;

        Ok(SourcePrice {
            source_name: "bitget".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    pub async fn fetch_mexc(client: &reqwest::Client, symbol: &str) -> Result<SourcePrice> {
        let url = parsers::mexc_url(symbol);
        let response = client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP {}", response.status());
        }

        let json: serde_json::Value = response.json().await?;
        let price = parsers::parse_mexc(&json)?;

        Ok(SourcePrice {
            source_name: "mexc".to_string(),
            price,
            timestamp: current_timestamp(),
        })
    }

    /// Fetch price from all available sources for a token using exchange config
    pub async fn fetch_all_sources(
        client: &reqwest::Client,
        config: &ExchangeConfig,
        api_key: Option<&str>,
    ) -> Vec<SourcePrice> {
        let mut prices = Vec::new();

        if let Some(ref cg_id) = config.coingecko {
            if let Ok(p) = fetch_coingecko(client, cg_id, api_key).await {
                prices.push(p);
            }
        }

        if let Some(ref symbol) = config.binance {
            if let Ok(p) = fetch_binance(client, symbol).await {
                prices.push(p);
            }
        }

        if let Some(ref symbol) = config.binance_us {
            if let Ok(p) = fetch_binance_us(client, symbol).await {
                prices.push(p);
            }
        }

        if let Some(ref address) = config.binance_alpha {
            if let Ok(p) = fetch_binance_alpha(client, address).await {
                prices.push(p);
            }
        }

        if let Some(price_id) = config.pyth_id() {
            if let Ok(p) = fetch_pyth(client, price_id).await {
                prices.push(p);
            }
        }

        if let Some(ref feed_address) = config.chainlink {
            if let Ok(p) = fetch_chainlink(client, feed_address).await {
                prices.push(p);
            }
        }

        if let Some(ref symbol) = config.huobi {
            if let Ok(p) = fetch_huobi(client, symbol).await {
                prices.push(p);
            }
        }

        if let Some(ref symbol) = config.kucoin {
            if let Ok(p) = fetch_kucoin(client, symbol).await {
                prices.push(p);
            }
        }

        if let Some(ref pair) = config.gate {
            if let Ok(p) = fetch_gate(client, pair).await {
                prices.push(p);
            }
        }

        if let Some(ref instrument) = config.cryptocom {
            if let Ok(p) = fetch_cryptocom(client, instrument).await {
                prices.push(p);
            }
        }

        if let Some(ref pair) = config.kraken {
            if let Ok(p) = fetch_kraken(client, pair).await {
                prices.push(p);
            }
        }

        if let Some(ref product_id) = config.coinbase {
            if let Ok(p) = fetch_coinbase(client, product_id).await {
                prices.push(p);
            }
        }

        if let Some(ref pair) = config.bitstamp {
            if let Ok(p) = fetch_bitstamp(client, pair).await {
                prices.push(p);
            }
        }

        if let Some(ref inst_id) = config.okx {
            if let Ok(p) = fetch_okx(client, inst_id).await {
                prices.push(p);
            }
        }

        if let Some(ref symbol) = config.bitget {
            if let Ok(p) = fetch_bitget(client, symbol).await {
                prices.push(p);
            }
        }

        if let Some(ref symbol) = config.mexc {
            if let Ok(p) = fetch_mexc(client, symbol).await {
                prices.push(p);
            }
        }

        prices
    }

    // ------------------------------------------------------------------------
    // Batch fetching: ONE request per source for the whole token set
    // ------------------------------------------------------------------------

    /// Status Binance answers with when a single symbol in a batch is unknown
    const HTTP_BAD_REQUEST: u16 = 400;

    async fn http_get_text_with_status(client: &reqwest::Client, url: &str) -> Result<(u16, String)> {
        let response = client.get(url).send().await?;
        let status = response.status().as_u16();
        Ok((status, response.text().await?))
    }

    async fn http_get_text(client: &reqwest::Client, url: &str) -> Result<String> {
        let (status, body) = http_get_text_with_status(client, url).await?;
        if !(200..300).contains(&status) {
            anyhow::bail!("HTTP {}", status);
        }
        Ok(body)
    }

    /// Fall back to one request per symbol when a venue rejects a whole batch over a single bad
    /// symbol — the same isolation the WASI side does, so one stale entry in the asset config
    /// cannot drop the venue for every token.
    async fn fetch_per_symbol<'a, F, Fut>(symbols: &[&'a str], fetch: F) -> parsers::BatchPrices
    where
        F: Fn(&'a str) -> Fut,
        Fut: std::future::Future<Output = Result<SourcePrice>>,
    {
        let mut prices = parsers::BatchPrices::new();
        for symbol in symbols {
            match fetch(symbol).await {
                Ok(price) => {
                    prices.insert(
                        symbol.to_string(),
                        parsers::BatchPrice {
                            price: price.price,
                            timestamp: None,
                        },
                    );
                }
                Err(e) => eprintln!("{} failed: {}", symbol, e),
            }
        }
        prices
    }

    async fn fetch_chainlink_batch(
        client: &reqwest::Client,
        feed_addresses: &[&str],
    ) -> Result<parsers::BatchPrices> {
        if CHAINLINK_DISABLED.load(std::sync::atomic::Ordering::Relaxed) {
            anyhow::bail!("Chainlink disabled (all RPCs failed)");
        }

        let body = parsers::chainlink_multicall_body(feed_addresses)?;
        let body_str = serde_json::to_string(&body)?;

        // Start from the last working RPC index, then cycle through the rest
        let last_idx = LAST_CHAINLINK_RPC.load(std::sync::atomic::Ordering::Relaxed);
        let rpcs = parsers::CHAINLINK_RPC_URLS;
        let mut errors = Vec::new();

        for i in 0..rpcs.len() {
            let idx = (last_idx + i) % rpcs.len();
            let rpc_url = rpcs[idx];

            let attempt = async {
                let response = client
                    .post(rpc_url)
                    .header("Content-Type", "application/json")
                    .body(body_str.clone())
                    .send()
                    .await?;
                if !response.status().is_success() {
                    anyhow::bail!("{}: HTTP {}", rpc_url, response.status());
                }
                let json: serde_json::Value = response.json().await?;
                parsers::parse_chainlink_multicall(&json, feed_addresses)
                    .map_err(|e| anyhow::anyhow!("{}: {}", rpc_url, e))
            }
            .await;

            match attempt {
                Ok(batch) => {
                    LAST_CHAINLINK_RPC.store(idx, std::sync::atomic::Ordering::Relaxed);
                    for (address, reason) in &batch.failures {
                        eprintln!("Chainlink feed {} unavailable: {}", address, reason);
                    }
                    return Ok(batch.prices);
                }
                Err(e) => {
                    eprintln!("Chainlink RPC failed: {}", e);
                    errors.push(e.to_string());
                }
            }
        }

        CHAINLINK_DISABLED.store(true, std::sync::atomic::Ordering::Relaxed);
        anyhow::bail!("all Chainlink RPCs failed: {}", errors.join("; "))
    }

    /// Fetch every configured source for a whole set of tokens with ONE request per source.
    ///
    /// The scheduler used to call `fetch_all_sources` per token purely to decide whether
    /// anything moved: 182 requests per poll for our 18 assets, against ~16 for the refresh
    /// those requests were guarding. That put Kraken and Bitstamp permanently over their rate
    /// limits, and the resulting gaps changed the scheduler's median without changing the
    /// worker's — which is exactly what makes a deviation trigger fire forever.
    ///
    /// Semantics match the WASI batch: sources are queried in the same order, a source that
    /// fails contributes nothing and never aborts the others, and `SourcePrice.timestamp` is
    /// the fetch time except for Pyth, where it is the feed's own `publish_time`.
    pub async fn fetch_all_sources_batch(
        client: &reqwest::Client,
        configs: &std::collections::HashMap<String, ExchangeConfig>,
        api_key: Option<&str>,
    ) -> std::collections::HashMap<String, Vec<SourcePrice>> {
        let mut out: std::collections::HashMap<String, Vec<SourcePrice>> = configs
            .keys()
            .map(|token| (token.clone(), Vec::new()))
            .collect();

        let fetched_now = |_: &str, _: &parsers::BatchPrice| Some(current_timestamp());

        /// One venue whose whole batch is a single GET: index the symbols, run `$body` with
        /// them in scope, fan the answer back out. A failing venue only logs — it must never
        /// abort the ones after it.
        macro_rules! venue {
            ($field:expr, $name:literal, $symbols:ident => $body:expr) => {{
                let index = index_symbols(configs, $field);
                if !index.is_empty() {
                    let $symbols: Vec<&str> = index.keys().copied().collect();
                    let result: Result<parsers::BatchPrices> = async { $body }.await;
                    match result {
                        Ok(prices) => fan_out(&mut out, &index, &prices, $name, fetched_now),
                        Err(e) => eprintln!("{} batch failed: {}", $name, e),
                    }
                }
            }};
        }

        let coingecko = index_symbols(configs, |c: &ExchangeConfig| c.coingecko.as_deref());
        if !coingecko.is_empty() {
            let ids: Vec<&str> = coingecko.keys().copied().collect();
            match http_get_text(client, &parsers::coingecko_batch_url(&ids, api_key)).await {
                Ok(body) => match parsers::parse_coingecko_batch(&body, &ids) {
                    Ok(prices) => fan_out(&mut out, &coingecko, &prices, "coingecko", fetched_now),
                    Err(e) => eprintln!("coingecko batch failed: {}", e),
                },
                Err(e) => eprintln!("coingecko batch failed: {}", e),
            }
        }

        // Binance rejects the WHOLE batch when one symbol is unknown; only then pay per symbol
        let binance = index_symbols(configs, |c: &ExchangeConfig| c.binance.as_deref());
        if !binance.is_empty() {
            let symbols: Vec<&str> = binance.keys().copied().collect();
            let url = parsers::binance_batch_url(&symbols);
            match http_get_text_with_status(client, &url).await {
                Ok((status, body)) => {
                    let parsed = if status == HTTP_BAD_REQUEST {
                        eprintln!("Binance rejected the batch ({}), retrying per symbol", body.trim());
                        Ok(fetch_per_symbol(&symbols, |s| fetch_binance(client, s)).await)
                    } else if !(200..300).contains(&status) {
                        Err(anyhow::anyhow!("HTTP {}", status))
                    } else {
                        parsers::parse_binance_batch(&body, &symbols)
                    };
                    match parsed {
                        Ok(prices) => fan_out(&mut out, &binance, &prices, "binance", fetched_now),
                        Err(e) => eprintln!("binance batch failed: {}", e),
                    }
                }
                Err(e) => eprintln!("binance batch failed: {}", e),
            }
        }

        let binance_us = index_symbols(configs, |c: &ExchangeConfig| c.binance_us.as_deref());
        if !binance_us.is_empty() {
            let symbols: Vec<&str> = binance_us.keys().copied().collect();
            let url = parsers::binance_us_batch_url(&symbols);
            match http_get_text_with_status(client, &url).await {
                Ok((status, body)) => {
                    let parsed = if status == HTTP_BAD_REQUEST {
                        eprintln!("Binance.US rejected the batch ({}), retrying per symbol", body.trim());
                        Ok(fetch_per_symbol(&symbols, |s| fetch_binance_us(client, s)).await)
                    } else if !(200..300).contains(&status) {
                        Err(anyhow::anyhow!("HTTP {}", status))
                    } else {
                        parsers::parse_binance_batch(&body, &symbols)
                    };
                    match parsed {
                        Ok(prices) => {
                            fan_out(&mut out, &binance_us, &prices, "binance_us", fetched_now)
                        }
                        Err(e) => eprintln!("binance_us batch failed: {}", e),
                    }
                }
                Err(e) => eprintln!("binance_us batch failed: {}", e),
            }
        }

        // Binance serves the Alpha listing gzipped by default, hence the identity encoding
        venue!(
            |c: &ExchangeConfig| c.binance_alpha.as_deref(),
            "binance_alpha",
            addresses => {
                let response = client
                    .get(parsers::binance_alpha_url())
                    .header("Accept-Encoding", "identity")
                    .send()
                    .await?;
                if !response.status().is_success() {
                    anyhow::bail!("HTTP {}", response.status());
                }
                parsers::parse_binance_alpha_batch(&response.text().await?, &addresses)
            }
        );

        let pyth = index_symbols(configs, |c: &ExchangeConfig| c.pyth_id());
        if !pyth.is_empty() {
            let price_ids: Vec<&str> = pyth.keys().copied().collect();
            let url = parsers::pyth_batch_url(&price_ids);
            match http_get_text(client, &url).await {
                Ok(body) => match parsers::parse_pyth_batch(&body, &price_ids) {
                    Ok(prices) => fan_out(&mut out, &pyth, &prices, "pyth", pyth_publish_time),
                    Err(e) => eprintln!("pyth batch failed: {}", e),
                },
                Err(e) => eprintln!("pyth batch failed: {}", e),
            }
        }

        let chainlink = index_symbols(configs, |c: &ExchangeConfig| c.chainlink.as_deref());
        if !chainlink.is_empty() {
            let feeds: Vec<&str> = chainlink.keys().copied().collect();
            match fetch_chainlink_batch(client, &feeds).await {
                Ok(prices) => fan_out(&mut out, &chainlink, &prices, "chainlink", fetched_now),
                Err(e) => eprintln!("chainlink batch failed: {}", e),
            }
        }

        venue!(
            |c: &ExchangeConfig| c.huobi.as_deref(),
            "huobi",
            symbols => {
                let body = http_get_text(client, &parsers::huobi_batch_url()).await?;
                parsers::parse_huobi_batch(&body, &symbols)
            }
        );
        venue!(
            |c: &ExchangeConfig| c.kucoin.as_deref(),
            "kucoin",
            symbols => {
                let body = http_get_text(client, &parsers::kucoin_batch_url()).await?;
                parsers::parse_kucoin_batch(&body, &symbols)
            }
        );
        venue!(
            |c: &ExchangeConfig| c.gate.as_deref(),
            "gate",
            pairs => {
                let body = http_get_text(client, &parsers::gate_batch_url()).await?;
                parsers::parse_gate_batch(&body, &pairs)
            }
        );
        venue!(
            |c: &ExchangeConfig| c.cryptocom.as_deref(),
            "cryptocom",
            instruments => {
                let body = http_get_text(client, &parsers::cryptocom_batch_url()).await?;
                parsers::parse_cryptocom_batch(&body, &instruments)
            }
        );

        // Kraken answers an unknown pair with an error object rather than a status code
        let kraken = index_symbols(configs, |c: &ExchangeConfig| c.kraken.as_deref());
        if !kraken.is_empty() {
            let pairs: Vec<&str> = kraken.keys().copied().collect();
            match http_get_text(client, &parsers::kraken_batch_url(&pairs)).await {
                Ok(body) => {
                    let parsed = if parsers::is_kraken_unknown_pair(&body) {
                        eprintln!("Kraken rejected the batch ({}), retrying per pair", body.trim());
                        Ok(fetch_per_symbol(&pairs, |p| fetch_kraken(client, p)).await)
                    } else {
                        parsers::parse_kraken_batch(&body, &pairs)
                    };
                    match parsed {
                        Ok(prices) => fan_out(&mut out, &kraken, &prices, "kraken", fetched_now),
                        Err(e) => eprintln!("kraken batch failed: {}", e),
                    }
                }
                Err(e) => eprintln!("kraken batch failed: {}", e),
            }
        }

        venue!(
            |c: &ExchangeConfig| c.coinbase.as_deref(),
            "coinbase",
            product_ids => {
                let body = http_get_text(client, &parsers::coinbase_batch_url()).await?;
                parsers::parse_coinbase_batch(&body, &product_ids)
            }
        );
        venue!(
            |c: &ExchangeConfig| c.bitstamp.as_deref(),
            "bitstamp",
            pairs => {
                let body = http_get_text(client, &parsers::bitstamp_batch_url()).await?;
                parsers::parse_bitstamp_batch(&body, &pairs)
            }
        );
        venue!(
            |c: &ExchangeConfig| c.okx.as_deref(),
            "okx",
            inst_ids => {
                let body = http_get_text(client, &parsers::okx_batch_url()).await?;
                parsers::parse_okx_batch(&body, &inst_ids)
            }
        );
        venue!(
            |c: &ExchangeConfig| c.bitget.as_deref(),
            "bitget",
            symbols => {
                let body = http_get_text(client, &parsers::bitget_batch_url()).await?;
                parsers::parse_bitget_batch(&body, &symbols)
            }
        );
        venue!(
            |c: &ExchangeConfig| c.mexc.as_deref(),
            "mexc",
            symbols => {
                let body = http_get_text(client, &parsers::mexc_batch_url()).await?;
                parsers::parse_mexc_batch(&body, &symbols)
            }
        );

        out
    }
}
