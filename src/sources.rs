use crate::types::{SourcePrice, CustomSourceConfig, ValueType, DataValue};
use serde_json::Value;
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};
use std::time::Duration;
use std::env;
use wasi_http_client::Client;

/// Fetch price from CoinGecko
pub fn fetch_coingecko(token_id: &str, api_key: Option<&str>) -> Result<SourcePrice, Box<dyn Error>> {
    // Build URL - with or without API key
    let url = if let Some(key) = api_key {
        format!(
            "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd&x_cg_pro_api_key={}",
            token_id, key
        )
    } else {
        format!(
            "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd",
            token_id
        )
    };

    // Make HTTP GET request
    let response = Client::new()
        .get(&url)
        .connect_timeout(Duration::from_secs(10))
        .send()?;

    // Check status
    let status = response.status();
    if status < 200 || status >= 300 {
        return Err(format!("HTTP {}", status).into());
    }

    // Parse JSON response
    let body = response.body()?;
    let json: Value = serde_json::from_slice(&body)?;

    // Extract price from response format: {"bitcoin": {"usd": 100000.0}}
    let price = json
        .get(token_id)
        .and_then(|v| v.get("usd"))
        .and_then(|v| v.as_f64())
        .ok_or("Price not found in response")?;

    // Get current timestamp
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs();

    Ok(SourcePrice {
        source_name: "coingecko".to_string(),
        value: DataValue::Number(price),
        timestamp,
    })
}

/// Fetch price from CoinMarketCap
pub fn fetch_coinmarketcap(token_id: &str, api_key: Option<&str>) -> Result<SourcePrice, Box<dyn Error>> {
    // CoinMarketCap requires API key
    let api_key = api_key.ok_or("CoinMarketCap requires API key")?;

    // Build URL
    let url = format!(
        "https://pro-api.coinmarketcap.com/v1/cryptocurrency/quotes/latest?symbol={}&convert=USD",
        token_id
    );

    // Make HTTP GET request with API key header
    let response = Client::new()
        .get(&url)
        .header("X-CMC_PRO_API_KEY", api_key)
        .connect_timeout(Duration::from_secs(10))
        .send()?;

    // Check status
    let status = response.status();
    if status < 200 || status >= 300 {
        return Err(format!("HTTP {}", status).into());
    }

    // Parse JSON response
    let body = response.body()?;
    let json: Value = serde_json::from_slice(&body)?;

    // Extract price from response format:
    // {"data": {"BTC": {"quote": {"USD": {"price": 100000.0}}}}}
    let price = json
        .get("data")
        .and_then(|v| v.get(token_id))
        .and_then(|v| v.get("quote"))
        .and_then(|v| v.get("USD"))
        .and_then(|v| v.get("price"))
        .and_then(|v| v.as_f64())
        .ok_or("Price not found in response")?;

    // Get current timestamp
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs();

    Ok(SourcePrice {
        source_name: "coinmarketcap".to_string(),
        value: DataValue::Number(price),
        
        timestamp,
    })
}

/// Fetch price from TwelveData (commodities, forex, crypto)
pub fn fetch_twelvedata(token_id: &str, api_key: Option<&str>) -> Result<SourcePrice, Box<dyn Error>> {
    // Build URL - with or without API key
    let url = if let Some(key) = api_key {
        format!(
            "https://api.twelvedata.com/price?symbol={}&apikey={}",
            token_id, key
        )
    } else {
        // Free tier endpoint
        format!(
            "https://api.twelvedata.com/price?symbol={}",
            token_id
        )
    };

    // Make HTTP GET request
    let response = Client::new()
        .get(&url)
        .connect_timeout(Duration::from_secs(10))
        .send()?;

    // Check status
    let status = response.status();
    if status < 200 || status >= 300 {
        return Err(format!("HTTP {}", status).into());
    }

    // Parse JSON response
    let body = response.body()?;
    let json: Value = serde_json::from_slice(&body)?;

    // Extract price from response format: {"price": "1850.25"}
    let price_str = json
        .get("price")
        .and_then(|v| v.as_str())
        .ok_or("Price not found in response")?;

    let price: f64 = price_str.parse()?;

    // Get current timestamp
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs();

    Ok(SourcePrice {
        source_name: "twelvedata".to_string(),
        value: DataValue::Number(price),
        
        timestamp,
    })
}

/// Fetch exchange rate from ExchangeRate-API (free, no API key needed)
/// Format: EUR/USD -> base=EUR, target=USD
pub fn fetch_exchangerate_api(token_id: &str, _api_key: Option<&str>) -> Result<SourcePrice, Box<dyn Error>> {
    // Parse token_id format: "EUR/USD" -> base="EUR", target="USD"
    let parts: Vec<&str> = token_id.split('/').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid forex pair format: {}. Expected BASE/TARGET (e.g. EUR/USD)", token_id).into());
    }

    let base_currency = parts[0];
    let target_currency = parts[1];

    // Build URL - free endpoint, no API key needed
    let url = format!("https://open.er-api.com/v6/latest/{}", base_currency);

    // Make HTTP GET request
    let response = Client::new()
        .get(&url)
        .connect_timeout(Duration::from_secs(10))
        .send()?;

    // Check status
    let status = response.status();
    if status < 200 || status >= 300 {
        return Err(format!("HTTP {}", status).into());
    }

    // Parse JSON response
    let body = response.body()?;
    let json: Value = serde_json::from_slice(&body)?;

    // Extract rate from response format: {"rates": {"USD": 1.0542, ...}}
    let rate = json
        .get("rates")
        .and_then(|v| v.get(target_currency))
        .and_then(|v| v.as_f64())
        .ok_or(format!("Rate not found for {}", target_currency))?;

    // Get current timestamp
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs();

    Ok(SourcePrice {
        source_name: "exchangerate-api".to_string(),
        value: DataValue::Number(rate),
        timestamp,
    })
}

/// Fetch price from Binance
pub fn fetch_binance(symbol: &str) -> Result<SourcePrice, Box<dyn Error>> {
    let url = format!("https://api.binance.com/api/v3/ticker/price?symbol={}", symbol);

    let response = Client::new()
        .get(&url)
        .connect_timeout(Duration::from_secs(10))
        .send()?;

    let status = response.status();
    if status < 200 || status >= 300 {
        return Err(format!("HTTP {}", status).into());
    }

    let body = response.body()?;
    let json: Value = serde_json::from_slice(&body)?;

    let price = json
        .get("price")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or("Price not found in response")?;

    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    Ok(SourcePrice {
        source_name: "binance".to_string(),
        value: DataValue::Number(price),
        timestamp,
    })
}

/// Fetch price from Huobi
pub fn fetch_huobi(symbol: &str) -> Result<SourcePrice, Box<dyn Error>> {
    let url = format!("https://api.huobi.pro/market/detail/merged?symbol={}", symbol);

    let response = Client::new()
        .get(&url)
        .connect_timeout(Duration::from_secs(10))
        .send()?;

    let status = response.status();
    if status < 200 || status >= 300 {
        return Err(format!("HTTP {}", status).into());
    }

    let body = response.body()?;
    let json: Value = serde_json::from_slice(&body)?;

    // Get bid and ask prices
    let bid = json.get("tick")
        .and_then(|v| v.get("bid"))
        .and_then(|v| v.get(0))
        .and_then(|v| v.as_f64());

    let ask = json.get("tick")
        .and_then(|v| v.get("ask"))
        .and_then(|v| v.get(0))
        .and_then(|v| v.as_f64());

    let price = match (bid, ask) {
        (Some(b), Some(a)) => (b + a) / 2.0,
        _ => return Err("Bid/Ask not found in response".into()),
    };

    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    Ok(SourcePrice {
        source_name: "huobi".to_string(),
        value: DataValue::Number(price),
        timestamp,
    })
}

/// Fetch price from Crypto.com
pub fn fetch_cryptocom(instrument: &str) -> Result<SourcePrice, Box<dyn Error>> {
    let url = format!("https://api.crypto.com/v2/public/get-ticker?instrument_name={}", instrument);

    let response = Client::new()
        .get(&url)
        .connect_timeout(Duration::from_secs(10))
        .send()?;

    let status = response.status();
    if status < 200 || status >= 300 {
        return Err(format!("HTTP {}", status).into());
    }

    let body = response.body()?;
    let json: Value = serde_json::from_slice(&body)?;

    // Navigate to result.data[0] for the ticker data
    let data = json.get("result")
        .and_then(|v| v.get("data"))
        .and_then(|v| v.get(0))
        .ok_or("Data array not found or empty")?;

    // Get prices: b (bid), k (ask), a (latest price)
    let bid = data.get("b")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok());

    let ask = data.get("k")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok());

    let last = data.get("a")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok());

    let price = match (bid, ask, last) {
        (Some(b), Some(k), Some(a)) => (b + k + a) / 3.0,
        (Some(b), Some(k), None) => (b + k) / 2.0,
        (_, _, Some(a)) => a,
        _ => return Err("Price not found in response".into()),
    };

    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    Ok(SourcePrice {
        source_name: "cryptocom".to_string(),
        value: DataValue::Number(price),
        timestamp,
    })
}

/// Fetch price from KuCoin
pub fn fetch_kucoin(symbol: &str) -> Result<SourcePrice, Box<dyn Error>> {
    let url = format!("https://api.kucoin.com/api/v1/market/orderbook/level1?symbol={}", symbol);

    let response = Client::new()
        .get(&url)
        .connect_timeout(Duration::from_secs(10))
        .send()?;

    let status = response.status();
    if status < 200 || status >= 300 {
        return Err(format!("HTTP {}", status).into());
    }

    let body = response.body()?;
    let json: Value = serde_json::from_slice(&body)?;

    let bid = json.get("data")
        .and_then(|v| v.get("bestBid"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok());

    let ask = json.get("data")
        .and_then(|v| v.get("bestAsk"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok());

    let last = json.get("data")
        .and_then(|v| v.get("price"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok());

    let price = match (bid, ask, last) {
        (Some(b), Some(a), Some(l)) => (b + a + l) / 3.0,
        (Some(b), Some(a), None) => (b + a) / 2.0,
        (_, _, Some(l)) => l,
        _ => return Err("Price not found in response".into()),
    };

    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    Ok(SourcePrice {
        source_name: "kucoin".to_string(),
        value: DataValue::Number(price),
        timestamp,
    })
}

/// Fetch price from Gate.io
pub fn fetch_gate(pair: &str) -> Result<SourcePrice, Box<dyn Error>> {
    let url = format!("https://data.gateapi.io/api2/1/ticker/{}", pair);

    let response = Client::new()
        .get(&url)
        .connect_timeout(Duration::from_secs(10))
        .send()?;

    let status = response.status();
    if status < 200 || status >= 300 {
        return Err(format!("HTTP {}", status).into());
    }

    let body = response.body()?;
    let json: Value = serde_json::from_slice(&body)?;

    // Check if result is successful
    let result = json.get("result")
        .and_then(|v| v.as_str())
        .ok_or("Result not found")?;

    if result != "true" {
        return Err("Gate.io API returned unsuccessful result".into());
    }

    let bid = json.get("highestBid").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok());
    let ask = json.get("lowestAsk").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok());
    let last = json.get("last").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok());

    let price = match (bid, ask, last) {
        (Some(b), Some(a), Some(l)) => (b + a + l) / 3.0,
        (Some(b), Some(a), None) => (b + a) / 2.0,
        (_, _, Some(l)) => l,
        _ => return Err("Price not found in response".into()),
    };

    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    Ok(SourcePrice {
        source_name: "gate".to_string(),
        value: DataValue::Number(price),
        timestamp,
    })
}

/// Fetch price from Pyth Network
pub fn fetch_pyth(price_id: &str) -> Result<SourcePrice, Box<dyn Error>> {
    // Remove 0x prefix if present
    let clean_id = price_id.strip_prefix("0x").unwrap_or(price_id);
    let url = format!("https://hermes.pyth.network/v2/updates/price/latest?ids[]={}", price_id);

    let response = Client::new()
        .get(&url)
        .connect_timeout(Duration::from_secs(10))
        .send()?;

    let status = response.status();
    if status < 200 || status >= 300 {
        return Err(format!("HTTP {}", status).into());
    }

    let body = response.body()?;
    let json: Value = serde_json::from_slice(&body)?;

    // Get price data from parsed array
    let price_data = json.get("parsed")
        .and_then(|v| v.get(0))
        .and_then(|v| v.get("price"))
        .ok_or("Price data not found")?;

    let price_raw = price_data.get("price")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or("Price value not found")?;

    let expo = price_data.get("expo")
        .and_then(|v| v.as_i64())
        .ok_or("Exponent not found")?;

    let publish_time = price_data.get("publish_time")
        .and_then(|v| v.as_u64())
        .ok_or("Publish time not found")?;

    // Check if price is fresh (within 120 seconds)
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    if now - publish_time > 120 {
        return Err(format!("Pyth price is stale (published {} seconds ago)", now - publish_time).into());
    }

    // Calculate decimal price
    let price = price_raw * 10f64.powi(expo as i32);

    Ok(SourcePrice {
        source_name: "pyth".to_string(),
        value: DataValue::Number(price),
        timestamp: publish_time,
    })
}

/// Fetch price from custom user-defined source
pub fn fetch_custom(config: &CustomSourceConfig) -> Result<SourcePrice, Box<dyn Error>> {
    // Build HTTP request
    let mut request = match config.method.to_uppercase().as_str() {
        "GET" => Client::new().get(&config.url),
        "POST" => Client::new().post(&config.url),
        _ => return Err(format!("Unsupported HTTP method: {}", config.method).into()),
    };

    // Add custom headers
    for (key, value) in &config.headers {
        request = request.header(key.as_str(), value.as_str());
    }

    // Auto-add Authorization Bearer if API_KEY is in environment
    if let Ok(api_key) = env::var("API_KEY") {
        eprintln!("✓ API_KEY found, string length: {} characters", api_key.len());
        let auth_header = format!("Bearer {}", api_key);
        request = request.header("Authorization", auth_header.as_str());
    }

    // Send request
    let response = request
        .connect_timeout(Duration::from_secs(10))
        .send()?;

    // Check status
    let status = response.status();
    if status < 200 || status >= 300 {
        return Err(format!("HTTP {}", status).into());
    }

    // Parse JSON response
    let body = response.body()?;
    let json: Value = serde_json::from_slice(&body)?;

    // Extract value using JSON path (e.g. "data.price" or "rates.USD")
    let value = extract_json_value(&json, &config.json_path, &config.value_type)?;

    // Get current timestamp
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs();

    Ok(SourcePrice {
        source_name: "custom".to_string(),
        value,
        timestamp,
    })
}

/// Extract value from JSON using dot notation path
/// Examples: "price", "data.price", "rates.USD", "blocks.0.author_account_id"
fn extract_json_value(json: &Value, path: &str, value_type: &ValueType) -> Result<DataValue, Box<dyn Error>> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = json;

    for part in parts {
        // Try as object key first (string)
        if let Some(next) = current.get(part) {
            current = next;
        } else if let Ok(index) = part.parse::<usize>() {
            // If not found as string key, try as array index
            current = current
                .get(index)
                .ok_or_else(|| format!("JSON path '{}' array index '{}' out of bounds", path, part))?;
        } else {
            return Err(format!("JSON path '{}' not found at '{}'", path, part).into());
        }
    }

    // Extract based on requested type
    match value_type {
        ValueType::Number => {
            if let Some(num) = current.as_f64() {
                Ok(DataValue::Number(num))
            } else if let Some(s) = current.as_str() {
                // Try to parse string as number
                let num = s.parse::<f64>()
                    .map_err(|e| format!("Failed to parse '{}' as number: {}", s, e))?;
                Ok(DataValue::Number(num))
            } else if let Some(i) = current.as_i64() {
                Ok(DataValue::Number(i as f64))
            } else if let Some(u) = current.as_u64() {
                Ok(DataValue::Number(u as f64))
            } else {
                Err(format!("Value at '{}' is not a number", path).into())
            }
        }
        ValueType::String => {
            if let Some(s) = current.as_str() {
                Ok(DataValue::Text(s.to_string()))
            } else {
                // Convert to string representation
                Ok(DataValue::Text(current.to_string()))
            }
        }
        ValueType::Boolean => {
            if let Some(b) = current.as_bool() {
                Ok(DataValue::Boolean(b))
            } else {
                Err(format!("Value at '{}' is not a boolean", path).into())
            }
        }
    }
}

/// Get price fetcher function by source name
pub fn fetch_price(
    source_name: &str,
    token_id: &str,
    api_key: Option<&str>,
) -> Result<SourcePrice, Box<dyn Error>> {
    match source_name {
        "coingecko" => fetch_coingecko(token_id, api_key),
        "coinmarketcap" => fetch_coinmarketcap(token_id, api_key),
        "twelvedata" => fetch_twelvedata(token_id, api_key),
        "exchangerate-api" => fetch_exchangerate_api(token_id, api_key),
        "binance" => fetch_binance(token_id),
        "huobi" => fetch_huobi(token_id),
        "cryptocom" => fetch_cryptocom(token_id),
        "kucoin" => fetch_kucoin(token_id),
        "gate" => fetch_gate(token_id),
        "pyth" => fetch_pyth(token_id),
        _ => Err(format!("Unknown source: {}", source_name).into()),
    }
}

/// Fetch price with custom config support
pub fn fetch_price_with_config(
    source_name: &str,
    token_id: &str,
    api_key: Option<&str>,
    custom_config: Option<&CustomSourceConfig>,
) -> Result<SourcePrice, Box<dyn Error>> {
    if source_name == "custom" {
        let config = custom_config.ok_or("Custom source requires 'custom' config")?;
        fetch_custom(config)
    } else {
        fetch_price(source_name, token_id, api_key)
    }
}
