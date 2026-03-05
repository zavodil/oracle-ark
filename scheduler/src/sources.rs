//! Price sources for scheduler
//!
//! Uses shared oracle-ark-sources crate for parsing logic.
//! Exchange configs loaded from public storage (synced from contract via DAO).

use anyhow::Result;
use oracle_ark_sources::{parsers, ExchangeConfig, SourcePrice};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info};

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Fetch price from all sources and return median
pub async fn fetch_price(
    client: &reqwest::Client,
    token: &str,
    config: &ExchangeConfig,
    api_key: Option<&str>,
) -> Result<f64> {
    let prices = fetch_all_sources(client, config, api_key).await;

    if prices.is_empty() {
        anyhow::bail!("No sources available for {}", token);
    }

    // Log each source price (verbose mode)
    for p in &prices {
        debug!("{} {}: ${:.4}", p.source_name, token, p.price);
    }

    let mut values: Vec<f64> = prices.iter().map(|p| p.price).collect();
    let median = parsers::median(&mut values);

    // Log summary with sources list
    let source_names: Vec<&str> = prices.iter().map(|p| p.source_name.as_str()).collect();
    info!(
        "{}: ${:.6} ({} sources: {})",
        token,
        median,
        prices.len(),
        source_names.join(", ")
    );
    Ok(median)
}

/// Fetch price from all available sources for a token using its ExchangeConfig
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
        match fetch_binance_alpha(client, address).await {
            Ok(p) => prices.push(p),
            Err(e) => debug!("binance_alpha failed: {}", e),
        }
    }

    if let Some(pyth_id) = config.pyth_id() {
        if let Ok(p) = fetch_pyth(client, pyth_id).await {
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

    prices
}

async fn fetch_coingecko(
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

async fn fetch_binance(client: &reqwest::Client, symbol: &str) -> Result<SourcePrice> {
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

async fn fetch_binance_us(client: &reqwest::Client, symbol: &str) -> Result<SourcePrice> {
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

async fn fetch_binance_alpha(client: &reqwest::Client, contract_address: &str) -> Result<SourcePrice> {
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

async fn fetch_pyth(client: &reqwest::Client, price_id: &str) -> Result<SourcePrice> {
    let url = parsers::pyth_url(price_id);
    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!("HTTP {}", response.status());
    }

    let json: serde_json::Value = response.json().await?;
    let (price, publish_time) = parsers::parse_pyth(&json)?;

    let now = current_timestamp();
    if now - publish_time > 120 {
        anyhow::bail!(
            "Pyth price is stale (published {} seconds ago)",
            now - publish_time
        );
    }

    Ok(SourcePrice {
        source_name: "pyth".to_string(),
        price,
        timestamp: publish_time,
    })
}

async fn fetch_huobi(client: &reqwest::Client, symbol: &str) -> Result<SourcePrice> {
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

async fn fetch_kucoin(client: &reqwest::Client, symbol: &str) -> Result<SourcePrice> {
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

async fn fetch_gate(client: &reqwest::Client, pair: &str) -> Result<SourcePrice> {
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

async fn fetch_cryptocom(client: &reqwest::Client, instrument: &str) -> Result<SourcePrice> {
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
