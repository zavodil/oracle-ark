//! Oracle Scheduler - Proactive price updates via OutLayer WASI
//!
//! This scheduler monitors price changes and triggers WASI oracle updates
//! when prices change significantly or after a time interval.
//!
//! Key principle: Scheduler does NOT pass prices to WASI!
//! It only compares prices and triggers WASI to fetch its own prices in TEE.

mod sources;
mod telegram;
mod token_config;

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use futures::future::join_all;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::time::{Duration, Instant};
use token_config::TokensConfig;
use tracing::{debug, error, info, warn};

/// Configuration loaded from environment
#[derive(Debug, Clone)]
struct Config {
    /// Coordinator URL for reading public storage and WASI calls
    coordinator_url: String,

    /// Project owner (e.g., "alice.near")
    project_owner: String,

    /// Project name (e.g., "oracle-ark")
    project_name: String,

    /// Project UUID for reading public storage
    project_uuid: String,

    /// Payment key for WASI calls (format: owner:nonce:secret)
    payment_key: String,

    /// Tokens configuration loaded from tokens.json
    tokens_config: TokensConfig,

    /// Update interval in seconds (time-based trigger)
    update_interval_secs: u64,

    /// Price difference threshold percentage (price-based trigger)
    price_diff_threshold_percent: f64,

    /// Whether to also update the contract via WASI
    update_contract_enabled: bool,

    /// Contract ID to update (if update_contract_enabled)
    oracle_contract_id: Option<String>,

    /// Aggregation method: "median", "average", "weighted_average"
    aggregation_method: String,

    /// Minimum number of sources required
    min_sources_num: u8,

    /// Telegram bot token for alerts (optional)
    telegram_bot_token: Option<String>,

    /// Telegram chat ID for alerts (optional)
    telegram_chat_id: Option<String>,
}

impl Config {
    fn from_env() -> Result<Self> {
        // Load tokens from tokens.json (shared with WASI)
        let tokens_path = env::var("TOKENS_CONFIG")
            .unwrap_or_else(|_| "../tokens.json".to_string());
        let tokens_config = TokensConfig::load(&tokens_path)
            .with_context(|| format!("Failed to load tokens from {}", tokens_path))?;

        Ok(Self {
            coordinator_url: env::var("COORDINATOR_URL")
                .unwrap_or_else(|_| "https://api.outlayer.fastnear.com".to_string()),
            project_owner: env::var("PROJECT_OWNER").context("PROJECT_OWNER not set")?,
            project_name: env::var("PROJECT_NAME").context("PROJECT_NAME not set")?,
            project_uuid: env::var("PROJECT_UUID").context("PROJECT_UUID not set")?,
            payment_key: env::var("PAYMENT_KEY").context("PAYMENT_KEY not set")?,
            tokens_config,
            update_interval_secs: env::var("UPDATE_INTERVAL_SECS")
                .unwrap_or_else(|_| "60".to_string())
                .parse()
                .unwrap_or(60),
            price_diff_threshold_percent: env::var("PRICE_DIFF_THRESHOLD_PERCENT")
                .unwrap_or_else(|_| "1.0".to_string())
                .parse()
                .unwrap_or(1.0),
            update_contract_enabled: env::var("UPDATE_CONTRACT_ENABLED")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            oracle_contract_id: env::var("ORACLE_CONTRACT_ID").ok(),
            aggregation_method: env::var("AGGREGATION_METHOD")
                .unwrap_or_else(|_| "median".to_string()),
            min_sources_num: env::var("MIN_SOURCES_NUM")
                .unwrap_or_else(|_| "1".to_string())
                .parse()
                .unwrap_or(1),
            telegram_bot_token: env::var("TELEGRAM_BOT_TOKEN").ok(),
            telegram_chat_id: env::var("TELEGRAM_CHAT_ID").ok(),
        })
    }
}

/// Price stored in WASI public storage
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct StoredPrice {
    price: f64,
    timestamp: u64,
    sources: Vec<SourceInfo>,
    aggregation_method: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct SourceInfo {
    name: String,
    price: f64,
}

/// Response item from batch storage API
#[derive(Debug, Deserialize)]
struct BatchStorageItem {
    exists: bool,
    value: Option<String>,
}

/// Response from coordinator batch storage API
#[derive(Debug, Deserialize)]
struct BatchStorageResponse {
    results: HashMap<String, BatchStorageItem>,
}

/// Response from WASI call
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct WasiCallResponse {
    call_id: Option<String>,
    status: Option<String>,
    output: Option<serde_json::Value>,
    error: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("oracle_scheduler=info".parse().unwrap()),
        )
        .init();

    let config = Config::from_env()?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let tokens = config.tokens_config.token_ids();

    info!("Starting oracle scheduler");
    info!("Coordinator: {}", config.coordinator_url);
    info!("Project: {}/{}", config.project_owner, config.project_name);
    info!("Tokens: {} configured", tokens.len());
    for token in &tokens {
        let cfg = config.tokens_config.get(token).unwrap();
        let sources: Vec<&str> = [
            cfg.coingecko.as_ref().map(|_| "coingecko"),
            cfg.binance.as_ref().map(|_| "binance"),
            cfg.pyth.as_ref().map(|_| "pyth"),
            cfg.huobi.as_ref().map(|_| "huobi"),
            cfg.kucoin.as_ref().map(|_| "kucoin"),
            cfg.gate.as_ref().map(|_| "gate"),
            cfg.cryptocom.as_ref().map(|_| "cryptocom"),
        ]
        .into_iter()
        .flatten()
        .collect();
        debug!("  {} -> {} sources: {:?}", token, sources.len(), sources);
    }
    info!(
        "Update interval: {}s, threshold: {}%",
        config.update_interval_secs, config.price_diff_threshold_percent
    );
    info!("Update contract: {}", config.update_contract_enabled);
    info!(
        "Telegram alerts: {}",
        if config.telegram_bot_token.is_some() { "enabled" } else { "disabled" }
    );

    // Track last update time per token
    let mut last_update: HashMap<String, Instant> = HashMap::new();

    // Track consecutive failures for alerting
    let mut consecutive_failures: u32 = 0;
    const ALERT_THRESHOLD: u32 = 3; // Alert after 3 consecutive failures

    // Main loop - poll every 10 seconds
    let poll_interval = Duration::from_secs(10);

    loop {
        match poll_and_update(&client, &config, &mut last_update).await {
            Ok(_) => {
                consecutive_failures = 0;
            }
            Err(e) => {
                error!("Poll cycle failed: {}", e);
                consecutive_failures += 1;

                // Send Telegram alert after threshold failures
                if consecutive_failures == ALERT_THRESHOLD {
                    telegram::send_alert(
                        &client,
                        config.telegram_bot_token.as_deref(),
                        config.telegram_chat_id.as_deref(),
                        "Oracle Scheduler Error",
                        &format!(
                            "Project: {}/{}\n{} consecutive failures\nLast error: {}",
                            config.project_owner, config.project_name, consecutive_failures, e
                        ),
                    )
                    .await;
                }
            }
        }

        tokio::time::sleep(poll_interval).await;
    }
}

async fn poll_and_update(
    client: &reqwest::Client,
    config: &Config,
    last_update: &mut HashMap<String, Instant>,
) -> Result<()> {
    let tokens = config.tokens_config.token_ids();
    let api_key = env::var("API_KEY").ok();

    // 1. Fetch current prices from external sources in parallel (for comparison only!)
    let fetch_futures: Vec<_> = tokens
        .iter()
        .map(|token| {
            let token = token.clone();
            let api_key = api_key.clone();
            async move {
                let result =
                    sources::fetch_price(client, &token, &config.tokens_config, api_key.as_deref())
                        .await;
                (token, result)
            }
        })
        .collect();

    let fetch_results = join_all(fetch_futures).await;

    let mut current_prices: HashMap<String, f64> = HashMap::new();
    for (token, result) in fetch_results {
        match result {
            Ok(price) => {
                current_prices.insert(token, price);
            }
            Err(e) => {
                warn!("Failed to fetch {} from sources: {}", token, e);
            }
        }
    }

    if current_prices.is_empty() {
        warn!("No current prices fetched from any source");

        // Alert if we couldn't get any prices
        telegram::send_alert(
            client,
            config.telegram_bot_token.as_deref(),
            config.telegram_chat_id.as_deref(),
            "No Prices Available",
            &format!(
                "Project: {}/{}\nScheduler failed to fetch prices from any external source.\nCheck API connectivity.",
                config.project_owner, config.project_name
            ),
        )
        .await;

        return Ok(());
    }

    // 2. Batch read all stored prices from WASI public storage
    let tokens_to_read: Vec<&str> = current_prices.keys().map(|s| s.as_str()).collect();
    let stored_prices = read_public_storage_batch(client, config, &tokens_to_read).await?;

    // 3. Check triggers for each token
    let mut tokens_to_update = Vec::new();

    for (token, current_price) in &current_prices {
        let stored_price = stored_prices.get(token);

        let time_since_last = last_update
            .get(token)
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(u64::MAX);

        let time_trigger = time_since_last >= config.update_interval_secs;

        let price_trigger = match stored_price {
            Some(stored) => {
                let diff = ((current_price - stored.price) / stored.price).abs() * 100.0;
                debug!(
                    "{}: current={:.4}, stored={:.4}, diff={:.2}%",
                    token, current_price, stored.price, diff
                );
                diff > config.price_diff_threshold_percent
            }
            None => {
                info!("{}: no stored price, will trigger update", token);
                true
            }
        };

        if time_trigger || price_trigger {
            let reason = if price_trigger && !time_trigger {
                "price change"
            } else if time_trigger && !price_trigger {
                "interval"
            } else {
                "price change + interval"
            };
            info!(
                "{}: triggering update (reason: {}, current={:.4})",
                token, reason, current_price
            );
            tokens_to_update.push(token.clone());
        }
    }

    // 4. Trigger WASI update if needed
    if !tokens_to_update.is_empty() {
        info!("Triggering WASI update for {} tokens", tokens_to_update.len());

        match call_wasi_update(client, config, &tokens_to_update).await {
            Ok(_) => {
                info!("Triggering WASI update successful");
                let now = Instant::now();
                for token in &tokens_to_update {
                    last_update.insert(token.clone(), now);
                }
            }
            Err(e) => {
                error!("WASI update failed: {}", e);

                // Send Telegram alert for WASI failures
                telegram::send_alert(
                    client,
                    config.telegram_bot_token.as_deref(),
                    config.telegram_chat_id.as_deref(),
                    "WASI Update Failed",
                    &format!(
                        "Project: {}/{}\nTokens: {:?}\nError: {}",
                        config.project_owner, config.project_name, tokens_to_update, e
                    ),
                )
                .await;
            }
        }
    }

    Ok(())
}

async fn read_public_storage_batch(
    client: &reqwest::Client,
    config: &Config,
    tokens: &[&str],
) -> Result<HashMap<String, StoredPrice>> {
    let url = format!("{}/public/storage/batch", config.coordinator_url);

    let keys: Vec<String> = tokens.iter().map(|t| format!("price:{}", t)).collect();

    let body = serde_json::json!({
        "project_uuid": config.project_uuid,
        "keys": keys,
    });

    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status, text);
    }

    let batch_resp: BatchStorageResponse = resp.json().await?;

    let mut result = HashMap::new();

    for token in tokens {
        let key = format!("price:{}", token);
        if let Some(item) = batch_resp.results.get(&key) {
            if item.exists {
                if let Some(value_b64) = &item.value {
                    match BASE64.decode(value_b64) {
                        Ok(value_bytes) => match serde_json::from_slice::<StoredPrice>(&value_bytes)
                        {
                            Ok(stored) => {
                                result.insert(token.to_string(), stored);
                            }
                            Err(e) => {
                                warn!("Failed to parse stored price for {}: {}", token, e);
                            }
                        },
                        Err(e) => {
                            warn!("Failed to decode base64 for {}: {}", token, e);
                        }
                    }
                }
            }
        }
    }

    Ok(result)
}

async fn call_wasi_update(
    client: &reqwest::Client,
    config: &Config,
    tokens: &[String],
) -> Result<()> {
    let url = format!(
        "{}/call/{}/{}",
        config.coordinator_url, config.project_owner, config.project_name
    );

    // Build WASI input - NOTE: we do NOT pass prices!
    // WASI will fetch its own prices from sources inside TEE
    let input = serde_json::json!({
        "command": "update_prices",
        "tokens": tokens,
        "update_contract": config.update_contract_enabled,
        "contract_id": config.oracle_contract_id,
        "aggregation_method": config.aggregation_method,
        "min_sources_num": config.min_sources_num,
    });

    let body = serde_json::json!({
        "input": input,
        "async": false,  // Wait for result
    });

    debug!("Calling WASI: {}", serde_json::to_string_pretty(&body)?);

    let resp = client
        .post(&url)
        .header("X-Payment-Key", &config.payment_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status, text);
    }

    let wasi_resp: WasiCallResponse = resp.json().await?;

    if let Some(error) = wasi_resp.error {
        anyhow::bail!("WASI error: {}", error);
    }

    if let Some(status) = &wasi_resp.status {
        if status == "failed" {
            anyhow::bail!("WASI execution failed");
        }
    }

    debug!("WASI response: {:?}", wasi_resp);
    Ok(())
}
