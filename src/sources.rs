//! Custom source fetching for fetch_external command
//!
//! Standard sources (CoinGecko, Binance, Pyth) use the shared oracle-ark-sources crate.
//! This module only handles Custom sources which need special JSON path extraction.

use crate::types::CustomSourceConfig;
// The SSRF guard and the API_KEY allowlist live in `oracle-ark-sources` so that this path and
// `oracle_ark_sources::sources::sync::fetch_custom` cannot drift apart — see that module.
use oracle_ark_sources::security;
use std::env;
use std::error::Error;
use std::time::Duration;
use wasi_http_client::Client;

/// Connect timeout for custom-source requests. See the note in `oracle-ark-sources`: the client
/// offers no read/total timeout, so this only bounds the connect phase; a stalled-after-connect
/// server is bounded by the WASI call's `max_execution_seconds`.
/// See `oracle_ark_sources::sources::USER_AGENT` — providers such as CoinGecko
/// reject requests that carry no User-Agent.
const USER_AGENT: &str = "oracle-ark/1.0 (+https://github.com/zavodil/oracle-ark)";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// Fetch price from custom user-defined source (returns f64)
pub fn fetch_custom(config: &CustomSourceConfig) -> Result<f64, Box<dyn Error>> {
    let value = fetch_custom_raw(config)?;
    parse_as_number(&value)
}

/// Fetch value from custom source, returning appropriate JSON type based on value_type
pub fn fetch_custom_value(config: &CustomSourceConfig) -> Result<serde_json::Value, Box<dyn Error>> {
    let raw_value = fetch_custom_raw(config)?;

    match config.value_type.as_str() {
        "string" => {
            // Return as string
            if let Some(s) = raw_value.as_str() {
                Ok(serde_json::Value::String(s.to_string()))
            } else {
                Ok(serde_json::Value::String(raw_value.to_string()))
            }
        }
        "boolean" => {
            // Return as boolean
            if let Some(b) = raw_value.as_bool() {
                Ok(serde_json::Value::Bool(b))
            } else if let Some(s) = raw_value.as_str() {
                let b = s.eq_ignore_ascii_case("true") || s == "1";
                Ok(serde_json::Value::Bool(b))
            } else {
                Ok(serde_json::Value::Bool(raw_value.as_f64().map(|n| n != 0.0).unwrap_or(false)))
            }
        }
        _ => {
            // Default: "number" - return as JSON number
            let num = parse_as_number(&raw_value)?;
            // Use serde_json::Number to ensure it serializes as a number, not string
            Ok(serde_json::Number::from_f64(num)
                .map(serde_json::Value::Number)
                .unwrap_or_else(|| serde_json::json!(num)))
        }
    }
}

/// Internal: fetch raw JSON value from custom source
fn fetch_custom_raw(config: &CustomSourceConfig) -> Result<serde_json::Value, Box<dyn Error>> {
    // Block requests to local/private network resources (SSRF guard).
    // Standard sources use hard-coded URLs; only custom sources take a caller-supplied URL.
    security::validate_url(&config.url)?;

    // Build HTTP request
    let mut request = match config.method.to_uppercase().as_str() {
        "GET" => Client::new().get(&config.url).header("User-Agent", USER_AGENT),
        "POST" => {
            let mut req = Client::new().post(&config.url).header("User-Agent", USER_AGENT);

            // Add body if provided
            if let Some(body) = &config.body {
                let body_str = serde_json::to_string(body)?;
                req = req.body(body_str.as_bytes());
                // Auto-add Content-Type header if not already provided
                if !config
                    .headers
                    .iter()
                    .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                {
                    req = req.header("Content-Type", "application/json");
                }
            }

            req
        }
        _ => return Err(format!("Unsupported HTTP method: {}", config.method).into()),
    };

    // Add custom headers
    for (key, value) in &config.headers {
        request = request.header(key.as_str(), value.as_str());
    }

    // Auto-add Authorization Bearer if API_KEY is in environment — but only for the
    // providers the key belongs to. The URL is caller-supplied, so attaching it to every
    // request published an OutLayer-managed secret to whoever asked for it.
    if security::may_receive_api_key(&config.url) {
        if let Ok(api_key) = env::var("API_KEY") {
            let auth_header = format!("Bearer {}", api_key);
            request = request.header("Authorization", auth_header.as_str());
        }
    }

    // Send request
    let response = request.connect_timeout(CONNECT_TIMEOUT).send()?;

    // Check status
    let status = response.status();
    if status < 200 || status >= 300 {
        return Err(format!("HTTP {}", status).into());
    }

    // Parse JSON response
    let body = response.body()?;
    let json: serde_json::Value = serde_json::from_slice(&body)?;

    // Extract value using JSON path
    extract_json_path(&json, &config.json_path)
}

/// Extract value from JSON using dot notation path
/// Examples: "price", "data.price", "rates.USD", "blocks.0.author_account_id"
fn extract_json_path(
    json: &serde_json::Value,
    path: &str,
) -> Result<serde_json::Value, Box<dyn Error>> {
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

    Ok(current.clone())
}

/// Parse JSON value as f64
fn parse_as_number(value: &serde_json::Value) -> Result<f64, Box<dyn Error>> {
    if let Some(num) = value.as_f64() {
        Ok(num)
    } else if let Some(s) = value.as_str() {
        // Try to parse string as number
        s.parse::<f64>()
            .map_err(|e| format!("Failed to parse '{}' as number: {}", s, e).into())
    } else if let Some(i) = value.as_i64() {
        Ok(i as f64)
    } else if let Some(u) = value.as_u64() {
        Ok(u as f64)
    } else {
        Err(format!("Value is not a number: {:?}", value).into())
    }
}
