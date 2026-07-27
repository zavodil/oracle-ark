//! Oracle Scheduler - Proactive price updates via OutLayer WASI
//!
//! This scheduler monitors price changes and triggers WASI oracle updates
//! when prices change significantly or after a time interval.
//!
//! Key principle: Scheduler does NOT pass prices to WASI!
//! It only compares prices and triggers WASI to fetch its own prices in TEE.

mod sources;
mod telegram;

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use futures::future::join_all;
use futures::stream::{self, StreamExt};
use oracle_ark_sources::ExchangeConfig;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::time::{Duration, Instant};
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

    /// Update interval in seconds for normal assets (time-based trigger)
    update_interval_secs: u64,

    /// Update interval for priority assets (NEAR, BTC, ETH) — default: same as update_interval
    update_interval_priority_secs: u64,

    /// Poll interval in seconds when no update is needed (default: 5)
    poll_interval_secs: u64,

    /// Priority asset IDs (updated more frequently)
    priority_assets: Vec<String>,

    /// Price difference threshold percentage (price-based trigger)
    price_diff_threshold_percent: f64,

    /// Max tokens per WASI cache-refresh call. WASI fetches sources sequentially, so a big batch
    /// is slow; splitting it into groups that run concurrently is what keeps the cycle short.
    group_max_tokens: usize,

    /// How many cache-refresh groups may be in flight at once.
    fetch_concurrency: usize,

    /// How often the contract push runs (seconds). Deliberately decoupled from the cache-refresh
    /// cadence: refreshing is cheap and frequent, pushing costs gas and must stay rare.
    contract_push_interval_secs: u64,

    /// Whether to also update the contract via WASI
    update_contract_enabled: bool,

    /// Contract ID to update (if update_contract_enabled)
    oracle_contract_id: Option<String>,

    /// NEAR RPC URL for reading contract state
    near_rpc_url: String,

    /// Secrets profile for WASI calls that need PROTECTED_ keys (e.g., "default")
    secrets_profile: Option<String>,

    /// Secrets account ID (project owner's NEAR account)
    secrets_account_id: Option<String>,

    /// Aggregation method: "median", "average", "weighted_average"
    aggregation_method: String,

    /// Minimum number of sources required
    min_sources_num: u8,

    /// Oracle signer implicit account (hex, 64 chars) to check balance before pushing
    /// When balance < oracle_min_balance_near, contract updates are paused (warm-only mode)
    oracle_signer_account: Option<String>,

    /// Minimum NEAR balance for oracle signer to push prices (default: 0.05)
    oracle_min_balance_near: f64,

    /// Telegram bot token for alerts (optional)
    telegram_bot_token: Option<String>,

    /// Telegram chat ID for alerts (optional)
    telegram_chat_id: Option<String>,
}

impl Config {
    fn from_env() -> Result<Self> {
        Ok(Self {
            coordinator_url: env::var("COORDINATOR_URL")
                .unwrap_or_else(|_| "https://api.outlayer.fastnear.com".to_string()),
            project_owner: env::var("PROJECT_OWNER").context("PROJECT_OWNER not set")?,
            project_name: env::var("PROJECT_NAME").context("PROJECT_NAME not set")?,
            project_uuid: env::var("PROJECT_UUID").context("PROJECT_UUID not set")?,
            payment_key: env::var("PAYMENT_KEY").context("PAYMENT_KEY not set")?,
            update_interval_secs: env::var("UPDATE_INTERVAL_SECS")
                .unwrap_or_else(|_| "60".to_string())
                .parse()
                .unwrap_or(60),
            update_interval_priority_secs: env::var("UPDATE_INTERVAL_PRIORITY_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(|| {
                    env::var("UPDATE_INTERVAL_SECS")
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(60)
                }),
            poll_interval_secs: env::var("POLL_INTERVAL_SECS")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .unwrap_or(5),
            priority_assets: env::var("PRIORITY_ASSETS")
                // ETH is `eth.bridge.near`; the old `aurora` asset it replaced is the Aurora
                // EVM account, not a distinct ETH feed (the AURORA token is a separate asset).
                .unwrap_or_else(|_| "wrap.near,nbtc.bridge.near,eth.bridge.near".to_string())
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            price_diff_threshold_percent: env::var("PRICE_DIFF_THRESHOLD_PERCENT")
                .unwrap_or_else(|_| "1.0".to_string())
                .parse()
                .unwrap_or(1.0),
            // Default high on purpose: the worker now fetches one batch request PER SOURCE for the
            // whole token set, so splitting the set into groups multiplies those requests instead
            // of parallelising work — N groups means N full pulls of every all-tickers endpoint
            // (Coinbase alone is ~107 KB) and pushes rate-limited sources like CoinGecko over the
            // edge, which shows up as some assets silently missing a source. One group = one
            // request per source. Lower it only if a single call approaches the execution limit.
            group_max_tokens: env::var("GROUP_MAX_TOKENS")
                .unwrap_or_else(|_| "64".to_string())
                .parse()
                .unwrap_or(64),
            fetch_concurrency: env::var("FETCH_CONCURRENCY")
                .unwrap_or_else(|_| "3".to_string())
                .parse()
                .unwrap_or(3),
            contract_push_interval_secs: env::var("CONTRACT_PUSH_INTERVAL_SECS")
                .unwrap_or_else(|_| "300".to_string())
                .parse()
                .unwrap_or(300),
            update_contract_enabled: env::var("UPDATE_CONTRACT_ENABLED")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            oracle_contract_id: env::var("ORACLE_CONTRACT_ID").ok(),
            secrets_profile: env::var("SECRETS_PROFILE").ok(),
            secrets_account_id: env::var("SECRETS_ACCOUNT_ID").ok(),
            near_rpc_url: env::var("NEAR_RPC_URL")
                .unwrap_or_else(|_| "https://rpc.mainnet.fastnear.com".to_string()),
            aggregation_method: env::var("AGGREGATION_METHOD")
                .unwrap_or_else(|_| "median".to_string()),
            min_sources_num: env::var("MIN_SOURCES_NUM")
                .unwrap_or_else(|_| "1".to_string())
                .parse()
                .unwrap_or(1),
            oracle_signer_account: env::var("ORACLE_SIGNER_ACCOUNT").ok(),
            oracle_min_balance_near: env::var("ORACLE_MIN_BALANCE_NEAR")
                .unwrap_or_else(|_| "0.05".to_string())
                .parse()
                .unwrap_or(0.05),
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
        .timeout(Duration::from_secs(30)) // All requests are short (async submit + poll)
        // CoinGecko rejects requests with no User-Agent (HTTP 403), and the scheduler queries the
        // same sources as the worker for its price comparison.
        .user_agent("oracle-ark-scheduler/1.0 (+https://github.com/zavodil/oracle-ark)")
        .build()?;

    info!("Starting oracle scheduler");
    info!("Coordinator: {}", config.coordinator_url);
    info!("Project: {}/{}", config.project_owner, config.project_name);
    info!("Exchange configs: loaded from public storage (config:assets)");
    info!(
        "Update intervals: priority={}s, full={}s, poll={}s, threshold={}%",
        config.update_interval_priority_secs,
        config.update_interval_secs,
        config.poll_interval_secs,
        config.price_diff_threshold_percent
    );
    info!("Priority assets: {:?}", config.priority_assets);
    info!("Update contract: {}", config.update_contract_enabled);
    if let Some(ref contract_id) = config.oracle_contract_id {
        info!("Contract asset source: {} (via {})", contract_id, config.near_rpc_url);
    }
    info!(
        "Telegram alerts: {}",
        if config.telegram_bot_token.is_some() { "enabled" } else { "disabled" }
    );

    // Global batch timers (full update vs priority-only)
    let mut last_priority_update: Option<Instant> = None;
    let mut last_full_update: Option<Instant> = None;
    // Separate, much slower timer for the on-chain push — it costs gas, so it must not follow the
    // cache-refresh cadence.
    let mut last_contract_push: Option<Instant> = None;

    // Cached exchange configs from public storage, refreshed every hour
    let mut exchange_configs_cache: Option<HashMap<String, ExchangeConfig>> = None;
    let mut exchange_configs_fetched_at: Option<Instant> = None;
    const EXCHANGE_CONFIGS_CACHE_TTL: Duration = Duration::from_secs(600); // 10 minutes

    // Cached oracle key mapping from contract (asset_id -> key_env_name), refreshed every hour
    let mut oracle_keys_cache: Option<HashMap<String, String>> = None;
    let mut oracle_keys_fetched_at: Option<Instant> = None;
    const ORACLE_KEYS_CACHE_TTL: Duration = Duration::from_secs(3600); // 1 hour

    // Auto-discovered signer accounts (from contract, refreshed with oracle keys)
    let mut signer_accounts_cache: Vec<String> = Vec::new();

    // Balance check for oracle signer (every 5 minutes)
    let mut contract_update_paused = false;
    let mut balance_alert_sent = false;
    let mut balance_check_at: Option<Instant> = None;
    const BALANCE_CHECK_INTERVAL: Duration = Duration::from_secs(300);

    // Track consecutive failures for alerting
    let mut consecutive_failures: u32 = 0;
    const ALERT_THRESHOLD: u32 = 3; // Alert after 3 consecutive failures

    // Throttle repeated telegram alerts (10 min cooldown)
    let mut alert_throttle = telegram::AlertThrottle::new();

    // Main loop - each iteration does one WASI call (if needed),
    // then waits the full interval before the next check.

    loop {
        // Refresh exchange configs from public storage
        let should_refresh_configs = exchange_configs_fetched_at
            .map(|t| t.elapsed() >= EXCHANGE_CONFIGS_CACHE_TTL)
            .unwrap_or(true);
        if should_refresh_configs {
            match fetch_exchange_configs(&client, &config).await {
                Ok(configs) => {
                    info!("Loaded {} exchange configs from public storage", configs.len());
                    for (token, cfg) in &configs {
                        let sources: Vec<&str> = [
                            cfg.coingecko.as_ref().map(|_| "coingecko"),
                            cfg.binance.as_ref().map(|_| "binance"),
                            cfg.pyth.as_ref().map(|_| "pyth"),
                            cfg.huobi.as_ref().map(|_| "huobi"),
                            cfg.kucoin.as_ref().map(|_| "kucoin"),
                            cfg.gate.as_ref().map(|_| "gate"),
                            cfg.cryptocom.as_ref().map(|_| "cryptocom"),
                            cfg.kraken.as_ref().map(|_| "kraken"),
                            cfg.coinbase.as_ref().map(|_| "coinbase"),
                            cfg.bitstamp.as_ref().map(|_| "bitstamp"),
                            cfg.okx.as_ref().map(|_| "okx"),
                            cfg.bitget.as_ref().map(|_| "bitget"),
                            cfg.mexc.as_ref().map(|_| "mexc"),
                        ]
                        .into_iter()
                        .flatten()
                        .collect();
                        debug!("  {} -> {} sources: {:?}", token, sources.len(), sources);
                    }
                    exchange_configs_cache = Some(configs);
                    exchange_configs_fetched_at = Some(Instant::now());
                }
                Err(e) => {
                    if exchange_configs_cache.is_none() {
                        error!("Failed to load exchange configs and no cache available: {}", e);
                        tokio::time::sleep(Duration::from_secs(config.poll_interval_secs)).await;
                        continue;
                    }
                    warn!("Failed to refresh exchange configs: {}. Using cached.", e);
                }
            }
        }

        let exchange_configs = exchange_configs_cache.as_ref().unwrap();

        // Refresh oracle keys cache if expired or not yet fetched
        if config.update_contract_enabled {
            let should_refresh = oracle_keys_fetched_at
                .map(|t| t.elapsed() >= ORACLE_KEYS_CACHE_TTL)
                .unwrap_or(true);
            if should_refresh {
                if let Some(ref contract_id) = config.oracle_contract_id {
                    match read_asset_oracle_keys(&client, &config.near_rpc_url, contract_id).await {
                        Ok(keys) => {
                            if !keys.is_empty() {
                                info!("Loaded {} oracle key mappings from contract", keys.len());
                                for (asset, key) in &keys {
                                    debug!("  {} -> {}", asset, key);
                                }

                                // Auto-discover signer accounts if not manually configured
                                if config.oracle_signer_account.is_none() {
                                    let accounts = discover_signer_accounts(
                                        &client, &config.near_rpc_url, contract_id, &keys,
                                    ).await;
                                    if !accounts.is_empty() {
                                        info!("Discovered {} signer accounts from contract", accounts.len());
                                        for acc in &accounts {
                                            debug!("  signer: {}", acc);
                                        }
                                    }
                                    signer_accounts_cache = accounts;
                                }
                            }
                            oracle_keys_cache = if keys.is_empty() { None } else { Some(keys) };
                            oracle_keys_fetched_at = Some(Instant::now());
                        }
                        Err(e) => {
                            warn!("Failed to read oracle keys from contract: {}", e);
                            // Keep using old cache if available
                        }
                    }
                }
            }
        }

        // Check oracle signer balance periodically
        if config.update_contract_enabled {
            // Use manually configured account or auto-discovered accounts
            let accounts_to_check: Vec<&str> = if let Some(ref account) = config.oracle_signer_account {
                vec![account.as_str()]
            } else {
                signer_accounts_cache.iter().map(|s| s.as_str()).collect()
            };

            if !accounts_to_check.is_empty() {
                let should_check = balance_check_at
                    .map(|t| t.elapsed() >= BALANCE_CHECK_INTERVAL)
                    .unwrap_or(true);
                if should_check {
                    let mut any_low = false;      // at least one signer confirmed below min
                    let mut check_failed = false; // at least one balance could not be confirmed
                    for signer_account in &accounts_to_check {
                        match check_near_balance(&client, &config.near_rpc_url, signer_account).await {
                            Ok(balance) => {
                                if balance < config.oracle_min_balance_near {
                                    any_low = true;
                                    warn!(
                                        "Oracle signer {} balance {:.4} NEAR < min {:.4}",
                                        signer_account, balance, config.oracle_min_balance_near
                                    );
                                    if !balance_alert_sent {
                                        balance_alert_sent = true;
                                        telegram::send_alert(
                                            &client,
                                            config.telegram_bot_token.as_deref(),
                                            config.telegram_chat_id.as_deref(),
                                            "Oracle Balance Low",
                                            &format!(
                                                "Account: {}\nBalance: {:.4} NEAR\nMinimum: {:.4} NEAR\n\nContract updates paused. Fund the account to resume.",
                                                signer_account, balance, config.oracle_min_balance_near
                                            ),
                                        ).await;
                                    }
                                }
                            }
                            Err(e) => {
                                check_failed = true;
                                warn!("Failed to check oracle signer balance for {}: {}", signer_account, e);
                            }
                        }
                    }
                    // Fail-safe: push ONLY when every signer balance is confirmed >= min.
                    // A balance we couldn't read (RPC error) is NOT a green light — we keep
                    // updates paused and never resume on a guess. Resuming requires a positive
                    // confirmation, which is exactly the "fund the account to resume" flow.
                    let was_paused = contract_update_paused;
                    contract_update_paused = any_low || check_failed;
                    if !contract_update_paused && was_paused {
                        info!("Oracle signer balances confirmed >= min, resuming contract updates");
                        balance_alert_sent = false;
                    }
                    balance_check_at = Some(Instant::now());
                }
            }
        }

        let wasi_was_called = match poll_and_update(&client, &config, &mut last_priority_update, &mut last_full_update, &mut last_contract_push, oracle_keys_cache.as_ref(), contract_update_paused, exchange_configs, &mut alert_throttle).await {
            Ok(called) => {
                consecutive_failures = 0;
                called
            }
            Err(e) => {
                error!("Poll cycle failed: {}", e);
                consecutive_failures += 1;

                // Send Telegram alert after threshold failures
                if consecutive_failures >= ALERT_THRESHOLD {
                    alert_throttle.send(
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
                false
            }
        };

        if !wasi_was_called || consecutive_failures > 0 {
            // No update needed or error — wait before next check
            tokio::time::sleep(Duration::from_secs(config.poll_interval_secs)).await;
        }
        // If WASI was called successfully — immediately start next check cycle
    }
}

/// Read asset -> oracle key env var mapping from contract.
/// Returns HashMap<asset_id, key_env_name> (e.g., "wrap.near" -> "PROTECTED_ORACLE_KEY_A").
async fn read_asset_oracle_keys(
    client: &reqwest::Client,
    rpc_url: &str,
    contract_id: &str,
) -> Result<HashMap<String, String>> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "oracle-scheduler",
        "method": "query",
        "params": {
            "request_type": "call_function",
            "finality": "optimistic",
            "account_id": contract_id,
            "method_name": "get_asset_oracle_keys",
            "args_base64": BASE64.encode(b"{}")
        }
    });

    let resp = client
        .post(rpc_url)
        .json(&body)
        .send()
        .await
        .context("RPC request failed")?;

    let rpc_resp: serde_json::Value = resp.json().await.context("RPC response parse failed")?;

    let result_bytes = rpc_resp["result"]["result"]
        .as_array()
        .context("Missing result.result array in RPC response")?
        .iter()
        .map(|v| v.as_u64().unwrap_or(0) as u8)
        .collect::<Vec<u8>>();

    // Response is Vec<(AssetId, String)>
    let pairs: Vec<(String, String)> =
        serde_json::from_slice(&result_bytes).context("Failed to parse get_asset_oracle_keys response")?;

    Ok(pairs.into_iter().collect())
}

/// Read push signer accounts for a given asset from the oracle contract.
/// Returns Vec of account IDs (64-char hex implicit accounts).
async fn read_push_signer_accounts(
    client: &reqwest::Client,
    rpc_url: &str,
    contract_id: &str,
    asset_id: &str,
) -> Result<Vec<String>> {
    let args = serde_json::json!({ "asset_id": asset_id });
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "oracle-scheduler",
        "method": "query",
        "params": {
            "request_type": "call_function",
            "finality": "optimistic",
            "account_id": contract_id,
            "method_name": "get_push_signer_accounts",
            "args_base64": BASE64.encode(args.to_string().as_bytes())
        }
    });

    let resp = client
        .post(rpc_url)
        .json(&body)
        .send()
        .await
        .context("RPC request failed")?;

    let rpc_resp: serde_json::Value = resp.json().await.context("RPC response parse failed")?;

    let result_bytes = rpc_resp["result"]["result"]
        .as_array()
        .context("Missing result.result array in RPC response")?
        .iter()
        .map(|v| v.as_u64().unwrap_or(0) as u8)
        .collect::<Vec<u8>>();

    // Response is Option<Vec<AccountId>> — null means no push signer restriction
    let accounts: Option<Vec<String>> =
        serde_json::from_slice(&result_bytes).context("Failed to parse get_push_signer_accounts response")?;

    Ok(accounts.unwrap_or_default())
}

/// Discover all unique signer accounts from oracle key mappings.
/// Groups by unique key name, picks one asset per key, calls get_push_signer_accounts.
async fn discover_signer_accounts(
    client: &reqwest::Client,
    rpc_url: &str,
    contract_id: &str,
    oracle_keys: &HashMap<String, String>,
) -> Vec<String> {
    // Collect one asset per unique key name
    let mut key_to_asset: HashMap<&str, &str> = HashMap::new();
    for (asset_id, key_name) in oracle_keys {
        key_to_asset.entry(key_name.as_str()).or_insert(asset_id.as_str());
    }

    let mut all_accounts: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (key_name, asset_id) in &key_to_asset {
        match read_push_signer_accounts(client, rpc_url, contract_id, asset_id).await {
            Ok(accounts) => {
                for account in accounts {
                    if seen.insert(account.clone()) {
                        debug!("Discovered signer account {} (key: {})", account, key_name);
                        all_accounts.push(account);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to read push signer accounts for {} (key {}): {}", asset_id, key_name, e);
            }
        }
    }

    all_accounts
}

/// Check NEAR balance of an account via RPC. Returns balance in NEAR.
async fn check_near_balance(
    client: &reqwest::Client,
    rpc_url: &str,
    account_id: &str,
) -> Result<f64> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "oracle-scheduler",
        "method": "query",
        "params": {
            "request_type": "view_account",
            "finality": "optimistic",
            "account_id": account_id
        }
    });

    let resp = client
        .post(rpc_url)
        .json(&body)
        .send()
        .await
        .context("RPC request failed")?;

    let rpc_resp: serde_json::Value = resp.json().await.context("RPC response parse failed")?;

    if let Some(error) = rpc_resp.get("error") {
        // UNKNOWN_ACCOUNT means the implicit account hasn't been funded yet — treat as balance 0
        if error.to_string().contains("UNKNOWN_ACCOUNT") {
            return Ok(0.0);
        }
        anyhow::bail!("RPC error: {}", error);
    }

    let amount_str = rpc_resp["result"]["amount"]
        .as_str()
        .context("Missing amount in view_account response")?;

    // Convert yoctoNEAR to NEAR (1 NEAR = 10^24 yoctoNEAR)
    let yocto: u128 = amount_str.parse().unwrap_or(0);
    let near = yocto as f64 / 1e24;
    Ok(near)
}

/// Fetch exchange configs from public storage (config:assets key).
async fn fetch_exchange_configs(
    client: &reqwest::Client,
    config: &Config,
) -> Result<HashMap<String, ExchangeConfig>> {
    let url = format!("{}/public/storage/batch", config.coordinator_url);
    let body = serde_json::json!({
        "project_uuid": config.project_uuid,
        "keys": ["config:assets"],
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
    let item = batch_resp
        .results
        .get("config:assets")
        .context("config:assets not in response")?;

    if !item.exists {
        anyhow::bail!(
            "config:assets not found in public storage. Run sync_asset_configs on the contract first."
        );
    }

    let value_b64 = item
        .value
        .as_deref()
        .context("config:assets exists but value is null")?;
    let bytes = BASE64.decode(value_b64)?;
    let configs: HashMap<String, ExchangeConfig> = serde_json::from_slice(&bytes)?;
    Ok(configs)
}


/// Rough cost of querying one source, in milliseconds, measured from the production worker host.
/// Used only to balance groups against each other — relative order matters, absolute values do not.
fn source_cost_ms(source: &str) -> u32 {
    match source {
        "gate" => 570,      // by far the slowest configured source (TLS handshake alone ~0.42s)
        "binance_alpha" => 200,
        "cryptocom" => 200,
        "kucoin" => 185,
        "huobi" => 180,
        "chainlink" => 150, // varies by RPC, and may retry across several on failure
        "binance_us" => 130,
        "pyth" => 105,
        "coingecko" => 50,
        "binance" => 50,    // geo-blocked from our egress: fails fast, contributes no price
        // The six below were timed from a developer host rather than the worker, so treat
        // them as relative weights only — which is all this function is used for.
        "mexc" => 565,
        "bitget" => 390,
        "okx" => 315,
        "bitstamp" => 310,
        "coinbase" => 130,
        "kraken" => 120,
        _ => 150,
    }
}

/// Estimated fetch cost of a token = the sum of the sources its config enables.
fn token_fetch_cost_ms(cfg: &ExchangeConfig) -> u32 {
    let mut cost = 0;
    if cfg.coingecko.is_some() { cost += source_cost_ms("coingecko"); }
    if cfg.binance.is_some() { cost += source_cost_ms("binance"); }
    if cfg.binance_us.is_some() { cost += source_cost_ms("binance_us"); }
    if cfg.binance_alpha.is_some() { cost += source_cost_ms("binance_alpha"); }
    if cfg.pyth.is_some() { cost += source_cost_ms("pyth"); }
    if cfg.chainlink.is_some() { cost += source_cost_ms("chainlink"); }
    if cfg.huobi.is_some() { cost += source_cost_ms("huobi"); }
    if cfg.kucoin.is_some() { cost += source_cost_ms("kucoin"); }
    if cfg.gate.is_some() { cost += source_cost_ms("gate"); }
    if cfg.cryptocom.is_some() { cost += source_cost_ms("cryptocom"); }
    if cfg.kraken.is_some() { cost += source_cost_ms("kraken"); }
    if cfg.coinbase.is_some() { cost += source_cost_ms("coinbase"); }
    if cfg.bitstamp.is_some() { cost += source_cost_ms("bitstamp"); }
    if cfg.okx.is_some() { cost += source_cost_ms("okx"); }
    if cfg.bitget.is_some() { cost += source_cost_ms("bitget"); }
    if cfg.mexc.is_some() { cost += source_cost_ms("mexc"); }
    cost
}

/// Split tokens into groups of roughly equal fetch cost, capped at `group_size` tokens each.
///
/// Chunking by count alone produces badly skewed groups, because tokens differ enormously in how
/// much work they carry: an asset with six exchanges costs several times one priced from a single
/// feed. Since a cycle is only as fast as its slowest group, that skew is exactly what we pay for.
/// This is greedy longest-processing-time-first bin packing: heaviest token first, always into the
/// currently lightest group that still has room. Deterministic and stateless — no latency history
/// to keep, because a slow source is slow for every token that uses it.
fn balance_groups(
    tokens: &[String],
    configs: &HashMap<String, ExchangeConfig>,
    group_size: usize,
) -> Vec<Vec<String>> {
    if tokens.is_empty() {
        return Vec::new();
    }
    let group_size = group_size.max(1);
    let group_count = tokens.len().div_ceil(group_size);

    let mut items: Vec<(&String, u32)> = tokens
        .iter()
        .map(|t| {
            let cost = configs.get(t).map(token_fetch_cost_ms).unwrap_or(150);
            (t, cost)
        })
        .collect();
    // Heaviest first; tie-break on the id so the layout is stable across cycles.
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));

    let mut groups: Vec<Vec<String>> = vec![Vec::new(); group_count];
    let mut loads: Vec<u32> = vec![0; group_count];
    for (token, cost) in items {
        let target = (0..group_count)
            .filter(|&i| groups[i].len() < group_size)
            .min_by_key(|&i| loads[i])
            .unwrap_or(0);
        groups[target].push(token.clone());
        loads[target] += cost;
    }
    groups.retain(|g| !g.is_empty());
    groups
}

async fn poll_and_update(
    client: &reqwest::Client,
    config: &Config,
    last_priority_update: &mut Option<Instant>,
    last_full_update: &mut Option<Instant>,
    last_contract_push: &mut Option<Instant>,
    oracle_keys: Option<&HashMap<String, String>>,
    contract_update_paused: bool,
    exchange_configs: &HashMap<String, ExchangeConfig>,
    alert_throttle: &mut telegram::AlertThrottle,
) -> Result<bool> {
    // Reset Chainlink disabled state each cycle (scheduler is long-lived)
    oracle_ark_sources::CHAINLINK_DISABLED.store(false, std::sync::atomic::Ordering::Relaxed);

    let all_tokens: Vec<String> = exchange_configs.keys().cloned().collect();
    let api_key = env::var("API_KEY").ok();

    // 1. Fetch current prices from external sources in parallel (for comparison only!)
    let fetch_futures: Vec<_> = all_tokens
        .iter()
        .map(|token| {
            let config_for_token = exchange_configs[token].clone();
            let token = token.clone();
            let api_key = api_key.clone();
            async move {
                let result =
                    sources::fetch_price(client, &token, &config_for_token, api_key.as_deref())
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

        // Alert if we couldn't get any prices (throttled)
        alert_throttle.send(
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

        return Ok(false);
    }

    // 2. Determine scheduled batch: full update or priority-only
    let full_due = last_full_update
        .map(|t| t.elapsed().as_secs() >= config.update_interval_secs)
        .unwrap_or(true);
    let priority_due = last_priority_update
        .map(|t| t.elapsed().as_secs() >= config.update_interval_priority_secs)
        .unwrap_or(true);

    let mut tokens_to_update: Vec<String> = if full_due {
        // Full batch: all tokens that have prices
        info!("Full update due (every {}s)", config.update_interval_secs);
        current_prices.keys().cloned().collect()
    } else if priority_due {
        // Priority batch: only priority tokens
        info!("Priority update due (every {}s)", config.update_interval_priority_secs);
        config.priority_assets.iter()
            .filter(|t| current_prices.contains_key(t.as_str()))
            .cloned()
            .collect()
    } else {
        Vec::new()
    };

    // 3. Also add any tokens with significant price change (regardless of schedule)
    let tokens_to_read: Vec<&str> = current_prices.keys().map(|s| s.as_str()).collect();
    let stored_prices = read_public_storage_batch(client, config, &tokens_to_read).await?;

    for (token, current_price) in &current_prices {
        if tokens_to_update.contains(token) {
            continue;
        }
        let price_trigger = match stored_prices.get(token) {
            Some(stored) => {
                let diff = ((current_price - stored.price) / stored.price).abs() * 100.0;
                diff > config.price_diff_threshold_percent
            }
            None => true,
        };
        if price_trigger {
            info!("{}: triggering update (reason: price change, current={:.4})", token, current_price);
            tokens_to_update.push(token.clone());
        }
    }

    // 4. Phase 1 — refresh the price cache. WASI fetches its sources sequentially, so one call
    // carrying every token is the slow path; splitting it into groups that run concurrently is
    // what keeps a cycle short. Each group writes its own tokens to storage as it completes, so a
    // failing group never discards the work of the others. Never pushes on-chain (see phase 2).
    let mut cache_refreshed = false;
    if !tokens_to_update.is_empty() {
        let batch_type = if full_due { "full" } else if priority_due { "priority" } else { "price-triggered" };
        let group_size = config.group_max_tokens.max(1);
        let concurrency = config.fetch_concurrency.max(1);
        let groups = balance_groups(&tokens_to_update, exchange_configs, group_size);
        info!(
            "Refreshing {} tokens ({}) in {} group(s), {} concurrent",
            tokens_to_update.len(),
            batch_type,
            groups.len(),
            concurrency
        );

        let outcomes: Vec<(Vec<String>, Result<()>)> = stream::iter(groups.into_iter().map(|group| {
            let client = client.clone();
            let config = config.clone();
            async move {
                let res =
                    call_wasi_update(&client, &config, &group, None, contract_update_paused, false)
                        .await;
                (group, res)
            }
        }))
        .buffer_unordered(concurrency)
        .collect()
        .await;

        let group_count = outcomes.len();
        let mut failures: Vec<String> = Vec::new();
        for (group, res) in &outcomes {
            match res {
                Ok(_) => cache_refreshed = true,
                Err(e) => failures.push(format!("{:?}: {}", group, e)),
            }
        }

        if !failures.is_empty() {
            // Report the broken groups but keep what the healthy ones already committed, rather
            // than failing the whole cycle over one bad exchange.
            warn!("{}/{} refresh group(s) failed", failures.len(), group_count);
            alert_throttle
                .send(
                    client,
                    config.telegram_bot_token.as_deref(),
                    config.telegram_chat_id.as_deref(),
                    "WASI Update Failed",
                    &format!(
                        "Project: {}/{}\nFailed {}/{} group(s):\n{}",
                        config.project_owner,
                        config.project_name,
                        failures.len(),
                        group_count,
                        failures.join("\n")
                    ),
                )
                .await;
        }

        if cache_refreshed {
            let now = Instant::now();
            if full_due {
                *last_full_update = Some(now);
                *last_priority_update = Some(now);
            } else if priority_due {
                *last_priority_update = Some(now);
            }
        } else {
            anyhow::bail!("all {} refresh group(s) failed", group_count);
        }
    }

    // 5. Phase 2 — push prices on-chain. ONE call covering every token, on its own slow cadence.
    // Kept separate from the refresh above precisely so that parallelising the refresh cannot
    // multiply transactions: refreshing is cheap and frequent, pushing costs gas and stays rare.
    let mut pushed = false;
    if config.update_contract_enabled && !contract_update_paused {
        let push_due = last_contract_push
            .map(|t| t.elapsed().as_secs() >= config.contract_push_interval_secs)
            .unwrap_or(true);
        if push_due {
            let push_tokens: Vec<String> = current_prices.keys().cloned().collect();
            if !push_tokens.is_empty() {
                info!(
                    "Contract push due (every {}s): {} tokens",
                    config.contract_push_interval_secs,
                    push_tokens.len()
                );
                match call_wasi_update(
                    client,
                    config,
                    &push_tokens,
                    oracle_keys,
                    contract_update_paused,
                    true,
                )
                .await
                {
                    Ok(_) => {
                        info!("Contract push successful");
                        *last_contract_push = Some(Instant::now());
                        pushed = true;
                    }
                    Err(e) => {
                        // A failed push must not fail the cycle — the cache is already fresh and
                        // the next push cycle retries.
                        warn!("Contract push failed: {}", e);
                        alert_throttle
                            .send(
                                client,
                                config.telegram_bot_token.as_deref(),
                                config.telegram_chat_id.as_deref(),
                                "Contract Push Failed",
                                &format!(
                                    "Project: {}/{}\nTokens: {}\nError: {}",
                                    config.project_owner,
                                    config.project_name,
                                    push_tokens.len(),
                                    e
                                ),
                            )
                            .await;
                    }
                }
            }
        }
    }

    Ok(cache_refreshed || pushed)
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
    oracle_keys: Option<&HashMap<String, String>>,
    contract_update_paused: bool,
    push_to_contract: bool,
) -> Result<()> {
    let url = format!(
        "{}/call/{}/{}",
        config.coordinator_url, config.project_owner, config.project_name
    );

    // Contract push happens ONLY on the dedicated push cycle (`push_to_contract`), never on a
    // cache-refresh group — otherwise splitting a refresh into N parallel groups would fire N
    // on-chain transactions instead of one, multiplying gas. Balance fail-safe still applies:
    // when it is low we run warm-only (fetch + cache, no push).
    let effective_update_contract =
        push_to_contract && config.update_contract_enabled && !contract_update_paused;

    // Build WASI input - NOTE: we do NOT pass prices!
    // WASI will fetch its own prices from sources inside TEE
    let mut input = serde_json::json!({
        "command": "update_prices",
        "tokens": tokens,
        "update_contract": effective_update_contract,
        "contract_id": config.oracle_contract_id,
        "aggregation_method": config.aggregation_method,
        "min_sources_num": config.min_sources_num,
    });

    // Pass oracle key mapping only when actually updating contract
    if effective_update_contract {
        if let Some(keys) = oracle_keys {
            input["oracle_keys"] = serde_json::to_value(keys).unwrap_or_default();
        }
    }

    // The WASI HTTP client has no read timeout (only connect), so a source that accepts the
    // connection and then stalls can only be bounded from here. A refresh group is small
    // (GROUP_MAX_TOKENS x ~6 sources, ~5-15s in practice), so a tight limit cuts a hung group
    // loose quickly — and because groups run concurrently, the others are unaffected. The
    // contract push keeps the wider limit: it covers every token and also signs and sends
    // transactions.
    let max_execution_secs: u64 = if push_to_contract { 180 } else { 60 };

    let mut body = serde_json::json!({
        "input": input,
        "async": true,
        "resource_limits": {
            "max_execution_seconds": max_execution_secs
        }
    });

    // Include secrets only when actually updating contract
    if effective_update_contract {
        if let (Some(profile), Some(account_id)) = (&config.secrets_profile, &config.secrets_account_id) {
            body["secrets_ref"] = serde_json::json!({
                "profile": profile,
                "account_id": account_id,
            });
        } else {
            warn!("UPDATE_CONTRACT_ENABLED=true but SECRETS_PROFILE/SECRETS_ACCOUNT_ID not set. WASI won't have secrets (PROTECTED_ keys etc).");
        }
    }

    info!("Calling WASI (async): {} tokens", tokens.len());

    // 1. Submit async call — coordinator returns immediately with call_id
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

    let submit_resp: WasiCallResponse = resp.json().await?;
    let call_id = submit_resp.call_id
        .context("No call_id in async response")?;

    info!("WASI call submitted: call_id={}", call_id);

    // 2. Poll for result until completed/failed
    let poll_url = format!("{}/calls/{}", config.coordinator_url, call_id);
    let status_poll_interval = Duration::from_secs(3);
    // Allow 2x max_execution for compilation + overhead
    let deadline = Instant::now() + Duration::from_secs(max_execution_secs * 2);
    let mut last_status = String::new();

    loop {
        tokio::time::sleep(status_poll_interval).await;

        if Instant::now() > deadline {
            anyhow::bail!("WASI call {} timed out after {}s", call_id, max_execution_secs * 2);
        }

        let resp = match client
            .get(&poll_url)
            .header("X-Payment-Key", &config.payment_key)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                warn!("Poll {} request failed: {}", call_id, e);
                continue;
            }
        };

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            warn!("Poll {} returned HTTP {}: {}", call_id, status, text);
            continue;
        }

        let poll_resp: WasiCallResponse = match resp.json().await {
            Ok(r) => r,
            Err(e) => {
                warn!("Poll {} parse error: {}", call_id, e);
                continue;
            }
        };
        let status = poll_resp.status.as_deref().unwrap_or("unknown");

        match status {
            "completed" => {
                info!("WASI call {} completed", call_id);
                return Ok(());
            }
            "failed" => {
                let error = poll_resp.error.unwrap_or_else(|| "unknown error".to_string());
                anyhow::bail!("WASI call {} failed: {}", call_id, error);
            }
            _ => {
                // Log only on status change to avoid spam
                if status != last_status {
                    info!("WASI call {} status: {}", call_id, status);
                    last_status = status.to_string();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(sources: &[&str]) -> ExchangeConfig {
        let mut c = ExchangeConfig::default();
        for s in sources {
            match *s {
                "coingecko" => c.coingecko = Some("x".into()),
                "binance_us" => c.binance_us = Some("x".into()),
                "pyth" => c.pyth = Some("x".into()),
                "huobi" => c.huobi = Some("x".into()),
                "kucoin" => c.kucoin = Some("x".into()),
                "gate" => c.gate = Some("x".into()),
                "cryptocom" => c.cryptocom = Some("x".into()),
                other => panic!("unhandled source in test: {}", other),
            }
        }
        c
    }

    /// A token with six exchanges costs several times one priced from a single feed, so grouping
    /// by count alone leaves one group carrying most of the work.
    #[test]
    fn groups_are_balanced_by_cost_not_by_count() {
        let heavy = ["a", "b", "c", "d"]; // 6 sources each
        let light = ["e", "f", "g", "h"]; // 1 source each
        let mut configs = HashMap::new();
        for t in heavy {
            configs.insert(
                t.to_string(),
                cfg(&["binance_us", "huobi", "cryptocom", "kucoin", "gate", "pyth"]),
            );
        }
        for t in light {
            configs.insert(t.to_string(), cfg(&["pyth"]));
        }
        let tokens: Vec<String> = heavy.iter().chain(light.iter()).map(|s| s.to_string()).collect();

        let groups = balance_groups(&tokens, &configs, 4);
        assert_eq!(groups.len(), 2, "8 tokens / 4 per group");
        assert!(groups.iter().all(|g| g.len() <= 4), "group size cap holds");

        let loads: Vec<u32> = groups
            .iter()
            .map(|g| g.iter().map(|t| token_fetch_cost_ms(&configs[t])).sum())
            .collect();
        let (min, max) = (loads.iter().min().unwrap(), loads.iter().max().unwrap());
        // Naive chunking would put all four heavy tokens in one group: a ~6x imbalance.
        assert!(*max as f64 <= *min as f64 * 1.5, "loads roughly even: {:?}", loads);

        let total: usize = groups.iter().map(|g| g.len()).sum();
        assert_eq!(total, 8, "every token is scheduled exactly once");
    }

    #[test]
    fn every_token_is_assigned_and_layout_is_stable() {
        let mut configs = HashMap::new();
        for t in ["a", "b", "c", "d", "e"] {
            configs.insert(t.to_string(), cfg(&["pyth", "gate"]));
        }
        let tokens: Vec<String> = ["a", "b", "c", "d", "e"].iter().map(|s| s.to_string()).collect();

        let first = balance_groups(&tokens, &configs, 2);
        let second = balance_groups(&tokens, &configs, 2);
        assert_eq!(first, second, "grouping is deterministic across cycles");

        let mut seen: Vec<String> = first.iter().flatten().cloned().collect();
        seen.sort();
        assert_eq!(seen, tokens);
    }

    /// A token missing from the config map must still be scheduled, not silently dropped.
    #[test]
    fn unknown_tokens_still_get_scheduled() {
        let configs: HashMap<String, ExchangeConfig> = HashMap::new();
        let tokens = vec!["ghost".to_string()];
        let groups = balance_groups(&tokens, &configs, 4);
        assert_eq!(groups, vec![vec!["ghost".to_string()]]);
    }
}
