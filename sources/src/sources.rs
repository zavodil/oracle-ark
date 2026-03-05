//! High-level source fetch functions
//!
//! This module provides both sync (WASI) and async (scheduler) implementations.

use crate::{parsers, token_map, SourcePrice};
#[cfg(feature = "wasi")]
use crate::{CustomSourceConfig, HttpResponse};
use anyhow::Result;
use std::time::{SystemTime, UNIX_EPOCH};

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

// ============================================================================
// WASI (sync) implementation
// ============================================================================

#[cfg(feature = "wasi")]
pub mod sync {
    use super::*;
    use crate::{LAST_CHAINLINK_RPC, CHAINLINK_DISABLED};
    use std::time::Duration;
    use wasi_http_client::Client;

    fn http_get(url: &str) -> Result<HttpResponse> {
        let response = Client::new()
            .get(url)
            .connect_timeout(Duration::from_secs(10))
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
            .header("Accept-Encoding", "identity")
            .connect_timeout(Duration::from_secs(10))
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

        // Check freshness (120 seconds)
        let now = current_timestamp();
        if now - publish_time > 120 {
            anyhow::bail!("Pyth price is stale (published {} seconds ago)", now - publish_time);
        }

        Ok(SourcePrice {
            source_name: "pyth".to_string(),
            price,
            timestamp: publish_time,
        })
    }

    /// Try a single Chainlink RPC, returns Ok(price) or Err
    fn try_chainlink_rpc(rpc_url: &str, body_str: &str) -> Result<f64> {
        let response = Client::new()
            .post(rpc_url)
            .header("Content-Type", "application/json")
            .connect_timeout(Duration::from_secs(10))
            .body(body_str.as_bytes())
            .send()
            .map_err(|e| anyhow::anyhow!("{}: {}", rpc_url, e))?;

        let status = response.status();
        if status < 200 || status >= 300 {
            anyhow::bail!("{}: HTTP {}", rpc_url, status);
        }

        let json: serde_json::Value = serde_json::from_slice(&response.body()?)
            .map_err(|e| anyhow::anyhow!("{}: parse error: {}", rpc_url, e))?;

        if let Some(error) = json.get("error") {
            anyhow::bail!("{}: RPC error: {}", rpc_url, error);
        }

        parsers::parse_chainlink(&json)
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
                Err(e) => {
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

    pub fn fetch_custom(config: &CustomSourceConfig, api_key: Option<&str>) -> Result<SourcePrice> {
        let mut request = match config.method.to_uppercase().as_str() {
            "GET" => Client::new().get(&config.url),
            "POST" => {
                let mut req = Client::new().post(&config.url);
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

        // Add custom headers
        for (key, value) in &config.headers {
            request = request.header(key.as_str(), value.as_str());
        }

        // Add API_KEY as Bearer token if present
        if let Some(key) = api_key {
            let auth = format!("Bearer {}", key);
            request = request.header("Authorization", auth.as_str());
        }

        let response = request.connect_timeout(Duration::from_secs(10)).send()?;

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

    /// Fetch price from all available sources for a token
    pub fn fetch_all_sources(token: &str, api_key: Option<&str>) -> Vec<SourcePrice> {
        let mut prices = Vec::new();

        // CoinGecko
        if let Some(cg_id) = token_map::get_coingecko_id(token) {
            if let Ok(p) = fetch_coingecko(cg_id, api_key) {
                prices.push(p);
            }
        }

        // Binance
        if let Some(symbol) = token_map::get_binance_symbol(token) {
            if let Ok(p) = fetch_binance(symbol) {
                prices.push(p);
            }
        }

        // Binance US
        if let Some(symbol) = token_map::get_binance_us_symbol(token) {
            if let Ok(p) = fetch_binance_us(symbol) {
                prices.push(p);
            }
        }

        // Binance Alpha (for tokens like Rhea)
        if let Some(address) = token_map::get_binance_alpha_address(token) {
            if let Ok(p) = fetch_binance_alpha(address) {
                prices.push(p);
            }
        }

        // Pyth
        if let Some(price_id) = token_map::get_pyth_id(token) {
            if let Ok(p) = fetch_pyth(price_id) {
                prices.push(p);
            }
        }

        // Chainlink (Ethereum on-chain oracle)
        if let Some(feed_address) = token_map::get_chainlink_address(token) {
            if let Ok(p) = fetch_chainlink(feed_address) {
                prices.push(p);
            }
        }

        // Huobi
        if let Some(symbol) = token_map::get_huobi_symbol(token) {
            if let Ok(p) = fetch_huobi(symbol) {
                prices.push(p);
            }
        }

        // KuCoin
        if let Some(symbol) = token_map::get_kucoin_symbol(token) {
            if let Ok(p) = fetch_kucoin(symbol) {
                prices.push(p);
            }
        }

        // Gate.io
        if let Some(pair) = token_map::get_gate_pair(token) {
            if let Ok(p) = fetch_gate(pair) {
                prices.push(p);
            }
        }

        // Crypto.com
        if let Some(instrument) = token_map::get_cryptocom_instrument(token) {
            if let Ok(p) = fetch_cryptocom(instrument) {
                prices.push(p);
            }
        }

        prices
    }
}

// ============================================================================
// Async (scheduler) implementation
// ============================================================================

#[cfg(feature = "async")]
pub mod r#async {
    use super::*;

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
        if now - publish_time > 120 {
            anyhow::bail!("Pyth price is stale (published {} seconds ago)", now - publish_time);
        }

        Ok(SourcePrice {
            source_name: "pyth".to_string(),
            price,
            timestamp: publish_time,
        })
    }

    /// Try a single Chainlink RPC (async)
    async fn try_chainlink_rpc_async(
        client: &reqwest::Client,
        rpc_url: &str,
        body: &serde_json::Value,
    ) -> Result<f64> {
        let response = client
            .post(rpc_url)
            .json(body)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("{}: {}", rpc_url, e))?;

        if !response.status().is_success() {
            anyhow::bail!("{}: HTTP {}", rpc_url, response.status());
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("{}: parse error: {}", rpc_url, e))?;

        if let Some(error) = json.get("error") {
            anyhow::bail!("{}: RPC error: {}", rpc_url, error);
        }

        parsers::parse_chainlink(&json)
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
                Err(e) => {
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

    /// Fetch price from all available sources for a token
    pub async fn fetch_all_sources(
        client: &reqwest::Client,
        token: &str,
        api_key: Option<&str>,
    ) -> Vec<SourcePrice> {
        let mut prices = Vec::new();

        // CoinGecko
        if let Some(cg_id) = token_map::get_coingecko_id(token) {
            if let Ok(p) = fetch_coingecko(client, cg_id, api_key).await {
                prices.push(p);
            }
        }

        // Binance
        if let Some(symbol) = token_map::get_binance_symbol(token) {
            if let Ok(p) = fetch_binance(client, symbol).await {
                prices.push(p);
            }
        }

        // Binance US
        if let Some(symbol) = token_map::get_binance_us_symbol(token) {
            if let Ok(p) = fetch_binance_us(client, symbol).await {
                prices.push(p);
            }
        }

        // Binance Alpha (for tokens like Rhea)
        if let Some(address) = token_map::get_binance_alpha_address(token) {
            if let Ok(p) = fetch_binance_alpha(client, address).await {
                prices.push(p);
            }
        }

        // Pyth
        if let Some(price_id) = token_map::get_pyth_id(token) {
            if let Ok(p) = fetch_pyth(client, price_id).await {
                prices.push(p);
            }
        }

        // Chainlink (Ethereum on-chain oracle)
        if let Some(feed_address) = token_map::get_chainlink_address(token) {
            if let Ok(p) = fetch_chainlink(client, feed_address).await {
                prices.push(p);
            }
        }

        // Huobi
        if let Some(symbol) = token_map::get_huobi_symbol(token) {
            if let Ok(p) = fetch_huobi(client, symbol).await {
                prices.push(p);
            }
        }

        // KuCoin
        if let Some(symbol) = token_map::get_kucoin_symbol(token) {
            if let Ok(p) = fetch_kucoin(client, symbol).await {
                prices.push(p);
            }
        }

        // Gate.io
        if let Some(pair) = token_map::get_gate_pair(token) {
            if let Ok(p) = fetch_gate(client, pair).await {
                prices.push(p);
            }
        }

        // Crypto.com
        if let Some(instrument) = token_map::get_cryptocom_instrument(token) {
            if let Ok(p) = fetch_cryptocom(client, instrument).await {
                prices.push(p);
            }
        }

        prices
    }
}
