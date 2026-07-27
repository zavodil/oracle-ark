//! URL builders and response parsers for each price source
//!
//! These functions are used by both WASI and scheduler.

use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

// ============================================================================
// Batch primitives
//
// One request per source for the whole token set, instead of one per (token, source).
// Batch parsers take the RAW response body rather than a `serde_json::Value` on purpose:
// the all-ticker endpoints are 129-536 KB and each parser deserializes straight into the
// two or three fields it needs, so the rest of the document is never retained.
// ============================================================================

/// One price extracted from a batch response.
///
/// `timestamp` is the upstream publish time for the sources that report one (CoinGecko,
/// Pyth, Crypto.com, Chainlink) and `None` for the rest, which leaves the caller to stamp
/// the fetch time — the same value the single-symbol path stores today.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BatchPrice {
    pub price: f64,
    pub timestamp: Option<u64>,
}

/// Prices keyed by the symbol EXACTLY as the caller asked for it, so a caller can look up
/// what it put in without knowing how a given venue spells it.
pub type BatchPrices = HashMap<String, BatchPrice>;

/// Map the normalized (uppercased) form of every wanted symbol back to the caller's
/// spelling. Tickers are matched case-insensitively because the asset config and the
/// all-ticker endpoints disagree on case: Gate stores `near_usdt` and returns `NEAR_USDT`.
fn wanted_index<'a>(wanted: &[&'a str]) -> HashMap<String, &'a str> {
    wanted
        .iter()
        .map(|symbol| (symbol.to_ascii_uppercase(), *symbol))
        .collect()
}

/// Read a price field that some venues quote as a string and others as a bare number
/// (Huobi's all-ticker endpoint returns numbers where its single-symbol endpoint returns
/// strings). Non-finite and non-positive values are rejected rather than averaged in.
fn as_price(value: Option<&Value>) -> Option<f64> {
    let price = match value? {
        Value::Number(number) => number.as_f64()?,
        Value::String(text) => text.parse::<f64>().ok()?,
        _ => return None,
    };
    if price.is_finite() && price > 0.0 {
        Some(price)
    } else {
        None
    }
}

/// Reject a price that cannot be real.
///
/// A delisted symbol keeps answering instead of disappearing: Binance quotes DAIUSDT at
/// `"0.00000000"` and Gate's v2 ticker returns all-zero fields with `result: "true"`. Parsed
/// naively, that zero is not a missing source — it enters the aggregate and drags the
/// published price down, which is exactly what an oracle must never do.
fn check_price(price: f64, source: &str) -> Result<f64> {
    if !price.is_finite() || price <= 0.0 {
        return Err(anyhow!(
            "{} returned non-positive price {} (symbol delisted?)",
            source,
            price
        ));
    }
    Ok(price)
}

/// The (bid + ask + last) / 3 ladder the single-symbol parsers use, with the same
/// fallbacks: all three when present, bid/ask when `last` is missing, `last` alone otherwise.
fn blend(bid: Option<f64>, ask: Option<f64>, last: Option<f64>) -> Option<f64> {
    match (bid, ask, last) {
        (Some(b), Some(a), Some(l)) => Some((b + a + l) / 3.0),
        (Some(b), Some(a), None) => Some((b + a) / 2.0),
        (_, _, Some(l)) => Some(l),
        _ => None,
    }
}

/// Build CoinGecko API URL
pub fn coingecko_url(token_id: &str, api_key: Option<&str>) -> String {
    if let Some(key) = api_key {
        format!(
            "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd&x_cg_pro_api_key={}",
            token_id, key
        )
    } else {
        format!(
            "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd",
            token_id
        )
    }
}

/// Parse CoinGecko response: {"bitcoin": {"usd": 100000.0}}
pub fn parse_coingecko(json: &Value, token_id: &str) -> Result<f64> {
    let price = json
        .get(token_id)
        .and_then(|v| v.get("usd"))
        .and_then(|v| v.as_f64())
        .ok_or_else(|| anyhow!("Price not found for {} in CoinGecko response", token_id))?;
    check_price(price, "CoinGecko")
}

/// Build CoinGecko batch URL — one request for every id we need.
/// `include_last_updated_at` costs nothing and gives us the upstream publish time.
pub fn coingecko_batch_url(ids: &[&str], api_key: Option<&str>) -> String {
    let ids = ids.join(",");
    if let Some(key) = api_key {
        format!(
            "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd&include_last_updated_at=true&x_cg_pro_api_key={}",
            ids, key
        )
    } else {
        format!(
            "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd&include_last_updated_at=true",
            ids
        )
    }
}

/// Parse batched CoinGecko response: {"near": {"usd": 1.84, "last_updated_at": 1785142060}}
/// An id CoinGecko does not know is simply absent — it does not fail the request.
pub fn parse_coingecko_batch(body: &str, wanted: &[&str]) -> Result<BatchPrices> {
    #[derive(Deserialize)]
    struct Entry {
        usd: Option<f64>,
        last_updated_at: Option<u64>,
    }

    let entries: HashMap<String, Entry> = serde_json::from_str(body)
        .map_err(|e| anyhow!("CoinGecko batch response parse error: {}", e))?;

    let mut prices = BatchPrices::new();
    for id in wanted {
        if let Some(entry) = entries.get(*id) {
            if let Some(price) = entry.usd.filter(|p| p.is_finite() && *p > 0.0) {
                prices.insert(
                    (*id).to_string(),
                    BatchPrice {
                        price,
                        timestamp: entry.last_updated_at,
                    },
                );
            }
        }
    }

    Ok(prices)
}

/// Build Binance API URL
///
/// NOTE the host: `api.binance.com` answers HTTP 451 (geo-block) from our worker egress,
/// while `data-api.binance.vision` serves the identical public market-data API.
pub fn binance_url(symbol: &str) -> String {
    format!("https://data-api.binance.vision/api/v3/ticker/price?symbol={}", symbol)
}

/// Parse Binance response: {"symbol": "BTCUSDT", "price": "100000.00"}
pub fn parse_binance(json: &Value) -> Result<f64> {
    let price = json
        .get("price")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or_else(|| anyhow!("Price not found in Binance response"))?;
    check_price(price, "Binance")
}

/// Build Binance US API URL (same format as Binance, different domain)
pub fn binance_us_url(symbol: &str) -> String {
    format!("https://api.binance.us/api/v3/ticker/price?symbol={}", symbol)
}

/// Parse Binance US response (same format as Binance)
pub fn parse_binance_us(json: &Value) -> Result<f64> {
    parse_binance(json)
}

/// Percent-encode the `symbols=[...]` query value Binance expects (a JSON array inside the
/// query string). Ticker symbols are alphanumeric; anything else is escaped rather than
/// injected into the URL.
fn encode_symbols_param(symbols: &[&str]) -> String {
    let mut encoded = String::from("%5B");
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            encoded.push(',');
        }
        encoded.push_str("%22");
        for byte in symbol.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => encoded.push(byte as char),
                _ => encoded.push_str(&format!("%{:02X}", byte)),
            }
        }
        encoded.push_str("%22");
    }
    encoded.push_str("%5D");
    encoded
}

/// Build Binance batch URL (exact symbols, ~40 bytes of response per symbol)
pub fn binance_batch_url(symbols: &[&str]) -> String {
    format!(
        "https://data-api.binance.vision/api/v3/ticker/price?symbols={}",
        encode_symbols_param(symbols)
    )
}

/// Build Binance US batch URL (same format as Binance, different domain)
pub fn binance_us_batch_url(symbols: &[&str]) -> String {
    format!(
        "https://api.binance.us/api/v3/ticker/price?symbols={}",
        encode_symbols_param(symbols)
    )
}

/// Parse batched Binance/Binance US response: [{"symbol": "NEARUSD", "price": "1.84200000"}]
/// The array is sorted alphabetically, NOT in request order, so entries are matched on
/// `symbol` instead of position. Neither venue reports a timestamp here.
pub fn parse_binance_batch(body: &str, wanted: &[&str]) -> Result<BatchPrices> {
    parse_symbol_price_batch(body, wanted, "Binance")
}

/// Build Binance Alpha API URL (returns all tokens)
pub fn binance_alpha_url() -> String {
    "https://www.binance.com/bapi/defi/v1/public/wallet-direct/buw/wallet/cex/alpha/all/token/list".to_string()
}

/// Parse Binance Alpha response - find token by contract address
/// Response format: {data: [...]} where array contains objects with contractAddress, price, symbol, etc.
pub fn parse_binance_alpha(json: &Value, contract_address: &str) -> Result<f64> {
    let tokens = json
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Binance Alpha response: 'data' array not found"))?;

    // Normalize contract address for comparison (lowercase, no 0x prefix handling)
    let search_addr = contract_address.to_lowercase();

    for token in tokens {
        if let Some(addr) = token.get("contractAddress").and_then(|v| v.as_str()) {
            if addr.to_lowercase() == search_addr {
                let price = token
                    .get("price")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<f64>().ok())
                    .ok_or_else(|| anyhow!("Price not found for token {}", contract_address))?;
                return check_price(price, "Binance Alpha");
            }
        }
    }

    Err(anyhow!("Token {} not found in Binance Alpha response", contract_address))
}

/// Match several contract addresses against one Binance Alpha listing.
/// The endpoint always returns every token, so the batch is the same single request the
/// per-token path already pays for — it is just parsed once instead of once per token.
pub fn parse_binance_alpha_batch(body: &str, wanted: &[&str]) -> Result<BatchPrices> {
    #[derive(Deserialize)]
    struct Token {
        #[serde(rename = "contractAddress")]
        contract_address: Option<String>,
        price: Option<String>,
    }

    #[derive(Deserialize)]
    struct Response {
        data: Vec<Token>,
    }

    let response: Response = serde_json::from_str(body)
        .map_err(|e| anyhow!("Binance Alpha batch response parse error: {}", e))?;

    let index = wanted_index(wanted);
    let mut prices = BatchPrices::new();
    for token in response.data {
        let address = match token.contract_address {
            Some(address) => address,
            None => continue,
        };
        if let Some(wanted_address) = index.get(&address.to_ascii_uppercase()) {
            if let Some(price) = as_price(token.price.map(Value::String).as_ref()) {
                prices.insert(
                    (*wanted_address).to_string(),
                    BatchPrice {
                        price,
                        timestamp: None,
                    },
                );
            }
        }
    }

    Ok(prices)
}

/// Maximum age of a Pyth price before it is rejected, in seconds.
/// Pyth publishes sub-second, so anything this old means the feed stopped updating.
pub const PYTH_MAX_AGE_SECS: u64 = 120;

/// Build Pyth Hermes API URL
pub fn pyth_url(price_id: &str) -> String {
    format!("https://hermes.pyth.network/v2/updates/price/latest?ids[]={}", price_id)
}

/// Build Pyth Hermes batch URL — `ids[]` is repeated once per feed.
/// `ignore_invalid_price_ids=true` keeps a single unknown feed id from failing the whole
/// batch: without it Hermes answers `404 Price ids not found: 0x…` and we would lose Pyth
/// for every token because of one stale entry in the asset config.
pub fn pyth_batch_url(price_ids: &[&str]) -> String {
    let mut url = String::from(
        "https://hermes.pyth.network/v2/updates/price/latest?ignore_invalid_price_ids=true",
    );
    for price_id in price_ids {
        url.push_str("&ids[]=");
        url.push_str(price_id.strip_prefix("0x").unwrap_or(price_id));
    }
    url
}

/// Parse batched Pyth response, keyed by feed id.
///
/// `parsed[].id` comes back WITHOUT the `0x` prefix the asset config stores, so both sides
/// are stripped before matching. Price is `price * 10^expo`; the timestamp is the feed's own
/// `publish_time`, which the caller checks against `PYTH_MAX_AGE_SECS` per feed.
pub fn parse_pyth_batch(body: &str, wanted: &[&str]) -> Result<BatchPrices> {
    #[derive(Deserialize)]
    struct PriceValue {
        price: String,
        expo: i64,
        publish_time: u64,
    }

    #[derive(Deserialize)]
    struct Feed {
        id: String,
        price: PriceValue,
    }

    #[derive(Deserialize)]
    struct Response {
        parsed: Option<Vec<Feed>>,
    }

    let response: Response = serde_json::from_str(body)
        .map_err(|e| anyhow!("Pyth batch response parse error: {}", e))?;

    let stripped: Vec<&str> = wanted
        .iter()
        .map(|id| id.strip_prefix("0x").unwrap_or(id))
        .collect();
    // The returned key must be the caller's spelling, 0x prefix included if they used one
    let originals: HashMap<&str, &str> = stripped.iter().copied().zip(wanted.iter().copied()).collect();
    let index = wanted_index(&stripped);

    let mut prices = BatchPrices::new();
    for feed in response.parsed.unwrap_or_default() {
        let id = feed.id.strip_prefix("0x").unwrap_or(&feed.id).to_ascii_uppercase();
        let stripped_id = match index.get(&id) {
            Some(stripped_id) => *stripped_id,
            None => continue,
        };
        let raw = match feed.price.price.parse::<f64>() {
            Ok(raw) => raw,
            Err(_) => continue,
        };
        let price = raw * 10f64.powi(feed.price.expo as i32);
        if !price.is_finite() || price <= 0.0 {
            continue;
        }
        let key = originals.get(stripped_id).copied().unwrap_or(stripped_id);
        prices.insert(
            key.to_string(),
            BatchPrice {
                price,
                timestamp: Some(feed.price.publish_time),
            },
        );
    }

    Ok(prices)
}

/// Parse Pyth response and return (price, publish_time)
pub fn parse_pyth(json: &Value) -> Result<(f64, u64)> {
    let price_data = json
        .get("parsed")
        .and_then(|v| v.get(0))
        .and_then(|v| v.get("price"))
        .ok_or_else(|| anyhow!("Price data not found in Pyth response"))?;

    let price_raw = price_data
        .get("price")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or_else(|| anyhow!("Price value not found in Pyth response"))?;

    let expo = price_data
        .get("expo")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| anyhow!("Exponent not found in Pyth response"))?;

    let publish_time = price_data
        .get("publish_time")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow!("Publish time not found in Pyth response"))?;

    let price = price_raw * 10f64.powi(expo as i32);
    Ok((price, publish_time))
}

/// Ethereum RPC endpoints for Chainlink price feeds (tried in order, starting from last working)
pub const CHAINLINK_RPC_URLS: &[&str] = &[
    "https://eth.drpc.org",
    "https://rpc.mevblocker.io",
    "https://ethereum-rpc.publicnode.com",
    "https://0xrpc.io/eth",
    "https://ethereum-public.nodies.app",
    "https://mainnet.gateway.tenderly.co",
    "https://eth.api.onfinality.io/public",
];

/// Build Chainlink eth_call JSON-RPC request body
/// Calls latestAnswer() (selector 0x50d25bcd) on the price feed contract
pub fn chainlink_request_body(feed_address: &str) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_call",
        "params": [
            {
                "to": feed_address,
                "data": "0x50d25bcd"
            },
            "latest"
        ]
    })
}

/// Parse Chainlink eth_call response: hex-encoded int256, 8 decimals
///
/// A delisted feed has no code left at the address, so the node answers with a JSON-RPC
/// `error` (no `result` at all) or with an empty `0x` result. Both are reported as a feed
/// failure rather than as a malformed response, so the caller can tell "this feed is dead"
/// from "this RPC is broken".
pub fn parse_chainlink(json: &Value) -> Result<f64> {
    let hex_result = match json.get("result").and_then(|v| v.as_str()) {
        Some(hex_result) => hex_result,
        None => {
            return Err(anyhow!(
                "Chainlink feed returned no data (reverted or not a price feed)"
            ))
        }
    };

    // Strip 0x prefix and parse hex to u128
    let hex_str = hex_result.trim_start_matches("0x");
    if hex_str.is_empty() {
        return Err(anyhow!(
            "Chainlink feed returned no data (reverted or not a price feed)"
        ));
    }
    if hex_str.chars().all(|c| c == '0') {
        return Err(anyhow!("Chainlink returned zero price"));
    }

    let raw_value = u128::from_str_radix(hex_str, 16)
        .map_err(|e| anyhow!("Failed to parse Chainlink hex value: {}", e))?;

    // Chainlink price feeds use 8 decimals
    let price = raw_value as f64 / 100_000_000.0;

    if price <= 0.0 {
        return Err(anyhow!("Chainlink returned non-positive price"));
    }

    Ok(price)
}

/// True when a JSON-RPC `error` object describes a deterministic contract revert.
/// Rotating to the next RPC cannot fix it — every node executes the same bytecode — so the
/// caller should give up on the feed instead of replaying the call seven times.
pub fn is_execution_revert(error: &Value) -> bool {
    if error.get("code").and_then(|c| c.as_i64()) == Some(3) {
        return true;
    }
    error
        .get("message")
        .and_then(|m| m.as_str())
        .map(|message| {
            let message = message.to_ascii_lowercase();
            message.contains("execution reverted") || message.contains("out of gas")
        })
        .unwrap_or(false)
}

/// Multicall3 — same address on every EVM chain, including Ethereum mainnet
pub const MULTICALL3_ADDRESS: &str = "0xcA11bde05977b3631167028862bE2a173976CA11";

/// Selector of `aggregate3((address,bool,bytes)[])`
const MULTICALL3_AGGREGATE3_SELECTOR: &str = "82ad56cb";

/// Selector of `latestRoundData()` — returns
/// (roundId, answer, startedAt, updatedAt, answeredInRound)
const CHAINLINK_LATEST_ROUND_DATA_SELECTOR: &str = "feaf968c";

/// Encode a u64 as a 32-byte ABI word (hex, no `0x`)
fn encode_word(value: u64) -> String {
    format!("{:064x}", value)
}

/// Encode a 20-byte address as a left-padded 32-byte ABI word
fn encode_address_word(address: &str) -> Result<String> {
    let hex = address.strip_prefix("0x").unwrap_or(address);
    if hex.len() != 40 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(anyhow!("Invalid Chainlink feed address: {}", address));
    }
    Ok(format!("{:0>64}", hex.to_ascii_lowercase()))
}

/// Split a hex payload into 32-byte ABI words
fn abi_words(hex_payload: &str) -> Result<Vec<&str>> {
    let hex = hex_payload.strip_prefix("0x").unwrap_or(hex_payload);
    if hex.is_empty() || hex.len() % 64 != 0 {
        return Err(anyhow!("Malformed ABI payload: {} hex chars", hex.len()));
    }
    Ok((0..hex.len() / 64).map(|i| &hex[i * 64..(i + 1) * 64]).collect())
}

/// Read an ABI word as u128. Fails when the high 16 bytes are set, which for the int256
/// `answer` also covers a negative price.
fn word_to_u128(word: &str) -> Result<u128> {
    if word.len() != 64 {
        return Err(anyhow!("Malformed ABI word: {} hex chars", word.len()));
    }
    if word[..32].chars().any(|c| c != '0') {
        return Err(anyhow!("ABI word out of range: 0x{}", word));
    }
    u128::from_str_radix(&word[32..], 16).map_err(|e| anyhow!("Invalid ABI word: {}", e))
}

/// Result of one batched Chainlink read.
///
/// Feeds that answered land in `prices`; feeds whose inner call failed land in `failures` as
/// `(address, reason)` so a dead feed shows up as a named, per-feed failure in the logs
/// instead of poisoning the whole call.
#[derive(Debug, Default)]
pub struct ChainlinkBatch {
    pub prices: BatchPrices,
    pub failures: Vec<(String, String)>,
}

/// Build the `eth_call` body that reads several Chainlink feeds in ONE request through
/// Multicall3's `aggregate3`.
///
/// `allowFailure` is `true` for every call on purpose: a delisted feed reverts, and with
/// `false` that single revert takes the whole multicall — and every other feed — with it.
/// JSON-RPC array batching is deliberately NOT used instead: `eth.drpc.org` answers HTTP 500
/// for batches larger than three, while this is a plain `eth_call` that every RPC accepts.
pub fn chainlink_multicall_body(feed_addresses: &[&str]) -> Result<Value> {
    let count = feed_addresses.len();
    let mut data = String::from("0x");
    data.push_str(MULTICALL3_AGGREGATE3_SELECTOR);
    // head: offset to the array, then its length
    data.push_str(&encode_word(0x20));
    data.push_str(&encode_word(count as u64));
    // one offset per Call3 tuple, relative to the start of the array data
    for i in 0..count {
        data.push_str(&encode_word((32 * count + 160 * i) as u64));
    }
    // Call3 { address target, bool allowFailure, bytes callData }
    for address in feed_addresses {
        data.push_str(&encode_address_word(address)?);
        data.push_str(&encode_word(1)); // allowFailure = true
        data.push_str(&encode_word(0x60)); // offset of callData inside the tuple
        data.push_str(&encode_word(4)); // callData length
        data.push_str(CHAINLINK_LATEST_ROUND_DATA_SELECTOR);
        data.push_str(&"0".repeat(56)); // right-padded to a full word
    }

    Ok(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_call",
        "params": [
            {
                "to": MULTICALL3_ADDRESS,
                "data": data
            },
            "latest"
        ]
    }))
}

/// Decode a Multicall3 `aggregate3` response holding one `latestRoundData()` per feed.
///
/// Layout: offset, array length, one offset per `Result { bool success, bytes returnData }`.
/// `returnData` is (roundId, answer, startedAt, updatedAt, answeredInRound), so `answer` is
/// word 2 (int256, 8 decimals) and `updatedAt` is word 4.
///
/// Results come back in request order, so `feed_addresses` must be the exact slice the body
/// was built from. A feed that reverted, returned nothing, priced at zero or never updated
/// is recorded in `failures`; the other feeds still price.
pub fn parse_chainlink_multicall(json: &Value, feed_addresses: &[&str]) -> Result<ChainlinkBatch> {
    if let Some(error) = json.get("error") {
        return Err(anyhow!("Chainlink multicall RPC error: {}", error));
    }

    let hex_result = json
        .get("result")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("No result in Chainlink multicall response"))?;

    let words = abi_words(hex_result)?;
    if words.len() < 2 {
        return Err(anyhow!("Chainlink multicall response is too short"));
    }

    let count = word_to_u128(words[1])? as usize;
    if count != feed_addresses.len() {
        return Err(anyhow!(
            "Chainlink multicall returned {} results for {} feeds",
            count,
            feed_addresses.len()
        ));
    }

    let mut batch = ChainlinkBatch::default();
    for (i, address) in feed_addresses.iter().enumerate() {
        match decode_multicall_entry(&words, i) {
            Ok(price) => {
                batch.prices.insert((*address).to_string(), price);
            }
            Err(e) => batch.failures.push(((*address).to_string(), e.to_string())),
        }
    }

    Ok(batch)
}

/// Decode the i-th `Result` of an `aggregate3` response into a price
fn decode_multicall_entry(words: &[&str], index: usize) -> Result<BatchPrice> {
    // Result offsets are relative to the start of the array data, which begins at word 2
    let offset_word = words
        .get(2 + index)
        .ok_or_else(|| anyhow!("missing result offset"))?;
    let entry = 2 + (word_to_u128(offset_word)? as usize) / 32;

    let success = word_to_u128(words.get(entry).ok_or_else(|| anyhow!("missing success flag"))?)?;
    if success == 0 {
        return Err(anyhow!("call reverted (feed delisted or not a price feed)"));
    }

    let length = word_to_u128(
        words
            .get(entry + 2)
            .ok_or_else(|| anyhow!("missing returnData length"))?,
    )? as usize;
    if length < 160 {
        return Err(anyhow!("returned {} bytes, expected 160", length));
    }

    // returnData words start right after (success, offset, length)
    let answer_word = words
        .get(entry + 4)
        .ok_or_else(|| anyhow!("missing answer word"))?;
    if answer_word.starts_with(|c: char| matches!(c, '8'..='9' | 'a'..='f' | 'A'..='F')) {
        return Err(anyhow!("negative answer"));
    }
    let answer = word_to_u128(answer_word)?;

    let updated_at = word_to_u128(
        words
            .get(entry + 6)
            .ok_or_else(|| anyhow!("missing updatedAt word"))?,
    )? as u64;
    if updated_at == 0 {
        return Err(anyhow!("feed never updated (updatedAt = 0)"));
    }

    // Chainlink price feeds use 8 decimals
    let price = answer as f64 / 100_000_000.0;
    if !price.is_finite() || price <= 0.0 {
        return Err(anyhow!("non-positive price"));
    }

    Ok(BatchPrice {
        price,
        timestamp: Some(updated_at),
    })
}

/// Build Huobi API URL
pub fn huobi_url(symbol: &str) -> String {
    format!("https://api.huobi.pro/market/detail/merged?symbol={}", symbol)
}

/// Parse Huobi response - uses mid price (bid + ask) / 2
pub fn parse_huobi(json: &Value) -> Result<f64> {
    let bid = as_price(
        json.get("tick")
            .and_then(|v| v.get("bid"))
            .and_then(|v| v.get(0)),
    );

    let ask = as_price(
        json.get("tick")
            .and_then(|v| v.get("ask"))
            .and_then(|v| v.get(0)),
    );

    match (bid, ask) {
        (Some(b), Some(a)) => check_price((b + a) / 2.0, "Huobi"),
        _ => Err(anyhow!("Bid/Ask not found in Huobi response")),
    }
}

/// Build Huobi all-ticker URL.
/// Huobi has no way to narrow this down — `symbols`/`symbol` filters are rejected — so the
/// batch downloads all ~1000 pairs (129 KB) and keeps only the ones we asked for.
pub fn huobi_batch_url() -> String {
    "https://api.huobi.pro/market/tickers".to_string()
}

/// Parse Huobi all-ticker response, keyed by symbol (`nearusdt`).
///
/// CAUTION: here `bid`/`ask` are JSON NUMBERS at the top level of each entry, while the
/// single-symbol endpoint nests them as strings under `tick.bid[0]`. Mid price is
/// (bid + ask) / 2, the same as the single-symbol path. No upstream timestamp per symbol.
pub fn parse_huobi_batch(body: &str, wanted: &[&str]) -> Result<BatchPrices> {
    #[derive(Deserialize)]
    struct Ticker {
        symbol: String,
        bid: Option<Value>,
        ask: Option<Value>,
    }

    #[derive(Deserialize)]
    struct Response {
        status: Option<String>,
        data: Option<Vec<Ticker>>,
    }

    let response: Response = serde_json::from_str(body)
        .map_err(|e| anyhow!("Huobi batch response parse error: {}", e))?;

    if let Some(status) = response.status.as_deref() {
        if status != "ok" {
            return Err(anyhow!("Huobi batch returned status '{}'", status));
        }
    }

    let index = wanted_index(wanted);
    let mut prices = BatchPrices::new();
    for ticker in response.data.unwrap_or_default() {
        if let Some(symbol) = index.get(&ticker.symbol.to_ascii_uppercase()) {
            let bid = as_price(ticker.bid.as_ref());
            let ask = as_price(ticker.ask.as_ref());
            if let (Some(bid), Some(ask)) = (bid, ask) {
                prices.insert(
                    (*symbol).to_string(),
                    BatchPrice {
                        price: (bid + ask) / 2.0,
                        timestamp: None,
                    },
                );
            }
        }
    }

    Ok(prices)
}

/// Build KuCoin API URL
pub fn kucoin_url(symbol: &str) -> String {
    format!("https://api.kucoin.com/api/v1/market/orderbook/level1?symbol={}", symbol)
}

/// Parse KuCoin response
pub fn parse_kucoin(json: &Value) -> Result<f64> {
    let data = json.get("data").ok_or_else(|| anyhow!("No data in KuCoin response"))?;

    let bid = as_price(data.get("bestBid"));
    let ask = as_price(data.get("bestAsk"));
    let last = as_price(data.get("price"));

    match blend(bid, ask, last) {
        Some(price) => check_price(price, "KuCoin"),
        None => Err(anyhow!("Price not found in KuCoin response")),
    }
}

/// Build KuCoin all-ticker URL (501 KB — KuCoin exposes no filter on this endpoint)
pub fn kucoin_batch_url() -> String {
    "https://api.kucoin.com/api/v1/market/allTickers".to_string()
}

/// Parse KuCoin all-ticker response, keyed by symbol (`NEAR-USDT`).
/// `buy`/`sell`/`last` are strings here and feed the same (bid + ask + last) / 3 ladder the
/// single-symbol path uses for `bestBid`/`bestAsk`/`price`.
pub fn parse_kucoin_batch(body: &str, wanted: &[&str]) -> Result<BatchPrices> {
    #[derive(Deserialize)]
    struct Ticker {
        symbol: String,
        buy: Option<Value>,
        sell: Option<Value>,
        last: Option<Value>,
    }

    #[derive(Deserialize)]
    struct Data {
        ticker: Option<Vec<Ticker>>,
    }

    #[derive(Deserialize)]
    struct Response {
        code: Option<String>,
        data: Option<Data>,
    }

    let response: Response = serde_json::from_str(body)
        .map_err(|e| anyhow!("KuCoin batch response parse error: {}", e))?;

    if let Some(code) = response.code.as_deref() {
        if code != "200000" {
            return Err(anyhow!("KuCoin batch returned code '{}'", code));
        }
    }

    let index = wanted_index(wanted);
    let mut prices = BatchPrices::new();
    let tickers = response.data.and_then(|data| data.ticker).unwrap_or_default();
    for ticker in tickers {
        if let Some(symbol) = index.get(&ticker.symbol.to_ascii_uppercase()) {
            let price = blend(
                as_price(ticker.buy.as_ref()),
                as_price(ticker.sell.as_ref()),
                as_price(ticker.last.as_ref()),
            );
            if let Some(price) = price {
                prices.insert(
                    (*symbol).to_string(),
                    BatchPrice {
                        price,
                        timestamp: None,
                    },
                );
            }
        }
    }

    Ok(prices)
}

/// Build Gate.io API URL
pub fn gate_url(pair: &str) -> String {
    format!("https://data.gateapi.io/api2/1/ticker/{}", pair)
}

/// Parse Gate.io response
pub fn parse_gate(json: &Value) -> Result<f64> {
    let result = json
        .get("result")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("No result in Gate response"))?;

    if result != "true" {
        return Err(anyhow!("Gate.io API returned unsuccessful result"));
    }

    let bid = as_price(json.get("highestBid"));
    let ask = as_price(json.get("lowestAsk"));
    let last = as_price(json.get("last"));

    match blend(bid, ask, last) {
        Some(price) => check_price(price, "Gate"),
        None => Err(anyhow!("Price not found in Gate response")),
    }
}

/// Build Gate.io all-ticker URL (v4, 536 KB — the endpoint takes no multi-pair filter)
pub fn gate_batch_url() -> String {
    "https://api.gateio.ws/api/v4/spot/tickers".to_string()
}

/// Parse Gate.io v4 all-ticker response, keyed by `currency_pair`.
///
/// Two differences from the single-pair v2 endpoint: the pair comes back UPPERCASE
/// (`NEAR_USDT`) while the asset config stores `near_usdt`, hence the case-insensitive
/// match; and the fields are v4 (`last`, `highest_bid`, `lowest_ask`) rather than the v2
/// `highestBid`/`lowestAsk`. The (bid + ask + last) / 3 ladder is unchanged.
pub fn parse_gate_batch(body: &str, wanted: &[&str]) -> Result<BatchPrices> {
    #[derive(Deserialize)]
    struct Ticker {
        currency_pair: String,
        last: Option<Value>,
        highest_bid: Option<Value>,
        lowest_ask: Option<Value>,
    }

    let tickers: Vec<Ticker> = serde_json::from_str(body)
        .map_err(|e| anyhow!("Gate batch response parse error: {}", e))?;

    let index = wanted_index(wanted);
    let mut prices = BatchPrices::new();
    for ticker in tickers {
        if let Some(pair) = index.get(&ticker.currency_pair.to_ascii_uppercase()) {
            let price = blend(
                as_price(ticker.highest_bid.as_ref()),
                as_price(ticker.lowest_ask.as_ref()),
                as_price(ticker.last.as_ref()),
            );
            if let Some(price) = price {
                prices.insert(
                    (*pair).to_string(),
                    BatchPrice {
                        price,
                        timestamp: None,
                    },
                );
            }
        }
    }

    Ok(prices)
}

/// Build Crypto.com API URL
pub fn cryptocom_url(instrument: &str) -> String {
    format!("https://api.crypto.com/v2/public/get-ticker?instrument_name={}", instrument)
}

/// Parse Crypto.com response
pub fn parse_cryptocom(json: &Value) -> Result<f64> {
    let data = json
        .get("result")
        .and_then(|v| v.get("data"))
        .and_then(|v| v.get(0))
        .ok_or_else(|| anyhow!("Data not found in Crypto.com response"))?;

    let bid = as_price(data.get("b"));
    let ask = as_price(data.get("k"));
    let last = as_price(data.get("a"));

    match blend(bid, ask, last) {
        Some(price) => check_price(price, "Crypto.com"),
        None => Err(anyhow!("Price not found in Crypto.com response")),
    }
}

/// Build Crypto.com all-ticker URL (v1, 239 KB — no per-instrument filter for a set)
pub fn cryptocom_batch_url() -> String {
    "https://api.crypto.com/exchange/v1/public/get-tickers".to_string()
}

/// Parse Crypto.com v1 all-ticker response, keyed by instrument (`i` = `NEAR_USDT`).
/// Short field names: `a` = last, `b` = bid, `k` = ask, `t` = timestamp in MILLIseconds.
pub fn parse_cryptocom_batch(body: &str, wanted: &[&str]) -> Result<BatchPrices> {
    #[derive(Deserialize)]
    struct Ticker {
        i: String,
        a: Option<Value>,
        b: Option<Value>,
        k: Option<Value>,
        t: Option<u64>,
    }

    #[derive(Deserialize)]
    struct ResultData {
        data: Option<Vec<Ticker>>,
    }

    #[derive(Deserialize)]
    struct Response {
        code: Option<i64>,
        result: Option<ResultData>,
    }

    let response: Response = serde_json::from_str(body)
        .map_err(|e| anyhow!("Crypto.com batch response parse error: {}", e))?;

    if let Some(code) = response.code {
        if code != 0 {
            return Err(anyhow!("Crypto.com batch returned code {}", code));
        }
    }

    let index = wanted_index(wanted);
    let mut prices = BatchPrices::new();
    let tickers = response.result.and_then(|result| result.data).unwrap_or_default();
    for ticker in tickers {
        if let Some(instrument) = index.get(&ticker.i.to_ascii_uppercase()) {
            let price = blend(
                as_price(ticker.b.as_ref()),
                as_price(ticker.k.as_ref()),
                as_price(ticker.a.as_ref()),
            );
            if let Some(price) = price {
                prices.insert(
                    (*instrument).to_string(),
                    BatchPrice {
                        price,
                        timestamp: ticker.t.map(|ms| ms / 1000),
                    },
                );
            }
        }
    }

    Ok(prices)
}

// ============================================================================
// Kraken
// ============================================================================

/// Percent-encode a comma-separated Kraken `pair=` list.
///
/// Real Kraken pair names are alphanumeric, so this is a no-op for valid config; anything
/// else is escaped rather than injected into the URL, the same rule `encode_symbols_param`
/// applies to Binance.
fn encode_kraken_pairs(pairs: &[&str]) -> String {
    let mut encoded = String::new();
    for (i, pair) in pairs.iter().enumerate() {
        if i > 0 {
            encoded.push(',');
        }
        for byte in pair.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => encoded.push(byte as char),
                _ => encoded.push_str(&format!("%{:02X}", byte)),
            }
        }
    }
    encoded
}

/// Build Kraken ticker URL for a single pair
pub fn kraken_url(pair: &str) -> String {
    format!(
        "https://api.kraken.com/0/public/Ticker?pair={}",
        encode_kraken_pairs(&[pair])
    )
}

/// Build Kraken batch URL — one comma-separated `pair=` list for the whole set.
/// Measured: 15 pairs in one request, 4.2 KB, 0.15s.
pub fn kraken_batch_url(pairs: &[&str]) -> String {
    format!(
        "https://api.kraken.com/0/public/Ticker?pair={}",
        encode_kraken_pairs(pairs)
    )
}

/// True when Kraken's `error` array reports a pair it does not know.
///
/// Kraken answers HTTP 200 with `{"error":["EQuery:Unknown asset pair"],"result":{}}` and
/// drops EVERY pair in the request, so one stale entry in the asset config would silently
/// cost us Kraken for all assets. The caller retries per pair on this.
pub fn is_kraken_unknown_pair(body: &str) -> bool {
    #[derive(Deserialize)]
    struct Response {
        error: Option<Vec<String>>,
    }

    serde_json::from_str::<Response>(body)
        .ok()
        .and_then(|response| response.error)
        .map(|errors| {
            errors
                .iter()
                .any(|e| e.to_ascii_lowercase().contains("unknown asset pair"))
        })
        .unwrap_or(false)
}

/// Strip Kraken's legacy `X`/`Z` asset prefixes so an `altname` spelling lines up with the
/// CANONICAL name Kraken answers with (`XBTUSD` and `BTCUSD` both come back as `XXBTZUSD`).
///
/// This is a heuristic, and deliberately only ever used as a FALLBACK behind an exact match:
/// it cannot tell a legacy `Z`-prefixed quote from a base that simply ends in Z, so on the
/// full Kraken pair list it maps both `AIOUSD` and `AIOZUSD` onto `AIOUSD`. Collisions are
/// therefore detected and dropped in `kraken_fallback_index` rather than guessed at.
fn kraken_normalize(pair: &str) -> String {
    let pair = pair.to_ascii_uppercase();
    let base = if pair.len() > 4 && pair.ends_with("ZUSD") {
        &pair[..pair.len() - 4]
    } else if pair.len() > 3 && pair.ends_with("USD") {
        &pair[..pair.len() - 3]
    } else {
        return pair;
    };
    let base = if base.len() == 4 && base.starts_with('X') {
        &base[1..]
    } else {
        base
    };
    format!("{}USD", base)
}

/// Map the normalized form of every wanted pair back to the caller's spelling, keeping ONLY
/// the forms that identify exactly one wanted pair. A normalized form two wanted pairs share
/// is ambiguous, so it is dropped and those pairs match by exact name alone.
fn kraken_fallback_index<'a>(wanted: &[&'a str]) -> HashMap<String, &'a str> {
    let mut index: HashMap<String, &'a str> = HashMap::new();
    let mut ambiguous: Vec<String> = Vec::new();
    for pair in wanted {
        let normalized = kraken_normalize(pair);
        if let Some(existing) = index.insert(normalized.clone(), *pair) {
            if existing != *pair {
                ambiguous.push(normalized);
            }
        }
    }
    for key in ambiguous {
        index.remove(&key);
    }
    index
}

/// Parse a single-pair Kraken ticker: `result.<PAIR>.c[0]` is the last trade price.
///
/// The response holds exactly the one pair asked for, but under Kraken's canonical name,
/// which is often NOT the name requested — so the entry is read positionally instead of by
/// key. `c[0]` rather than a bid/ask mid on purpose: a `cancel_only` market such as
/// `RHEAUSD` still quotes a wide bid/ask while its last trade is `"0.000000000"`, and a
/// zero is rejected here where a mid would have invented a price.
pub fn parse_kraken(json: &Value) -> Result<f64> {
    if let Some(errors) = json.get("error").and_then(|v| v.as_array()) {
        if !errors.is_empty() {
            return Err(anyhow!("Kraken API error: {}", Value::Array(errors.clone())));
        }
    }

    let price = json
        .get("result")
        .and_then(|v| v.as_object())
        .and_then(|result| result.values().next())
        .and_then(|ticker| ticker.get("c"))
        .and_then(|last| last.get(0))
        .and_then(|price| as_price(Some(price)))
        .ok_or_else(|| anyhow!("Price not found in Kraken response"))?;
    check_price(price, "Kraken")
}

/// Parse a batched Kraken ticker response, keyed by pair.
///
/// Kraken replies under its CANONICAL pair name whatever alias was requested (verified live:
/// `XBTUSD`, `BTCUSD` and `XXBTZUSD` all answer as `XXBTZUSD`), so a returned key is matched
/// exactly first and only then through the collision-guarded legacy-prefix fallback.
/// No upstream timestamp: the ticker reports none, so the caller stamps the fetch time.
pub fn parse_kraken_batch(body: &str, wanted: &[&str]) -> Result<BatchPrices> {
    #[derive(Deserialize)]
    struct Ticker {
        c: Option<Vec<Value>>,
    }

    #[derive(Deserialize)]
    struct Response {
        error: Option<Vec<String>>,
        result: Option<HashMap<String, Ticker>>,
    }

    let response: Response = serde_json::from_str(body)
        .map_err(|e| anyhow!("Kraken batch response parse error: {}", e))?;

    if let Some(errors) = response.error.as_ref() {
        if !errors.is_empty() {
            return Err(anyhow!("Kraken batch API error: {}", errors.join(", ")));
        }
    }

    let exact = wanted_index(wanted);
    let fallback = kraken_fallback_index(wanted);

    let mut prices = BatchPrices::new();
    for (key, ticker) in response.result.unwrap_or_default() {
        let pair = match exact.get(&key.to_ascii_uppercase()) {
            Some(pair) => *pair,
            None => match fallback.get(&kraken_normalize(&key)) {
                Some(pair) => *pair,
                None => continue,
            },
        };
        if let Some(price) = as_price(ticker.c.as_ref().and_then(|last| last.first())) {
            prices.insert(pair.to_string(), BatchPrice { price, timestamp: None });
        }
    }

    Ok(prices)
}

// ============================================================================
// Coinbase Exchange
// ============================================================================

/// Build Coinbase Exchange ticker URL for one product (`NEAR-USD`)
pub fn coinbase_url(product_id: &str) -> String {
    format!(
        "https://api.exchange.coinbase.com/products/{}/ticker",
        product_id
    )
}

/// Build the Coinbase batch URL — 24h stats for every product in one 107 KB response.
///
/// Coinbase exposes no batch form of `/ticker`, and this endpoint is served with
/// `Cache-Control: max-age=5` against the ticker's `max-age=1`, so it trails the ticker by a
/// few seconds: measured divergence over our assets is 3.68 bps median, 8.60 bps max. That
/// is well inside the oracle's 100-second freshness SLA, and the alternative is one request
/// per asset, so the batch is used and the lag accepted.
pub fn coinbase_batch_url() -> String {
    "https://api.exchange.coinbase.com/products/stats".to_string()
}

/// Parse a single-product Coinbase ticker: `price`, plus a `time` the caller does not need
/// (like every other exchange here, the single path stamps the fetch time).
pub fn parse_coinbase(json: &Value) -> Result<f64> {
    let price = as_price(json.get("price"))
        .ok_or_else(|| anyhow!("Price not found in Coinbase response"))?;
    check_price(price, "Coinbase")
}

/// Parse the Coinbase stats response, an object keyed by product id.
///
/// Price is `stats_24hour.last`. Delisted products are absent from this document entirely
/// (their `/ticker` answers HTTP 400), and a product that reports no last trade is treated
/// as a missing source rather than as a price — NEVER as `0`.
pub fn parse_coinbase_batch(body: &str, wanted: &[&str]) -> Result<BatchPrices> {
    #[derive(Deserialize)]
    struct Stats {
        last: Option<Value>,
    }

    #[derive(Deserialize)]
    struct Entry {
        stats_24hour: Option<Stats>,
    }

    let entries: HashMap<String, Entry> = serde_json::from_str(body)
        .map_err(|e| anyhow!("Coinbase batch response parse error: {}", e))?;

    let index = wanted_index(wanted);
    let mut prices = BatchPrices::new();
    for (product, entry) in entries {
        if let Some(id) = index.get(&product.to_ascii_uppercase()) {
            let last = entry.stats_24hour.and_then(|stats| stats.last);
            if let Some(price) = as_price(last.as_ref()) {
                prices.insert((*id).to_string(), BatchPrice { price, timestamp: None });
            }
        }
    }

    Ok(prices)
}

// ============================================================================
// Bitstamp
// ============================================================================

/// Turn the stored pair (`BTC/USD`) into the single-pair URL form (`btcusd`).
/// The slashed spelling is what the all-ticker endpoint reports, so it is the one stored.
fn bitstamp_url_pair(pair: &str) -> String {
    pair.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Build Bitstamp ticker URL for one pair
pub fn bitstamp_url(pair: &str) -> String {
    format!(
        "https://www.bitstamp.net/api/v2/ticker/{}/",
        bitstamp_url_pair(pair)
    )
}

/// Build the Bitstamp all-ticker URL (92 KB — the endpoint takes no pair filter)
pub fn bitstamp_batch_url() -> String {
    "https://www.bitstamp.net/api/v2/ticker/".to_string()
}

/// Parse a single-pair Bitstamp ticker: `last`
pub fn parse_bitstamp(json: &Value) -> Result<f64> {
    let price = as_price(json.get("last"))
        .ok_or_else(|| anyhow!("Price not found in Bitstamp response"))?;
    check_price(price, "Bitstamp")
}

/// Parse the Bitstamp all-ticker response, an array keyed by `pair` (`BTC/USD`, WITH slash).
/// `timestamp` is unix SECONDS carried as a string, one of the few venues here that reports
/// an upstream publish time on its batch endpoint.
pub fn parse_bitstamp_batch(body: &str, wanted: &[&str]) -> Result<BatchPrices> {
    #[derive(Deserialize)]
    struct Ticker {
        pair: Option<String>,
        last: Option<Value>,
        timestamp: Option<String>,
    }

    let tickers: Vec<Ticker> = serde_json::from_str(body)
        .map_err(|e| anyhow!("Bitstamp batch response parse error: {}", e))?;

    let index = wanted_index(wanted);
    let mut prices = BatchPrices::new();
    for ticker in tickers {
        let pair = match ticker.pair {
            Some(pair) => pair,
            None => continue,
        };
        if let Some(wanted_pair) = index.get(&pair.to_ascii_uppercase()) {
            if let Some(price) = as_price(ticker.last.as_ref()) {
                prices.insert(
                    (*wanted_pair).to_string(),
                    BatchPrice {
                        price,
                        timestamp: ticker.timestamp.and_then(|ts| ts.parse::<u64>().ok()),
                    },
                );
            }
        }
    }

    Ok(prices)
}

// ============================================================================
// OKX
// ============================================================================

/// Build OKX ticker URL for one instrument
pub fn okx_url(inst_id: &str) -> String {
    format!("https://www.okx.com/api/v5/market/ticker?instId={}", inst_id)
}

/// Build the OKX batch URL — every SPOT instrument in one 409 KB response
pub fn okx_batch_url() -> String {
    "https://www.okx.com/api/v5/market/tickers?instType=SPOT".to_string()
}

/// Read the price out of one OKX `data[]` entry
fn okx_entry_price(entry: &Value) -> Option<f64> {
    as_price(entry.get("last"))
}

/// Parse a single-instrument OKX ticker. `code` is `"0"` on success.
pub fn parse_okx(json: &Value) -> Result<f64> {
    if let Some(code) = json.get("code").and_then(|v| v.as_str()) {
        if code != "0" {
            return Err(anyhow!(
                "OKX returned code '{}': {}",
                code,
                json.get("msg").and_then(|v| v.as_str()).unwrap_or("")
            ));
        }
    }

    let price = json
        .get("data")
        .and_then(|v| v.get(0))
        .and_then(okx_entry_price)
        .ok_or_else(|| anyhow!("Price not found in OKX response"))?;
    check_price(price, "OKX")
}

/// Parse the OKX all-ticker response, `data[]` keyed by `instId` (`NEAR-USDT`).
/// `ts` is MILLIseconds carried as a string; the oracle works in seconds.
pub fn parse_okx_batch(body: &str, wanted: &[&str]) -> Result<BatchPrices> {
    #[derive(Deserialize)]
    struct Ticker {
        #[serde(rename = "instId")]
        inst_id: String,
        last: Option<Value>,
        ts: Option<Value>,
    }

    #[derive(Deserialize)]
    struct Response {
        code: Option<String>,
        msg: Option<String>,
        data: Option<Vec<Ticker>>,
    }

    let response: Response = serde_json::from_str(body)
        .map_err(|e| anyhow!("OKX batch response parse error: {}", e))?;

    if let Some(code) = response.code.as_deref() {
        if code != "0" {
            return Err(anyhow!(
                "OKX batch returned code '{}': {}",
                code,
                response.msg.unwrap_or_default()
            ));
        }
    }

    let index = wanted_index(wanted);
    let mut prices = BatchPrices::new();
    for ticker in response.data.unwrap_or_default() {
        if let Some(inst_id) = index.get(&ticker.inst_id.to_ascii_uppercase()) {
            if let Some(price) = as_price(ticker.last.as_ref()) {
                prices.insert(
                    (*inst_id).to_string(),
                    BatchPrice {
                        price,
                        timestamp: millis_to_secs(ticker.ts.as_ref()),
                    },
                );
            }
        }
    }

    Ok(prices)
}

/// Read a millisecond timestamp that a venue may quote as a string or as a bare number,
/// and return it in seconds.
fn millis_to_secs(value: Option<&Value>) -> Option<u64> {
    match value? {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.parse::<u64>().ok(),
        _ => None,
    }
    .map(|ms| ms / 1000)
}

// ============================================================================
// Bitget
// ============================================================================

/// Build Bitget ticker URL for one symbol
pub fn bitget_url(symbol: &str) -> String {
    format!(
        "https://api.bitget.com/api/v2/spot/market/tickers?symbol={}",
        symbol
    )
}

/// Build the Bitget batch URL — every spot symbol in one 378 KB response
pub fn bitget_batch_url() -> String {
    "https://api.bitget.com/api/v2/spot/market/tickers".to_string()
}

/// Parse a single-symbol Bitget ticker. `code` is `"00000"` on success and the price field
/// is `lastPr`, NOT `last` — reading `last` here yields nothing at all.
pub fn parse_bitget(json: &Value) -> Result<f64> {
    if let Some(code) = json.get("code").and_then(|v| v.as_str()) {
        if code != "00000" {
            return Err(anyhow!(
                "Bitget returned code '{}': {}",
                code,
                json.get("msg").and_then(|v| v.as_str()).unwrap_or("")
            ));
        }
    }

    let price = json
        .get("data")
        .and_then(|v| v.get(0))
        .and_then(|entry| as_price(entry.get("lastPr")))
        .ok_or_else(|| anyhow!("Price not found in Bitget response"))?;
    check_price(price, "Bitget")
}

/// Parse the Bitget all-ticker response, `data[]` keyed by `symbol` (`NEARUSDT`).
/// Price is `lastPr`; `ts` is MILLIseconds carried as a string.
pub fn parse_bitget_batch(body: &str, wanted: &[&str]) -> Result<BatchPrices> {
    #[derive(Deserialize)]
    struct Ticker {
        symbol: String,
        #[serde(rename = "lastPr")]
        last_pr: Option<Value>,
        ts: Option<Value>,
    }

    #[derive(Deserialize)]
    struct Response {
        code: Option<String>,
        msg: Option<String>,
        data: Option<Vec<Ticker>>,
    }

    let response: Response = serde_json::from_str(body)
        .map_err(|e| anyhow!("Bitget batch response parse error: {}", e))?;

    if let Some(code) = response.code.as_deref() {
        if code != "00000" {
            return Err(anyhow!(
                "Bitget batch returned code '{}': {}",
                code,
                response.msg.unwrap_or_default()
            ));
        }
    }

    let index = wanted_index(wanted);
    let mut prices = BatchPrices::new();
    for ticker in response.data.unwrap_or_default() {
        if let Some(symbol) = index.get(&ticker.symbol.to_ascii_uppercase()) {
            if let Some(price) = as_price(ticker.last_pr.as_ref()) {
                prices.insert(
                    (*symbol).to_string(),
                    BatchPrice {
                        price,
                        timestamp: millis_to_secs(ticker.ts.as_ref()),
                    },
                );
            }
        }
    }

    Ok(prices)
}

// ============================================================================
// MEXC
// ============================================================================

/// Build MEXC ticker URL for one symbol
pub fn mexc_url(symbol: &str) -> String {
    format!("https://api.mexc.com/api/v3/ticker/price?symbol={}", symbol)
}

/// Build the MEXC batch URL — every symbol in one response.
/// This endpoint quotes USDT pairs only and carries no timestamp.
pub fn mexc_batch_url() -> String {
    "https://api.mexc.com/api/v3/ticker/price".to_string()
}

/// Parse a single-symbol MEXC ticker: `{"symbol": "NEARUSDT", "price": "1.848"}`
pub fn parse_mexc(json: &Value) -> Result<f64> {
    let price =
        as_price(json.get("price")).ok_or_else(|| anyhow!("Price not found in MEXC response"))?;
    check_price(price, "MEXC")
}

/// Parse a `[{"symbol": ..., "price": ...}]` array — the shape Binance and MEXC share.
/// Entries are matched on `symbol`, never on position: the array is not in request order.
fn parse_symbol_price_batch(body: &str, wanted: &[&str], source: &str) -> Result<BatchPrices> {
    #[derive(Deserialize)]
    struct Ticker {
        symbol: String,
        price: String,
    }

    let tickers: Vec<Ticker> = serde_json::from_str(body)
        .map_err(|e| anyhow!("{} batch response parse error: {}", source, e))?;

    let index = wanted_index(wanted);
    let mut prices = BatchPrices::new();
    for ticker in tickers {
        if let Some(symbol) = index.get(&ticker.symbol.to_ascii_uppercase()) {
            if let Some(price) = as_price(Some(&Value::String(ticker.price))) {
                prices.insert(
                    (*symbol).to_string(),
                    BatchPrice {
                        price,
                        timestamp: None,
                    },
                );
            }
        }
    }

    Ok(prices)
}

/// Parse the batched MEXC response (same shape as Binance, no timestamp)
pub fn parse_mexc_batch(body: &str, wanted: &[&str]) -> Result<BatchPrices> {
    parse_symbol_price_batch(body, wanted, "MEXC")
}

/// Extract value from JSON using dot notation path
/// Examples: "price", "data.price", "rates.USD", "items.0.value"
pub fn extract_json_path(json: &Value, path: &str) -> Result<Value> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = json;

    for part in parts {
        if let Some(next) = current.get(part) {
            current = next;
        } else if let Ok(index) = part.parse::<usize>() {
            current = current
                .get(index)
                .ok_or_else(|| anyhow!("JSON path '{}' array index '{}' out of bounds", path, part))?;
        } else {
            return Err(anyhow!("JSON path '{}' not found at '{}'", path, part));
        }
    }

    Ok(current.clone())
}

/// Parse value from JSON based on type
pub fn parse_value(value: &Value, value_type: &str) -> Result<f64> {
    match value_type {
        "number" => {
            if let Some(num) = value.as_f64() {
                Ok(num)
            } else if let Some(s) = value.as_str() {
                s.parse::<f64>()
                    .map_err(|e| anyhow!("Failed to parse '{}' as number: {}", s, e))
            } else if let Some(i) = value.as_i64() {
                Ok(i as f64)
            } else if let Some(u) = value.as_u64() {
                Ok(u as f64)
            } else {
                Err(anyhow!("Value is not a number: {:?}", value))
            }
        }
        "boolean" => {
            if let Some(b) = value.as_bool() {
                Ok(if b { 1.0 } else { 0.0 })
            } else {
                Err(anyhow!("Value is not a boolean: {:?}", value))
            }
        }
        _ => Err(anyhow!("Unsupported value type: {}", value_type)),
    }
}

/// Calculate median of prices
pub fn median(prices: &mut [f64]) -> f64 {
    prices.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let len = prices.len();
    if len == 0 {
        return 0.0;
    }
    if len % 2 == 0 {
        (prices[len / 2 - 1] + prices[len / 2]) / 2.0
    } else {
        prices[len / 2]
    }
}

/// Calculate arithmetic average of prices
pub fn average(prices: &[f64]) -> f64 {
    if prices.is_empty() {
        return 0.0;
    }
    let sum: f64 = prices.iter().sum();
    sum / prices.len() as f64
}

/// Calculate weighted average (currently equal weights, can be extended)
pub fn weighted_average(prices: &[f64]) -> f64 {
    // For now, use equal weights (same as average)
    // Can be extended with reputation-based weighting
    average(prices)
}

/// Calculate price deviation percentage between min and max prices
pub fn price_deviation(prices: &[f64]) -> f64 {
    if prices.len() < 2 {
        return 0.0;
    }

    let mut min_price = f64::MAX;
    let mut max_price = f64::MIN;

    for &price in prices {
        if price < min_price {
            min_price = price;
        }
        if price > max_price {
            max_price = price;
        }
    }

    if min_price == 0.0 {
        return 100.0;
    }

    ((max_price - min_price) / min_price) * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_coingecko() {
        let json = json!({"bitcoin": {"usd": 45000.5}});
        assert_eq!(parse_coingecko(&json, "bitcoin").unwrap(), 45000.5);
    }

    #[test]
    fn test_parse_binance() {
        let json = json!({"symbol": "BTCUSDT", "price": "45000.50"});
        assert_eq!(parse_binance(&json).unwrap(), 45000.5);
    }

    #[test]
    fn test_extract_json_path() {
        let json = json!({"data": {"items": [{"price": 100}]}});
        let value = extract_json_path(&json, "data.items.0.price").unwrap();
        assert_eq!(value.as_i64().unwrap(), 100);
    }

    #[test]
    fn test_median() {
        let mut prices = vec![5.0, 1.0, 3.0];
        assert_eq!(median(&mut prices), 3.0);

        let mut prices = vec![4.0, 1.0, 3.0, 2.0];
        assert_eq!(median(&mut prices), 2.5);
    }

    #[test]
    fn test_delisted_symbol_is_not_priced_as_zero() {
        // Real response for DAIUSDT, which Binance still answers for after delisting it.
        // Parsed naively this is 0.0, and a 0 in the aggregate is far worse than a source
        // that simply did not report.
        assert!(parse_binance(&json!({"symbol": "DAIUSDT", "price": "0.00000000"})).is_err());

        // Real Gate v2 ticker for a pair it no longer lists: every field zero, result "true"
        let gate = json!({"quoteVolume":"0","baseVolume":"0","highestBid":"0","high24hr":"0",
                          "last":"0","lowestAsk":"0","result":"true","low24hr":"0","percentChange":"0"});
        assert!(parse_gate(&gate).is_err());

        // The batch parsers drop it the same way, without disturbing the other symbols
        let body = r#"[{"symbol":"DAIUSDT","price":"0.00000000"},{"symbol":"NEARUSDT","price":"1.84100000"}]"#;
        let prices = parse_binance_batch(body, &["DAIUSDT", "NEARUSDT"]).unwrap();
        assert!(!prices.contains_key("DAIUSDT"));
        assert_eq!(prices["NEARUSDT"].price, 1.841);
    }

    // ------------------------------------------------------------------------
    // Batch parsers
    //
    // Every response body below is a real one, captured from the live endpoint and trimmed
    // to the symbols under test.
    // ------------------------------------------------------------------------

    /// Prices are blended with divisions, so compare within a float epsilon
    fn approx(actual: f64, expected: f64) -> bool {
        (actual - expected).abs() < 1e-9
    }

    #[test]
    fn test_parse_coingecko_batch() {
        let body = r#"{"ethereum":{"usd":1956.98,"last_updated_at":1785143300},"near":{"usd":1.83,"last_updated_at":1785143300}}"#;
        let prices = parse_coingecko_batch(body, &["near", "ethereum", "notacoin-xyz"]).unwrap();

        assert_eq!(prices["near"].price, 1.83);
        assert_eq!(prices["near"].timestamp, Some(1785143300));
        assert_eq!(prices["ethereum"].price, 1956.98);
        // CoinGecko drops ids it does not know instead of failing the request
        assert!(!prices.contains_key("notacoin-xyz"));
    }

    #[test]
    fn test_parse_binance_batch_matches_on_symbol_not_position() {
        // Binance returns the array sorted alphabetically, NOT in request order
        let body = r#"[{"symbol":"ETHUSD","price":"1957.93000000"},{"symbol":"NEARUSD","price":"1.84200000"}]"#;
        let prices = parse_binance_batch(body, &["NEARUSD", "ETHUSD"]).unwrap();

        assert_eq!(prices["NEARUSD"].price, 1.842);
        assert_eq!(prices["ETHUSD"].price, 1957.93);
        // Neither Binance endpoint reports a timestamp
        assert_eq!(prices["NEARUSD"].timestamp, None);
    }

    #[test]
    fn test_binance_batch_urls() {
        // api.binance.com is geo-blocked (HTTP 451) from our egress
        let url = binance_batch_url(&["NEARUSDT", "BTCUSDT"]);
        assert_eq!(
            url,
            "https://data-api.binance.vision/api/v3/ticker/price?symbols=%5B%22NEARUSDT%22,%22BTCUSDT%22%5D"
        );
        assert!(binance_url("NEARUSDT").starts_with("https://data-api.binance.vision/"));
        assert!(binance_us_batch_url(&["NEARUSD"]).starts_with("https://api.binance.us/"));
        // Anything that is not alphanumeric is escaped rather than injected into the URL
        assert_eq!(encode_symbols_param(&["A B"]), "%5B%22A%20B%22%5D");
    }

    #[test]
    fn test_parse_binance_alpha_batch() {
        let body = r#"{"code":"000000","message":null,"data":[
            {"tokenId":"20476B966302A843CFCAFE7960CF6F4B","chainId":"56","contractAddress":"0x4c067de26475e1cefee8b8d1f6e2266b33a2372e","name":"Rhea Finance","symbol":"RHEA","price":"0.0122594936999068210059896380096674","percentChange24h":"0.57"},
            {"tokenId":"0000000000000000000000000000AAAA","chainId":"56","contractAddress":"0x0000000000000000000000000000000000000001","name":"Other","symbol":"OTHER","price":"1.5","percentChange24h":"0"}
        ],"success":true}"#;
        // The asset config stores the address lowercase; matching is case-insensitive anyway
        let prices =
            parse_binance_alpha_batch(body, &["0x4C067DE26475E1CEFEE8B8D1F6E2266B33A2372E"]).unwrap();

        assert_eq!(prices.len(), 1);
        assert!(approx(
            prices["0x4C067DE26475E1CEFEE8B8D1F6E2266B33A2372E"].price,
            0.012259493699906821
        ));
    }

    #[test]
    fn test_parse_pyth_batch_strips_0x_prefix() {
        // Hermes reports ids WITHOUT the 0x prefix that tokens.json stores
        let body = r#"{"binary":{"encoding":"hex","data":["504e41550100000003b80100..."]},"parsed":[{"id":"c415de8d2eba7db216527dff4b60e8f3a5311c740dadb233e13e12547e226750","price":{"price":"183487211","conf":"211694","expo":-8,"publish_time":1785143368},"ema_price":{"price":"184242413","conf":"209509","expo":-8,"publish_time":1785143368},"metadata":{"slot":304918931,"proof_available_time":1785143370,"prev_publish_time":1785143368}},{"id":"ff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace","price":{"price":"195656091864","conf":"87908136","expo":-8,"publish_time":1785143368},"ema_price":{"price":"196361740000","conf":"69342421","expo":-8,"publish_time":1785143368},"metadata":{"slot":304918931,"proof_available_time":1785143370,"prev_publish_time":1785143368}}]}"#;
        let near = "0xc415de8d2eba7db216527dff4b60e8f3a5311c740dadb233e13e12547e226750";
        let eth = "ff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace";

        let prices = parse_pyth_batch(body, &[near, eth]).unwrap();

        // Keys come back exactly as the caller spelled them, 0x prefix or not
        assert!(approx(prices[near].price, 1.83487211));
        assert_eq!(prices[near].timestamp, Some(1785143368));
        assert!(approx(prices[eth].price, 1956.56091864));

        // The batch URL strips the prefix and asks Hermes to ignore unknown ids
        let url = pyth_batch_url(&[near, eth]);
        assert!(url.contains("ignore_invalid_price_ids=true"));
        assert!(url.contains("&ids[]=c415de8d"));
        assert!(!url.contains("0xc415de8d"));
    }

    #[test]
    fn test_parse_huobi_batch_accepts_numeric_and_string_fields() {
        // The all-ticker endpoint returns bid/ask as JSON NUMBERS...
        let body = r#"{"status":"ok","ts":1785143379900,"data":[{"symbol":"nearusdt","open":1.8114,"high":1.8666,"low":1.7888,"close":1.8452,"amount":347684.0278261904,"vol":635633.4074736909,"count":24424,"bid":1.8397,"bidSize":5.240480961923848,"ask":1.8509,"askSize":54.63},{"symbol":"ethusdt","open":1914.77,"high":1981.65,"low":1908.82,"close":1957.73,"amount":27880.668565318017,"vol":53932976.08442047,"count":24659,"bid":1957.98,"bidSize":0.6583,"ask":1957.99,"askSize":1.0522}]}"#;
        let prices = parse_huobi_batch(body, &["nearusdt", "ethusdt"]).unwrap();

        assert!(approx(prices["nearusdt"].price, (1.8397 + 1.8509) / 2.0));
        assert!(approx(prices["ethusdt"].price, (1957.98 + 1957.99) / 2.0));

        // ...while the single-symbol endpoint quotes them as strings, so both must parse
        let quoted = r#"{"status":"ok","data":[{"symbol":"nearusdt","bid":"1.8397","ask":"1.8509"}]}"#;
        let prices = parse_huobi_batch(quoted, &["nearusdt"]).unwrap();
        assert!(approx(prices["nearusdt"].price, (1.8397 + 1.8509) / 2.0));

        // A non-ok status is an error, not an empty result
        assert!(parse_huobi_batch(r#"{"status":"error","data":[]}"#, &["nearusdt"]).is_err());
    }

    #[test]
    fn test_parse_kucoin_batch() {
        let body = r#"{"code":"200000","data":{"time":1785143386586,"ticker":[{"symbol":"ETH-USDT","symbolName":"ETH-USDT","buy":"1958.49","bestBidSize":"5.8034879","sell":"1958.5","bestAskSize":"4.8345166","last":"1958.5","averagePrice":"1885.68524206"},{"symbol":"NEAR-USDT","symbolName":"NEAR-USDT","buy":"1.8376","bestBidSize":"163.4058","sell":"1.8378","bestAskSize":"67.5","last":"1.8377","averagePrice":"1.7966811"}]}}"#;
        let prices = parse_kucoin_batch(body, &["NEAR-USDT", "ETH-USDT"]).unwrap();

        assert!(approx(
            prices["NEAR-USDT"].price,
            (1.8376 + 1.8378 + 1.8377) / 3.0
        ));
        assert!(approx(
            prices["ETH-USDT"].price,
            (1958.49 + 1958.5 + 1958.5) / 3.0
        ));
    }

    #[test]
    fn test_parse_gate_batch_matches_uppercase_pairs() {
        // Gate v4 returns UPPERCASE pairs and v4 field names; tokens.json stores `near_usdt`
        let body = r#"[{"currency_pair":"NEAR_USDT","last":"1.838","lowest_ask":"1.838","highest_bid":"1.837","change_percentage":"1.94","base_volume":"1414286","quote_volume":"2585447.07802","high_24h":"1.86","low_24h":"1.787"},{"currency_pair":"ETH_USDT","last":"1958.48","lowest_ask":"1958.49","highest_bid":"1958.48","change_percentage":"4.04","base_volume":"115508.6974","quote_volume":"223794044.653381","high_24h":"1981","low_24h":"1882.06"}]"#;
        let prices = parse_gate_batch(body, &["near_usdt", "eth_usdt"]).unwrap();

        // Keys come back in the caller's (lowercase) spelling
        assert!(approx(
            prices["near_usdt"].price,
            (1.837 + 1.838 + 1.838) / 3.0
        ));
        // Reading the v2 names (highestBid/lowestAsk) would have collapsed this to `last`
        assert!(!approx(prices["near_usdt"].price, 1.838));
        assert!(approx(
            prices["eth_usdt"].price,
            (1958.48 + 1958.49 + 1958.48) / 3.0
        ));
    }

    #[test]
    fn test_parse_cryptocom_batch() {
        let body = r#"{"id":-1,"method":"public/get-tickers","code":0,"result":{"data":[{"i":"NEAR_USDT","h":"1.8597","l":"1.7890","a":"1.8365","v":"32038.9","vv":"58474.47","c":"0.0172","b":"1.8367","k":"1.8382","oi":"0","t":1785143376285},{"i":"ETH_USDT","h":"1982.74","l":"1881.78","a":"1958.43","v":"15056.3097","vv":"29174891.18","c":"0.0403","b":"1958.61","k":"1958.62","oi":"0","t":1785143387083}]}}"#;
        let prices = parse_cryptocom_batch(body, &["NEAR_USDT", "ETH_USDT"]).unwrap();

        assert!(approx(
            prices["NEAR_USDT"].price,
            (1.8367 + 1.8382 + 1.8365) / 3.0
        ));
        // `t` is milliseconds upstream, seconds everywhere in the oracle
        assert_eq!(prices["NEAR_USDT"].timestamp, Some(1785143376));
        assert_eq!(prices["ETH_USDT"].timestamp, Some(1785143387));
    }

    // ------------------------------------------------------------------------
    // Kraken / Coinbase / Bitstamp / OKX / Bitget / MEXC
    // ------------------------------------------------------------------------

    #[test]
    fn test_parse_kraken_batch_matches_legacy_asset_codes() {
        // Real response, trimmed. Kraken keys crypto with a legacy X prefix and fiat with Z
        // (XXBTZUSD, XETHZUSD) while newer listings are plain (NEARUSD, WBTCUSD).
        let body = r#"{"error":[],"result":{
            "NEARUSD":{"a":["1.84640","34","34.000"],"b":["1.84580","34","34.000"],"c":["1.84550","7.15034160"],"o":"1.83710"},
            "XXBTZUSD":{"a":["65232.00000","1","1.000"],"b":["65231.90000","1","1.000"],"c":["65231.80000","0.00802897"],"o":"65013.16841"},
            "XETHZUSD":{"a":["1965.60000","1","1.000"],"b":["1965.40000","2","2.000"],"c":["1965.40000","0.19000000"],"o":"1910.00000"},
            "USDTZUSD":{"a":["0.99910000","1","1.000"],"b":["0.99900000","1","1.000"],"c":["0.99908000","500.00000000"],"o":"0.99920000"},
            "XDGUSD":{"a":["0.072640000","1","1.000"],"b":["0.072630000","1","1.000"],"c":["0.072637900","100.00000000"],"o":"0.071000000"},
            "WBTCUSD":{"a":["64970.00000","1","1.000"],"b":["64950.00000","1","1.000"],"c":["64962.30000","0.00100000"],"o":"64000.00000"}
        }}"#;
        let wanted = ["NEARUSD", "XXBTZUSD", "XETHZUSD", "USDTZUSD", "XDGUSD", "WBTCUSD"];
        let prices = parse_kraken_batch(body, &wanted).unwrap();

        // `c[0]` is the last trade price, and the key comes back in the caller's spelling
        assert!(approx(prices["NEARUSD"].price, 1.8455));
        assert!(approx(prices["XXBTZUSD"].price, 65231.8));
        assert!(approx(prices["XETHZUSD"].price, 1965.4));
        assert!(approx(prices["USDTZUSD"].price, 0.99908));
        assert!(approx(prices["XDGUSD"].price, 0.0726379));
        assert!(approx(prices["WBTCUSD"].price, 64962.3));
        // Kraken reports no timestamp on the ticker
        assert_eq!(prices["NEARUSD"].timestamp, None);

        let url = kraken_batch_url(&wanted);
        assert_eq!(
            url,
            "https://api.kraken.com/0/public/Ticker?pair=NEARUSD,XXBTZUSD,XETHZUSD,USDTZUSD,XDGUSD,WBTCUSD"
        );
        // Anything non-alphanumeric is escaped rather than injected into the query
        assert_eq!(encode_kraken_pairs(&["A B&pair=X"]), "A%20B%26pair%3DX");
    }

    #[test]
    fn test_parse_kraken_batch_remaps_canonical_names() {
        // VERIFIED LIVE: asking for the altname `XBTUSD` (or `BTCUSD`, or `ETHUSD`) returns
        // the CANONICAL key, so an exact match alone would silently lose those pairs.
        let body = r#"{"error":[],"result":{
            "XXBTZUSD":{"c":["65231.80000","0.00802897"]},
            "XETHZUSD":{"c":["1965.40000","0.19000000"]},
            "USDTZUSD":{"c":["0.99908000","500.00000000"]}
        }}"#;
        let prices = parse_kraken_batch(body, &["XBTUSD", "ETHUSD", "USDTUSD"]).unwrap();

        assert!(approx(prices["XBTUSD"].price, 65231.8));
        assert!(approx(prices["ETHUSD"].price, 1965.4));
        assert!(approx(prices["USDTUSD"].price, 0.99908));

        // The remap is a FALLBACK: an exact match always wins over the normalized form
        let prices = parse_kraken_batch(body, &["XXBTZUSD"]).unwrap();
        assert!(approx(prices["XXBTZUSD"].price, 65231.8));
    }

    #[test]
    fn test_kraken_ambiguous_normalization_is_dropped_not_guessed() {
        // Kraken really lists both AIO/USD and AIOZ/USD. Stripping the legacy `Z` quote maps
        // BOTH onto "AIOUSD", so the fallback must refuse to guess and leave them to exact
        // matching — otherwise one asset would be priced with the other's price.
        assert_eq!(kraken_normalize("AIOUSD"), kraken_normalize("AIOZUSD"));

        let fallback = kraken_fallback_index(&["AIOUSD", "AIOZUSD"]);
        assert!(fallback.is_empty(), "an ambiguous pair must not be guessed at");

        let body = r#"{"error":[],"result":{
            "AIOUSD":{"c":["0.12000000","1"]},
            "AIOZUSD":{"c":["0.34000000","1"]}
        }}"#;
        let prices = parse_kraken_batch(body, &["AIOUSD", "AIOZUSD"]).unwrap();
        assert!(approx(prices["AIOUSD"].price, 0.12));
        assert!(approx(prices["AIOZUSD"].price, 0.34));

        // A pair with no legacy prefix at all is left alone
        assert_eq!(kraken_normalize("NEARUSD"), "NEARUSD");
        assert_eq!(kraken_normalize("XXBTZUSD"), "XBTUSD");
    }

    #[test]
    fn test_kraken_rejects_dead_market_and_batch_wide_error() {
        // REAL response for RHEAUSD, a `cancel_only` market: it still quotes a wide bid/ask
        // but its last trade is zero. A bid/ask mid would have invented ~0.019 out of thin
        // air; `c[0]` plus the non-positive check makes it a missing source instead.
        let dead = json!({"error":[],"result":{"RHEAUSD":{
            "a":["0.034000000","16015","16015.000"],
            "b":["0.004470000","2238","2238.000"],
            "c":["0.000000000","0.00000"],
            "v":["0.00000","0.00000"]}}});
        assert!(parse_kraken(&dead).is_err());

        let body = r#"{"error":[],"result":{"RHEAUSD":{"c":["0.000000000","0.00000"]},"NEARUSD":{"c":["1.84550","7.15"]}}}"#;
        let prices = parse_kraken_batch(body, &["RHEAUSD", "NEARUSD"]).unwrap();
        assert!(!prices.contains_key("RHEAUSD"));
        assert!(approx(prices["NEARUSD"].price, 1.8455));

        // REAL response: ONE unknown pair drops every pair, at HTTP 200. Detecting this is
        // what lets the caller retry per pair instead of losing Kraken for all assets.
        let unknown = r#"{"error":["EQuery:Unknown asset pair"]}"#;
        assert!(is_kraken_unknown_pair(unknown));
        assert!(parse_kraken_batch(unknown, &["NEARUSD"]).is_err());
        assert!(!is_kraken_unknown_pair(r#"{"error":[],"result":{}}"#));

        // A single-pair response carries the canonical key, so it is read positionally
        let single = json!({"error":[],"result":{"XXBTZUSD":{"c":["65231.80000","0.008"]}}});
        assert!(approx(parse_kraken(&single).unwrap(), 65231.8));
        assert!(parse_kraken(&json!({"error":["EQuery:Unknown asset pair"]})).is_err());
    }

    #[test]
    fn test_parse_coinbase_batch_reads_nested_stats() {
        // Real /products/stats response, trimmed. The price is nested two levels down, and
        // the endpoint trails /ticker by a few seconds (max-age=5 vs max-age=1).
        let body = r#"{
            "NEAR-USD":{"stats_30day":{"volume":"131512950.514","rfq_volume":"923595.884772"},"stats_24hour":{"open":"1.8","high":"1.858","low":"1.786","last":"1.847","volume":"2183324.5","rfq_volume":"19973.910319"}},
            "BTC-USD":{"stats_30day":{"volume":"1000.0"},"stats_24hour":{"open":"64000","high":"65500","low":"63800","last":"65237","volume":"9000.1"}},
            "AURORA-USD":{"stats_24hour":{"open":"0.0175","high":"0.0181","low":"0.0172","last":"0.0178","volume":"100.0"}}
        }"#;
        let prices = parse_coinbase_batch(body, &["NEAR-USD", "BTC-USD", "AURORA-USD"]).unwrap();

        assert!(approx(prices["NEAR-USD"].price, 1.847));
        assert!(approx(prices["BTC-USD"].price, 65237.0));
        assert!(approx(prices["AURORA-USD"].price, 0.0178));
        // /products/stats carries no timestamp at all
        assert_eq!(prices["NEAR-USD"].timestamp, None);

        // A delisted product is absent from the document entirely (its /ticker answers 400),
        // and a stats block without a last trade must not be read as a price
        assert!(!prices.contains_key("DAI-USD"));
        let empty = r#"{"DEAD-USD":{"stats_24hour":{"open":"0","high":"0","low":"0","last":"0"}},"GONE-USD":{"stats_24hour":{}}}"#;
        let prices = parse_coinbase_batch(empty, &["DEAD-USD", "GONE-USD"]).unwrap();
        assert!(prices.is_empty(), "a zero or absent stat is a missing source, not a price");

        assert_eq!(
            coinbase_url("NEAR-USD"),
            "https://api.exchange.coinbase.com/products/NEAR-USD/ticker"
        );
        // Real single-product ticker
        let ticker = json!({"ask":"1.847","bid":"1.846","price":"1.846","time":"2026-07-27T09:54:10.491922664Z"});
        assert!(approx(parse_coinbase(&ticker).unwrap(), 1.846));
        assert!(parse_coinbase(&json!({"price":"0"})).is_err());
    }

    #[test]
    fn test_parse_bitstamp_batch_matches_slashed_pair() {
        // Real all-ticker response, trimmed. The key is `pair` WITH a slash, and `timestamp`
        // is unix seconds carried as a string.
        let body = r#"[
            {"timestamp":"1785145936","open":"1.83391","high":"1.85494","low":"1.78772","last":"1.84115","volume":"57747.186","vwap":"1.82183","bid":"1.84541","ask":"1.84797","side":"1","open_24":"1.79957","percent_change_24":"2.31","market_type":"SPOT","pair":"NEAR/USD","market":"NEAR/USD"},
            {"timestamp":"1785145937","open":"64000","high":"65500","low":"63800","last":"65236.60","volume":"100.0","vwap":"64900","bid":"65236","ask":"65237","side":"1","open_24":"64000","percent_change_24":"1.9","market_type":"SPOT","pair":"BTC/USD","market":"BTC/USD"},
            {"timestamp":"1785145937","open":"0.9999","high":"1.0001","low":"0.9998","last":"0.99991","volume":"1000.0","vwap":"0.9999","bid":"0.99990","ask":"0.99992","side":"1","open_24":"0.9999","percent_change_24":"0.0","market_type":"SPOT","pair":"USDC/USD","market":"USDC/USD"}
        ]"#;
        let prices = parse_bitstamp_batch(body, &["NEAR/USD", "BTC/USD", "USDC/USD"]).unwrap();

        assert!(approx(prices["NEAR/USD"].price, 1.84115));
        assert!(approx(prices["BTC/USD"].price, 65236.60));
        assert!(approx(prices["USDC/USD"].price, 0.99991));
        // Bitstamp DOES report an upstream publish time, as a string of unix seconds
        assert_eq!(prices["NEAR/USD"].timestamp, Some(1785145936));

        // The single-pair URL is the slashed pair, lowercased with the slash removed
        assert_eq!(
            bitstamp_url("BTC/USD"),
            "https://www.bitstamp.net/api/v2/ticker/btcusd/"
        );
        assert_eq!(bitstamp_url("NEAR/USD"), "https://www.bitstamp.net/api/v2/ticker/nearusd/");

        // Real single-pair response has `last` but no `pair`
        let single = json!({"timestamp":"1785146141","last":"1.84115","bid":"1.84464","ask":"1.84699"});
        assert!(approx(parse_bitstamp(&single).unwrap(), 1.84115));
        assert!(parse_bitstamp(&json!({"last":"0"})).is_err());
    }

    #[test]
    fn test_parse_okx_batch() {
        // Real SPOT all-ticker response, trimmed. `ts` is MILLIseconds carried as a string.
        let body = r#"{"code":"0","msg":"","data":[
            {"instType":"SPOT","instId":"NEAR-USDT","last":"1.848","lastSz":"87.61588489","askPx":"1.849","askSz":"58.9233146","bidPx":"1.848","bidSz":"7289.11660123","open24h":"1.804","high24h":"1.86","low24h":"1.788","volCcy24h":"2557750.61131517111","vol24h":"1398101.93602604","ts":"1785145938266","sodUtc0":"1.839","sodUtc8":"1.804"},
            {"instType":"SPOT","instId":"USDT-USD","last":"0.999","askPx":"0.9991","bidPx":"0.9989","volCcy24h":"7986750.58085619","ts":"1785145938265"},
            {"instType":"SPOT","instId":"BTC-USDT","last":"65304","askPx":"65305","bidPx":"65304","volCcy24h":"226982596.186413857","ts":"1785145938264"}
        ]}"#;
        let prices = parse_okx_batch(body, &["NEAR-USDT", "USDT-USD", "BTC-USDT"]).unwrap();

        assert!(approx(prices["NEAR-USDT"].price, 1.848));
        assert!(approx(prices["USDT-USD"].price, 0.999));
        assert!(approx(prices["BTC-USDT"].price, 65304.0));
        // milliseconds upstream, seconds everywhere in the oracle
        assert_eq!(prices["NEAR-USDT"].timestamp, Some(1785145938));

        // A non-zero code is an error, not an empty result
        assert!(parse_okx_batch(r#"{"code":"50011","msg":"rate limit","data":[]}"#, &["NEAR-USDT"]).is_err());

        // Real single-instrument response has the same shape
        let single = json!({"code":"0","msg":"","data":[{"instId":"NEAR-USDT","last":"1.847","ts":"1785146141466"}]});
        assert!(approx(parse_okx(&single).unwrap(), 1.847));
        assert!(parse_okx(&json!({"code":"51001","msg":"instrument does not exist","data":[]})).is_err());
        assert!(parse_okx(&json!({"code":"0","data":[{"instId":"X","last":"0"}]})).is_err());
    }

    #[test]
    fn test_parse_bitget_batch_uses_last_pr_not_last() {
        // Real all-ticker response, trimmed. Note there is NO `last` field: the price lives
        // in `lastPr`, and reading `last` would yield nothing for every symbol.
        let body = r#"{"code":"00000","msg":"success","requestTime":1785146142382,"data":[
            {"open":"1.805","symbol":"NEARUSDT","high24h":"1.859","low24h":"1.788","lastPr":"1.848","quoteVolume":"733162.7","baseVolume":"401374.4","usdtVolume":"733162.69006","ts":"1785145938426","bidPr":"1.848","askPr":"1.849","bidSz":"118.12","askSz":"844.19","openUtc":"1.838","changeUtc24h":"0.00544","change24h":"0.02382"},
            {"open":"65000","symbol":"WBTCUSDT","high24h":"65500","low24h":"64000","lastPr":"65293.4","quoteVolume":"100","baseVolume":"1","usdtVolume":"100","ts":"1785145938421","bidPr":"65290","askPr":"65295"},
            {"open":"0.0123","symbol":"RHEAUSDT","high24h":"0.0126","low24h":"0.0120","lastPr":"0.01241","quoteVolume":"5000","baseVolume":"400000","usdtVolume":"5000","ts":"1785145938458","bidPr":"0.01240","askPr":"0.01242"}
        ]}"#;
        let prices = parse_bitget_batch(body, &["NEARUSDT", "WBTCUSDT", "RHEAUSDT"]).unwrap();

        assert!(approx(prices["NEARUSDT"].price, 1.848));
        assert!(approx(prices["WBTCUSDT"].price, 65293.4));
        assert!(approx(prices["RHEAUSDT"].price, 0.01241));
        assert_eq!(prices["NEARUSDT"].timestamp, Some(1785145938));

        // Reading `last` instead of `lastPr` finds nothing at all
        let wrong_field = r#"{"code":"00000","data":[{"symbol":"NEARUSDT","last":"1.848","ts":"1785145938426"}]}"#;
        assert!(parse_bitget_batch(wrong_field, &["NEARUSDT"]).unwrap().is_empty());

        // Bitget signals success with "00000", not "0"
        assert!(parse_bitget_batch(r#"{"code":"40034","msg":"param error","data":[]}"#, &["NEARUSDT"]).is_err());

        let single = json!({"code":"00000","msg":"success","data":[{"symbol":"NEARUSDT","lastPr":"1.848","ts":"1785146141311"}]});
        assert!(approx(parse_bitget(&single).unwrap(), 1.848));
        assert!(parse_bitget(&json!({"code":"00000","data":[{"symbol":"X","lastPr":"0"}]})).is_err());
    }

    #[test]
    fn test_parse_mexc_batch() {
        // Real response: same shape as Binance, USDT-quoted only, no timestamp
        let body = r#"[{"symbol":"NEARUSDT","price":"1.848"},{"symbol":"WBTCUSDT","price":"65275.31"},{"symbol":"RHEAUSDT","price":"0.012311"},{"symbol":"AURORAUSDT","price":"0.01801"}]"#;
        let prices =
            parse_mexc_batch(body, &["NEARUSDT", "WBTCUSDT", "RHEAUSDT", "AURORAUSDT"]).unwrap();

        assert!(approx(prices["NEARUSDT"].price, 1.848));
        assert!(approx(prices["WBTCUSDT"].price, 65275.31));
        assert!(approx(prices["RHEAUSDT"].price, 0.012311));
        assert!(approx(prices["AURORAUSDT"].price, 0.01801));
        assert_eq!(prices["NEARUSDT"].timestamp, None);

        // A zero price is a missing source, never a value
        let dead = r#"[{"symbol":"DEADUSDT","price":"0"},{"symbol":"NEARUSDT","price":"1.848"}]"#;
        let prices = parse_mexc_batch(dead, &["DEADUSDT", "NEARUSDT"]).unwrap();
        assert!(!prices.contains_key("DEADUSDT"));
        assert_eq!(prices.len(), 1);

        assert!(approx(
            parse_mexc(&json!({"symbol":"NEARUSDT","price":"1.8476"})).unwrap(),
            1.8476
        ));
        assert!(parse_mexc(&json!({"symbol":"DEADUSDT","price":"0"})).is_err());
    }

    #[test]
    fn test_new_source_batch_urls() {
        assert_eq!(
            coinbase_batch_url(),
            "https://api.exchange.coinbase.com/products/stats"
        );
        assert_eq!(bitstamp_batch_url(), "https://www.bitstamp.net/api/v2/ticker/");
        assert_eq!(
            okx_batch_url(),
            "https://www.okx.com/api/v5/market/tickers?instType=SPOT"
        );
        assert_eq!(
            bitget_batch_url(),
            "https://api.bitget.com/api/v2/spot/market/tickers"
        );
        assert_eq!(mexc_batch_url(), "https://api.mexc.com/api/v3/ticker/price");
        assert_eq!(
            okx_url("NEAR-USDT"),
            "https://www.okx.com/api/v5/market/ticker?instId=NEAR-USDT"
        );
        assert_eq!(
            bitget_url("NEARUSDT"),
            "https://api.bitget.com/api/v2/spot/market/tickers?symbol=NEARUSDT"
        );
        assert_eq!(
            mexc_url("NEARUSDT"),
            "https://api.mexc.com/api/v3/ticker/price?symbol=NEARUSDT"
        );
        assert_eq!(
            kraken_url("NEARUSD"),
            "https://api.kraken.com/0/public/Ticker?pair=NEARUSD"
        );
    }

    #[test]
    fn test_chainlink_multicall_body_layout() {
        let feeds = [
            "0x5f4eC3Df9cbd43714FE2740f5E3616155c5b8419",
            "0xF4030086522a5bEEa4988F8cA5B36dbC97BeE88c",
        ];
        let body = chainlink_multicall_body(&feeds).unwrap();

        assert_eq!(body["params"][0]["to"], MULTICALL3_ADDRESS);
        let data = body["params"][0]["data"].as_str().unwrap();
        let hex = data.strip_prefix("0x").unwrap();
        assert!(hex.starts_with("82ad56cb")); // aggregate3((address,bool,bytes)[])

        let words: Vec<&str> = abi_words(&hex[8..]).unwrap();
        assert_eq!(word_to_u128(words[0]).unwrap(), 0x20); // offset to the array
        assert_eq!(word_to_u128(words[1]).unwrap(), 2); // two calls
        assert_eq!(word_to_u128(words[2]).unwrap(), 64); // first tuple: 32 * 2
        assert_eq!(word_to_u128(words[3]).unwrap(), 224); // second: 32 * 2 + 160
        assert!(words[4].ends_with("5f4ec3df9cbd43714fe2740f5e3616155c5b8419"));
        assert_eq!(word_to_u128(words[5]).unwrap(), 1); // allowFailure MUST be true
        assert_eq!(word_to_u128(words[7]).unwrap(), 4); // callData length
        assert!(words[8].starts_with("feaf968c")); // latestRoundData()

        // A malformed address is refused instead of producing a call to address zero
        assert!(chainlink_multicall_body(&["0xnothex"]).is_err());
    }

    #[test]
    fn test_parse_chainlink_multicall_keeps_live_feeds_when_one_reverts() {
        // Real eth_call response: ETH/USD and BTC/USD answered, the delisted XRP/USD feed
        // reverted (success = false, empty returnData) — which with allowFailure = false
        // would have reverted the entire multicall
        let result = [
            "0000000000000000000000000000000000000000000000000000000000000020",
            "0000000000000000000000000000000000000000000000000000000000000003",
            "0000000000000000000000000000000000000000000000000000000000000060",
            "0000000000000000000000000000000000000000000000000000000000000160",
            "0000000000000000000000000000000000000000000000000000000000000260",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "0000000000000000000000000000000000000000000000000000000000000040",
            "00000000000000000000000000000000000000000000000000000000000000a0",
            "0000000000000000000000000000000000000000000000070000000000007cc1",
            "0000000000000000000000000000000000000000000000000000002da39c073e",
            "000000000000000000000000000000000000000000000000000000006a671e29",
            "000000000000000000000000000000000000000000000000000000006a671e3f",
            "0000000000000000000000000000000000000000000000070000000000007cc1",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "0000000000000000000000000000000000000000000000000000000000000040",
            "00000000000000000000000000000000000000000000000000000000000000a0",
            "0000000000000000000000000000000000000000000000070000000000005933",
            "000000000000000000000000000000000000000000000000000005eab7c98868",
            "000000000000000000000000000000000000000000000000000000006a671e31",
            "000000000000000000000000000000000000000000000000000000006a671e3f",
            "0000000000000000000000000000000000000000000000070000000000005933",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "0000000000000000000000000000000000000000000000000000000000000040",
            "0000000000000000000000000000000000000000000000000000000000000000",
        ]
        .concat();
        let json = json!({ "jsonrpc": "2.0", "id": 1, "result": format!("0x{}", result) });

        let eth = "0x5f4eC3Df9cbd43714FE2740f5E3616155c5b8419";
        let btc = "0xF4030086522a5bEEa4988F8cA5B36dbC97BeE88c";
        let xrp = "0xCed2660c6Dd1Ffd856A5A82C67f3482d88C50b12";
        let batch = parse_chainlink_multicall(&json, &[eth, btc, xrp]).unwrap();

        // answer is word 2 of latestRoundData(), 8 decimals; updatedAt is word 4
        assert!(approx(batch.prices[eth].price, 1960.18439998));
        assert_eq!(batch.prices[eth].timestamp, Some(1785142847));
        assert!(approx(batch.prices[btc].price, 65056.63924328));
        assert!(!batch.prices.contains_key(xrp));

        // The dead feed is a named per-feed failure, not a failure of the whole batch
        assert_eq!(batch.failures.len(), 1);
        assert_eq!(batch.failures[0].0, xrp);
        assert!(batch.failures[0].1.contains("reverted"));
    }

    #[test]
    fn test_parse_chainlink_multicall_rejects_mismatched_response() {
        // Two results for three feeds would silently shift every price by one address
        let result = [
            "0000000000000000000000000000000000000000000000000000000000000020",
            "0000000000000000000000000000000000000000000000000000000000000002",
        ]
        .concat();
        let json = json!({ "result": format!("0x{}", result) });
        assert!(parse_chainlink_multicall(&json, &["0x1", "0x2", "0x3"]).is_err());

        // An RPC-level error is surfaced instead of being read as a missing result
        let json = json!({ "error": { "code": -32000, "message": "header not found" } });
        assert!(parse_chainlink_multicall(&json, &["0x1"]).is_err());
    }

    #[test]
    fn test_parse_chainlink_reports_dead_feed_clearly() {
        // A delisted feed answers with no `result` at all
        let json = json!({ "jsonrpc": "2.0", "id": 1 });
        let error = parse_chainlink(&json).unwrap_err().to_string();
        assert!(error.contains("no data"), "unexpected error: {}", error);

        // ...or with empty return data
        let error = parse_chainlink(&json!({ "result": "0x" })).unwrap_err().to_string();
        assert!(error.contains("no data"), "unexpected error: {}", error);

        // A full word of zeros is a zero price, not a malformed response
        let error = parse_chainlink(&json!({ "result": format!("0x{}", "0".repeat(64)) }))
            .unwrap_err()
            .to_string();
        assert!(error.contains("zero price"), "unexpected error: {}", error);

        // A revert is deterministic, so the caller must not rotate to the next RPC
        assert!(is_execution_revert(&json!({ "code": 3, "message": "execution reverted" })));
        assert!(is_execution_revert(&json!({ "code": -32000, "message": "execution reverted" })));
        assert!(!is_execution_revert(&json!({ "code": -32000, "message": "header not found" })));
    }
}
