//! URL builders and response parsers for each price source
//!
//! These functions are used by both WASI and scheduler.

use anyhow::{anyhow, Result};
use serde_json::Value;

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
    json.get(token_id)
        .and_then(|v| v.get("usd"))
        .and_then(|v| v.as_f64())
        .ok_or_else(|| anyhow!("Price not found for {} in CoinGecko response", token_id))
}

/// Build Binance API URL
pub fn binance_url(symbol: &str) -> String {
    format!("https://api.binance.com/api/v3/ticker/price?symbol={}", symbol)
}

/// Parse Binance response: {"symbol": "BTCUSDT", "price": "100000.00"}
pub fn parse_binance(json: &Value) -> Result<f64> {
    json.get("price")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or_else(|| anyhow!("Price not found in Binance response"))
}

/// Build Binance US API URL (same format as Binance, different domain)
pub fn binance_us_url(symbol: &str) -> String {
    format!("https://api.binance.us/api/v3/ticker/price?symbol={}", symbol)
}

/// Parse Binance US response (same format as Binance)
pub fn parse_binance_us(json: &Value) -> Result<f64> {
    parse_binance(json)
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
                return token
                    .get("price")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<f64>().ok())
                    .ok_or_else(|| anyhow!("Price not found for token {}", contract_address));
            }
        }
    }

    Err(anyhow!("Token {} not found in Binance Alpha response", contract_address))
}

/// Build Pyth Hermes API URL
pub fn pyth_url(price_id: &str) -> String {
    format!("https://hermes.pyth.network/v2/updates/price/latest?ids[]={}", price_id)
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

/// Build Huobi API URL
pub fn huobi_url(symbol: &str) -> String {
    format!("https://api.huobi.pro/market/detail/merged?symbol={}", symbol)
}

/// Parse Huobi response - uses mid price (bid + ask) / 2
pub fn parse_huobi(json: &Value) -> Result<f64> {
    let bid = json
        .get("tick")
        .and_then(|v| v.get("bid"))
        .and_then(|v| v.get(0))
        .and_then(|v| v.as_f64());

    let ask = json
        .get("tick")
        .and_then(|v| v.get("ask"))
        .and_then(|v| v.get(0))
        .and_then(|v| v.as_f64());

    match (bid, ask) {
        (Some(b), Some(a)) => Ok((b + a) / 2.0),
        _ => Err(anyhow!("Bid/Ask not found in Huobi response")),
    }
}

/// Build KuCoin API URL
pub fn kucoin_url(symbol: &str) -> String {
    format!("https://api.kucoin.com/api/v1/market/orderbook/level1?symbol={}", symbol)
}

/// Parse KuCoin response
pub fn parse_kucoin(json: &Value) -> Result<f64> {
    let data = json.get("data").ok_or_else(|| anyhow!("No data in KuCoin response"))?;

    let bid = data
        .get("bestBid")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok());

    let ask = data
        .get("bestAsk")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok());

    let last = data
        .get("price")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok());

    match (bid, ask, last) {
        (Some(b), Some(a), Some(l)) => Ok((b + a + l) / 3.0),
        (Some(b), Some(a), None) => Ok((b + a) / 2.0),
        (_, _, Some(l)) => Ok(l),
        _ => Err(anyhow!("Price not found in KuCoin response")),
    }
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

    let bid = json.get("highestBid").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok());
    let ask = json.get("lowestAsk").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok());
    let last = json.get("last").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok());

    match (bid, ask, last) {
        (Some(b), Some(a), Some(l)) => Ok((b + a + l) / 3.0),
        (Some(b), Some(a), None) => Ok((b + a) / 2.0),
        (_, _, Some(l)) => Ok(l),
        _ => Err(anyhow!("Price not found in Gate response")),
    }
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

    let bid = data.get("b").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok());
    let ask = data.get("k").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok());
    let last = data.get("a").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok());

    match (bid, ask, last) {
        (Some(b), Some(k), Some(a)) => Ok((b + k + a) / 3.0),
        (Some(b), Some(k), None) => Ok((b + k) / 2.0),
        (_, _, Some(a)) => Ok(a),
        _ => Err(anyhow!("Price not found in Crypto.com response")),
    }
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
}
