mod near_tx;
mod signed_prices;
mod sources;
mod storage_types;
mod telegram;
mod types;

use oracle_example_sources::parsers;
use oracle_example_sources::sources::sync as shared_sources;
use oracle_example_sources::{ExchangeConfig, SourcePrice};
use outlayer::storage;
use storage_types::{SourceInfo, StoredPrice};
use types::*;
use std::collections::HashMap;
use std::env;
use std::io::{self, Read, Write};
use std::time::{SystemTime, UNIX_EPOCH};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read input from stdin
    let mut input_string = String::new();
    io::stdin().read_to_string(&mut input_string)?;

    // Parse command
    let output = match serde_json::from_str::<OracleCommand>(&input_string) {
        Ok(command) => match command {
            OracleCommand::UpdatePrices {
                tokens,
                only_sources,
                exclude_sources,
                update_contract,
                contract_id,
                aggregation_method,
                min_sources_num,
                oracle_keys,
            } => {
                let response = handle_update_prices(
                    &tokens,
                    only_sources.as_deref(),
                    exclude_sources.as_deref(),
                    update_contract,
                    contract_id.as_deref(),
                    aggregation_method,
                    min_sources_num,
                    oracle_keys.as_ref(),
                );
                serde_json::to_string(&response)?
            }
            OracleCommand::GetPrices {
                tokens,
                max_age_secs,
                aggregation_method,
                min_sources_num,
            } => {
                let response = handle_get_prices(&tokens, max_age_secs, aggregation_method, min_sources_num);
                serde_json::to_string(&response)?
            }
            OracleCommand::GetSignedPrices {
                tokens,
                max_age_secs,
                sig_format,
                expo,
                exclude_sources,
                aggregation_method,
                min_sources_num,
            } => {
                let response = handle_get_signed_prices(
                    &tokens,
                    max_age_secs,
                    sig_format.as_deref(),
                    expo,
                    exclude_sources.as_deref(),
                    aggregation_method,
                    min_sources_num,
                );
                serde_json::to_string(&response)?
            }
            OracleCommand::ForceUpdate {
                tokens,
                aggregation_method,
                min_sources_num,
            } => {
                let response = handle_force_update(&tokens, aggregation_method, min_sources_num);
                serde_json::to_string(&response)?
            }
            OracleCommand::FetchExternal { token_id, source } => {
                let response = handle_fetch_external(&token_id, &source);
                serde_json::to_string(&response)?
            }
            OracleCommand::FetchCustomData { requests } => {
                let response = handle_fetch_custom_data(&requests);
                serde_json::to_string(&response)?
            }
            OracleCommand::TestTelegram { message } => {
                let response = handle_test_telegram(message.as_deref());
                serde_json::to_string(&response)?
            }
            OracleCommand::GetPublicKey { key_name } => {
                let response = handle_get_public_key(&key_name);
                serde_json::to_string(&response)?
            }
            OracleCommand::SyncAssetConfigs { configs } => {
                let response = handle_sync_asset_configs(&configs);
                serde_json::to_string(&response)?
            }
        },
        Err(e) => {
            let error_response = CommandResponse {
                success: false,
                prices: vec![],
                error: Some(format!("Failed to parse request: {}", e)),
            };
            serde_json::to_string(&error_response)?
        }
    };

    print!("{}", output);
    io::stdout().flush()?;
    Ok(())
}

/// Get current timestamp in seconds
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Load exchange configs from WASI public storage (key: "config:assets").
/// These configs are synced from the oracle contract via DAO proposals.
fn load_exchange_configs() -> Result<HashMap<String, ExchangeConfig>, String> {
    match storage::get_worker("config:assets") {
        Ok(Some(data)) => serde_json::from_slice(&data)
            .map_err(|e| format!("config:assets parse error: {}", e)),
        Ok(None) => Err(
            "config:assets not found in storage. Call sync_asset_configs on the contract first."
                .to_string(),
        ),
        Err(e) => Err(format!("Storage error reading config:assets: {}", e)),
    }
}

/// Handle sync_asset_configs command — stores exchange configs in public storage.
/// Contract passes raw JSON strings; WASI parses and validates them here.
fn handle_sync_asset_configs(
    configs: &HashMap<String, String>,
) -> SyncResponse {
    // Parse each config string into a JSON Value, skip malformed entries
    let mut parsed: HashMap<String, serde_json::Value> = HashMap::new();
    for (asset_id, config_str) in configs {
        match serde_json::from_str::<serde_json::Value>(config_str) {
            Ok(val) => { parsed.insert(asset_id.clone(), val); }
            Err(e) => {
                eprintln!("WARNING: skipping malformed config for {}: {}", asset_id, e);
            }
        }
    }

    let json = match serde_json::to_vec(&parsed) {
        Ok(j) => j,
        Err(e) => {
            return SyncResponse {
                success: false,
                count: 0,
                error: Some(format!("Failed to serialize configs: {}", e)),
            };
        }
    };

    match storage::set_worker_with_options("config:assets", &json, Some(false)) {
        Ok(_) => SyncResponse {
            success: true,
            count: parsed.len(),
            error: None,
        },
        Err(e) => SyncResponse {
            success: false,
            count: 0,
            error: Some(format!("Failed to store config:assets: {}", e)),
        },
    }
}

/// Handle update_prices command (triggered by scheduler)
/// Fetches prices from sources IN TEE and stores in public storage
#[allow(clippy::too_many_arguments)]
fn handle_update_prices(
    tokens: &[String],
    only_sources: Option<&[String]>,
    exclude_sources: Option<&[String]>,
    update_contract: bool,
    contract_id: Option<&str>,
    aggregation_method: AggregationMethod,
    min_sources_num: u8,
    oracle_keys: Option<&std::collections::HashMap<String, String>>,
) -> CommandResponse {
    let mut results = Vec::new();

    // Load exchange configs from public storage
    let configs = match load_exchange_configs() {
        Ok(c) => c,
        Err(e) => {
            return CommandResponse {
                success: false,
                prices: vec![],
                error: Some(e),
            };
        }
    };

    // Which venues this refresh covers. Rejecting an unknown name here rather than ignoring it
    // matters: a typo in the slow tier's list would otherwise make that tier fetch everything
    // on the fast tier's cadence, quietly multiplying cost and rate-limit pressure.
    let cleared = match signed_prices::resolve_source_selection(only_sources, exclude_sources) {
        Ok(c) => c,
        Err(e) => {
            return CommandResponse {
                success: false,
                prices: vec![],
                error: Some(e),
            };
        }
    };
    let tiered = !cleared.is_empty();

    // Get universal API key from environment (used for CoinGecko, etc.)
    let api_key = env::var("API_KEY").ok();

    // ONE request per distinct source for the whole token set — at most 16 — instead of the
    // ~66 (token, source) round-trips this used to issue. Storage still happens per token
    // below, so a failure part-way through leaves the tokens handled before it updated.
    let selected: HashMap<String, ExchangeConfig> =
        configs_for(tokens.iter().map(String::as_str), &configs)
            .into_iter()
            .map(|(token, config)| {
                let config = if tiered {
                    signed_prices::filter_exchange_config(&config, &cleared)
                } else {
                    config
                };
                (token, config)
            })
            // Only in a tiered refresh: an asset with no venue in THIS tier is skipped and
            // reported as such below. In a full refresh an asset with no venues at all is a
            // config error, and must keep falling through to the "No Price Sources" alert
            // rather than being quietly reclassified as a tier mismatch.
            .filter(|(_, config)| !tiered || !config.configured_sources().is_empty())
            .collect();

    let batched = shared_sources::fetch_all_sources_batch(&selected, api_key.as_deref());

    for token in tokens {
        let token = token.to_string();
        if !configs.contains_key(&token) {
            results.push(PriceResult {
                token,
                price: None,
                timestamp: None,
                sources: None,
                from_cache: None,
                error: Some("Token not in exchange config".to_string()),
            });
            continue;
        }

        // A tier the asset has no venue in is a caller mistake, not a silent no-op: the
        // scheduler is expected to send only the assets that participate in the tier, so
        // reaching this means its view of the configs has drifted from ours.
        if tiered && !selected.contains_key(&token) {
            results.push(PriceResult {
                token,
                price: None,
                timestamp: None,
                sources: None,
                from_cache: None,
                error: Some(format!(
                    "No configured source in the requested tier (excluded: {})",
                    cleared.join(", ")
                )),
            });
            continue;
        }

        let source_prices = batched.get(&token).map(Vec::as_slice).unwrap_or_default();
        match aggregate_and_store_price(&token, source_prices, aggregation_method, min_sources_num) {
            Ok(stored) => {
                results.push(PriceResult {
                    token: token.clone(),
                    price: Some(stored.price),
                    timestamp: Some(stored.timestamp),
                    sources: Some(stored.sources.iter().map(|s| s.name.clone()).collect()),
                    from_cache: Some(false),
                    error: None,
                });
            }
            Err(e) => {
                results.push(PriceResult {
                    token: token.clone(),
                    price: None,
                    timestamp: None,
                    sources: None,
                    from_cache: None,
                    error: Some(e),
                });
            }
        }
    }

    // Alert if Chainlink was disabled during this run (all RPCs failed)
    if oracle_example_sources::CHAINLINK_DISABLED.load(std::sync::atomic::Ordering::Relaxed) {
        telegram::send_alert(
            "Chainlink Disabled",
            "All Chainlink Ethereum RPCs failed. Chainlink source disabled for this run.\nPrices still available from other sources.",
        );
    }

    // If update_contract is true, sign and send report_prices tx to the oracle contract
    // Throttle: skip tokens reported to contract less than 20s ago
    const MIN_CONTRACT_REPORT_INTERVAL: u64 = 20;

    if update_contract {
        if let Some(contract_id) = contract_id {
            let now = current_timestamp();

            // Build AssetPrice entries from successful results, skipping recently-reported ones
            // Contract Price format: { multiplier: u128 as string, decimals: u8 }
            // We use decimals=8, so multiplier = price * 10^8
            let prices_for_contract: Vec<(String, serde_json::Value)> = results.iter()
                .filter(|r| r.price.is_some() && r.error.is_none())
                .filter(|r| {
                    // Check last_contract_report from stored price
                    let key = StoredPrice::storage_key(&r.token);
                    match storage::get_worker(&key) {
                        Ok(Some(data)) => {
                            match serde_json::from_slice::<StoredPrice>(&data) {
                                Ok(stored) => {
                                    if let Some(last_report) = stored.last_contract_report {
                                        if now.saturating_sub(last_report) < MIN_CONTRACT_REPORT_INTERVAL {
                                            eprintln!("{}: skipping contract report ({}s since last)", r.token, now - last_report);
                                            return false;
                                        }
                                    }
                                    true
                                }
                                Err(_) => true,
                            }
                        }
                        _ => true,
                    }
                })
                .map(|r| {
                    let price_f64 = r.price.unwrap();
                    let multiplier = (price_f64 * 100_000_000.0).round() as u128;
                    (r.token.clone(), serde_json::json!({
                        "asset_id": r.token,
                        "price": {
                            "multiplier": multiplier.to_string(),
                            "decimals": 8
                        }
                    }))
                })
                .collect();

            if !prices_for_contract.is_empty() {
                // Group prices by oracle key
                let mut by_key: std::collections::HashMap<String, Vec<(String, serde_json::Value)>> =
                    std::collections::HashMap::new();

                if let Some(keys) = oracle_keys {
                    // Only push assets that have explicit key assignments
                    for (asset_id, price_json) in prices_for_contract {
                        if let Some(key_name) = keys.get(&asset_id) {
                            by_key.entry(key_name.clone()).or_default().push((asset_id, price_json));
                        }
                        // Assets without key mapping are warm-only (not pushed to contract)
                    }
                } else {
                    // No oracle_keys provided — push all with default key
                    let default_key = "PROTECTED_ORACLE_KEY".to_string();
                    for entry in prices_for_contract {
                        by_key.entry(default_key.clone()).or_default().push(entry);
                    }
                }

                // Send one transaction per key
                for (key_name, entries) in &by_key {
                    let price_jsons: Vec<&serde_json::Value> = entries.iter().map(|(_, j)| j).collect();
                    let args = serde_json::json!({ "prices": price_jsons });
                    match report_prices_to_contract(contract_id, key_name, &args.to_string()) {
                        Ok(hash) => {
                            eprintln!("Contract updated via {}: {}", key_name, hash);
                            // Mark reported tokens with last_contract_report timestamp
                            for (asset_id, _) in entries {
                                let key = StoredPrice::storage_key(asset_id);
                                if let Ok(Some(data)) = storage::get_worker(&key) {
                                    if let Ok(mut stored) = serde_json::from_slice::<StoredPrice>(&data) {
                                        stored.last_contract_report = Some(now);
                                        if let Ok(json) = serde_json::to_vec(&stored) {
                                            let _ = storage::set_worker_with_options(&key, &json, Some(false));
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            let signer_info = env::var(key_name)
                                .ok()
                                .and_then(|k| near_tx::derive_implicit_account(&k).ok())
                                .map(|(id, _)| id)
                                .unwrap_or_else(|| "unknown".to_string());
                            eprintln!("Contract update failed ({}): {}", key_name, e);
                            telegram::send_alert(
                                "Contract Update Failed",
                                &format!(
                                    "Contract: {}\nKey: {}\nSigner: {}\nAssets: {:?}\nError: {}\n\nIf 'account not found': fund the implicit account with ≥0.01 NEAR",
                                    contract_id, key_name, signer_info,
                                    entries.iter().filter_map(|(_, p)| p["asset_id"].as_str()).collect::<Vec<_>>(),
                                    e
                                ),
                            );
                        }
                    }
                }
            }
        } else {
            eprintln!("update_contract=true but no contract_id provided");
        }
    }

    let success = results.iter().all(|r| r.error.is_none());
    CommandResponse {
        success,
        prices: results,
        error: None,
    }
}

/// Gate every request-supplied environment variable name behind the `PROTECTED_` prefix.
///
/// `get_public_key` and the per-asset `oracle_keys` of `update_prices` let the caller name the
/// variable to read. The prefix is what keeps that choice inside the set of keys the enclave
/// generated for signing, instead of any secret the worker happens to hold — the oracle
/// contract enforces the same rule on `push_signer_key`.
///
/// `get_signed_prices` is NOT in that list any more: a prefix check still let the caller pick
/// WHICH signing key signed, including the on-chain push key, so its key is pinned in code as
/// `signed_prices::FEED_SIGNING_KEY` and is not a request field at all. The prefix is a floor,
/// not a substitute for deciding who signs.
fn require_protected_key_name(key_name: &str) -> Result<(), String> {
    if !key_name.starts_with("PROTECTED_") {
        return Err("key_name must start with PROTECTED_".to_string());
    }
    Ok(())
}

/// Sign and send report_prices transaction to the oracle contract
/// Uses a PROTECTED_ key (TEE-generated) and derives implicit account from it
fn report_prices_to_contract(contract_id: &str, key_name: &str, args: &str) -> Result<String, String> {
    require_protected_key_name(key_name)?;

    let signer_key = env::var(key_name)
        .map_err(|_| format!("{} not set", key_name))?;
    let rpc_url = env::var("NEAR_RPC_URL")
        .unwrap_or_else(|_| "https://rpc.mainnet.near.org".to_string());

    // Derive implicit account ID from the protected key. The parse error is deliberately
    // dropped: bs58 reports the offending character and its index, so a caller who can pick
    // the key name and read the error (it is logged and sent to Telegram) can walk the
    // secret out one position at a time.
    let (signer_id, _) = near_tx::derive_implicit_account(&signer_key)
        .map_err(|_| format!("{} does not hold a valid ed25519 key", key_name))?;

    near_tx::call(
        &rpc_url,
        &signer_id,
        &signer_key,
        contract_id,
        "report_prices",
        args,
        100_000_000_000_000, // 100 TGas
        0,                    // no deposit
    )
    .map_err(|e| e.to_string())
}

/// Number of cache misses from which get_prices refetches in one batch.
///
/// A batch costs one request per source no matter how many tokens are in it, so it pays off
/// from the second miss on. For a single miss the per-token path issues the same number of
/// requests without downloading the 129-536 KB all-ticker responses, so it stays cheaper.
const BATCH_FETCH_THRESHOLD: usize = 2;

/// How a token's cache entry resolved, before any fetch
enum CacheState {
    /// Fresh cached price, or a terminal error (unknown token, storage failure) — no fetch
    Resolved(PriceResult),
    /// Needs a fresh price; `fallback` is what to return if that fetch fails
    Miss {
        token: String,
        fallback: CacheFallback,
    },
}

/// What get_prices falls back to when the refetch of a cache miss fails
enum CacheFallback {
    /// Nothing was cached
    Empty,
    /// Stale but readable — served with a warning rather than dropped
    Stale(StoredPrice),
    /// The cached entry could not be parsed; carries the parse error
    Corrupt(String),
}

/// A price rebuilt from a stored record for one caller's freshness window.
struct WindowedPrice {
    price: f64,
    /// Oldest observation that contributed. This is what the caller may rely on: an aggregate is
    /// no fresher than its stalest input, so this — not the record's write time — is the number
    /// that goes out as `publish_time` and gets signed.
    timestamp: u64,
    sources: Vec<String>,
}

/// Rebuild a price from the stored per-source breakdown, for a given window and exclusion list.
///
/// This is what makes "give me prices no older than 40 seconds" mean something precise: the
/// answer is aggregated over exactly the venues observed within 40 seconds, and a slow tier that
/// last reported two minutes ago simply does not vote. Nothing is fetched here.
///
/// `None` means too few sources survived the filter. Callers turn that into a refetch rather
/// than an error — the record is a cache, not the last word.
fn windowed_price(
    stored: &StoredPrice,
    now: u64,
    max_age_secs: u64,
    exclude: &[String],
    aggregation_method: AggregationMethod,
    min_sources_num: u8,
) -> Option<WindowedPrice> {
    let kept: Vec<&SourceInfo> = stored
        .sources_within(now, max_age_secs)
        .into_iter()
        .filter(|s| !signed_prices::is_excluded(&s.name, exclude))
        .collect();

    if kept.is_empty() || kept.len() < min_sources_num as usize {
        return None;
    }

    let prices: Vec<f64> = kept.iter().map(|s| s.price).collect();
    // A kept set that aggregates to nothing usable is a failure for this asset, never a zero
    // price: fall through to a refetch instead of serving it.
    let price = signed_prices::aggregate(&prices, aggregation_method)?;
    let timestamp = stored.oldest_timestamp(&kept)?;

    Some(WindowedPrice {
        price,
        timestamp,
        sources: kept.iter().map(|s| s.name.clone()).collect(),
    })
}

/// Handle get_prices command (blockchain requests)
/// Returns cached prices if fresh, otherwise fetches new ones
fn handle_get_prices(
    tokens: &[String],
    max_age_secs: u64,
    aggregation_method: AggregationMethod,
    min_sources_num: u8,
) -> CommandResponse {
    let now = current_timestamp();

    // Load exchange configs from public storage
    let configs = match load_exchange_configs() {
        Ok(c) => c,
        Err(e) => {
            return CommandResponse {
                success: false,
                prices: vec![],
                error: Some(e),
            };
        }
    };

    // Get universal API key for potential fresh fetches
    let api_key = env::var("API_KEY").ok();

    // Pass 1: resolve every token against the cache, collecting the ones that need a fetch
    let mut states: Vec<CacheState> = Vec::with_capacity(tokens.len());
    for token_ref in tokens {
        let token = token_ref.to_string();
        if !configs.contains_key(&token) {
            states.push(CacheState::Resolved(PriceResult {
                token,
                price: None,
                timestamp: None,
                sources: None,
                from_cache: None,
                error: Some("Token not in exchange config".to_string()),
            }));
            continue;
        }

        // Try to read from public storage
        let key = StoredPrice::storage_key(&token);
        match storage::get_worker(&key) {
            Ok(Some(data)) => match serde_json::from_slice::<StoredPrice>(&data) {
                Ok(stored) => match windowed_price(
                    &stored,
                    now,
                    max_age_secs,
                    &[],
                    aggregation_method,
                    min_sources_num,
                ) {
                    // Enough sources inside the caller's window — serve without fetching.
                    // Re-aggregating rather than returning `stored.price` also means the
                    // requested aggregation_method is honoured on a cache hit, which serving
                    // the stored scalar silently ignored.
                    Some(view) => states.push(CacheState::Resolved(PriceResult {
                        token,
                        price: Some(view.price),
                        timestamp: Some(view.timestamp),
                        sources: Some(view.sources),
                        from_cache: Some(true),
                        error: None,
                    })),
                    // Too few sources are recent enough for this caller — fetch fresh
                    None => states.push(CacheState::Miss {
                        token,
                        fallback: CacheFallback::Stale(stored),
                    }),
                },
                // Corrupted cache, fetch fresh
                Err(e) => states.push(CacheState::Miss {
                    token,
                    fallback: CacheFallback::Corrupt(e.to_string()),
                }),
            },
            // No cached price, fetch fresh
            Ok(None) => states.push(CacheState::Miss {
                token,
                fallback: CacheFallback::Empty,
            }),
            Err(e) => states.push(CacheState::Resolved(PriceResult {
                token,
                price: None,
                timestamp: None,
                sources: None,
                from_cache: None,
                error: Some(format!("Storage error: {}", e)),
            })),
        }
    }

    // Pass 2: refetch the misses, batched once several of them need the same sources
    let misses: Vec<&str> = states
        .iter()
        .filter_map(|state| match state {
            CacheState::Miss { token, .. } => Some(token.as_str()),
            CacheState::Resolved(_) => None,
        })
        .collect();

    let batched = if misses.len() >= BATCH_FETCH_THRESHOLD {
        Some(shared_sources::fetch_all_sources_batch(
            &configs_for(misses.into_iter(), &configs),
            api_key.as_deref(),
        ))
    } else {
        None
    };

    let mut results = Vec::with_capacity(states.len());
    for state in states {
        let (token, fallback) = match state {
            CacheState::Resolved(result) => {
                results.push(result);
                continue;
            }
            CacheState::Miss { token, fallback } => (token, fallback),
        };

        let fetched = match &batched {
            Some(batched) => {
                let source_prices = batched.get(&token).map(Vec::as_slice).unwrap_or_default();
                aggregate_and_store_price(&token, source_prices, aggregation_method, min_sources_num)
            }
            // Pass 1 already established that the token is configured
            None => match configs.get(&token) {
                Some(config) => fetch_and_store_price(
                    &token,
                    config,
                    api_key.as_deref(),
                    aggregation_method,
                    min_sources_num,
                ),
                None => Err("Token not in exchange config".to_string()),
            },
        };

        match fetched {
            Ok(new_stored) => results.push(PriceResult {
                token,
                price: Some(new_stored.price),
                timestamp: Some(new_stored.timestamp),
                sources: Some(new_stored.sources.iter().map(|s| s.name.clone()).collect()),
                from_cache: Some(false),
                error: None,
            }),
            Err(e) => results.push(match fallback {
                CacheFallback::Empty => PriceResult {
                    token,
                    price: None,
                    timestamp: None,
                    sources: None,
                    from_cache: None,
                    error: Some(format!("No cache and fetch failed: {}", e)),
                },
                // Return stale price with warning
                CacheFallback::Stale(stored) => {
                    // Report the honest age: the OLDEST source behind this record, not the time
                    // the record was written. Under tiered refresh those differ by minutes — a
                    // record touched 5s ago by the fast tier still carries Pyth and Chainlink
                    // readings from far earlier. Reporting the write time here would let a stale
                    // fallback slip through a caller's `max_age_secs` gate and be signed as
                    // something far fresher than it is. Erring old can only cause a rejection.
                    let all: Vec<&SourceInfo> = stored.sources.iter().collect();
                    let observed = stored.oldest_timestamp(&all).unwrap_or(stored.timestamp);
                    PriceResult {
                        token,
                        price: Some(stored.price),
                        timestamp: Some(observed),
                        sources: Some(stored.sources.iter().map(|s| s.name.clone()).collect()),
                        from_cache: Some(true),
                        error: Some(format!("Using stale cache, fetch failed: {}", e)),
                    }
                }
                CacheFallback::Corrupt(parse_error) => PriceResult {
                    token,
                    price: None,
                    timestamp: None,
                    sources: None,
                    from_cache: None,
                    error: Some(format!(
                        "Cache parse error: {}, fetch error: {}",
                        parse_error, e
                    )),
                },
            }),
        }
    }

    let success = results.iter().all(|r| r.price.is_some());
    CommandResponse {
        success,
        prices: results,
        error: None,
    }
}

/// Build a failed get_signed_prices response
fn signed_prices_error(sig_format: &str, error: String) -> SignedPricesResponse {
    SignedPricesResponse {
        success: false,
        payload: None,
        signature: None,
        public_key: None,
        sig_format: sig_format.to_string(),
        error: Some(error),
    }
}

/// Handle get_signed_prices command - signed pull feed for external consumers
///
/// Prices come from the same cache-or-fetch path as get_prices, then get scaled to i64,
/// keyed by our canonical asset_id and signed in-enclave. The caller receives the exact
/// bytes we signed, so it can verify off-chain now and on-chain later without
/// re-serializing anything.
///
/// The signing key is `signed_prices::FEED_SIGNING_KEY` and nothing in the request can point
/// at another one — see that constant for why the caller does not get to choose the signer.
#[allow(clippy::too_many_arguments)]
fn handle_get_signed_prices(
    tokens: &[String],
    max_age_secs: u64,
    sig_format: Option<&str>,
    expo: Option<i32>,
    exclude_sources: Option<&[String]>,
    aggregation_method: AggregationMethod,
    min_sources_num: u8,
) -> SignedPricesResponse {
    // Resolve the payload format first — errors echo whatever the caller asked for
    let format = match signed_prices::SigFormat::parse(sig_format) {
        Ok(f) => f,
        Err(e) => return signed_prices_error(sig_format.unwrap_or("json"), e),
    };
    let format_str = format.as_str();

    let key_name = signed_prices::FEED_SIGNING_KEY;

    if tokens.is_empty() {
        return signed_prices_error(format_str, "tokens must not be empty".to_string());
    }

    let expo = expo.unwrap_or(signed_prices::DEFAULT_EXPO);
    if let Err(e) = signed_prices::validate_expo(expo) {
        return signed_prices_error(format_str, e);
    }

    // Validate the exclusion list before spending any HTTP call: a typo like "Pyht" must
    // not silently leave the source contributing to a price the caller thinks is clean
    let exclude = match signed_prices::validate_exclusions(exclude_sources.unwrap_or(&[])) {
        Ok(e) => e,
        Err(e) => return signed_prices_error(format_str, e),
    };

    // Resolve the signing key up front so a misconfigured key fails before any fetch
    let private_key = match env::var(key_name) {
        Ok(k) => k,
        Err(_) => {
            return signed_prices_error(
                format_str,
                format!("{} not found in environment", key_name),
            )
        }
    };
    // The parse error itself never reaches the caller: it quotes the first character it
    // could not decode and its index, which over repeated calls reconstructs the secret
    let public_key = match near_tx::derive_implicit_account(&private_key) {
        Ok((_, public_key)) => public_key,
        Err(_) => {
            return signed_prices_error(
                format_str,
                format!("{} does not hold a valid ed25519 key", key_name),
            )
        }
    };

    // Without exclusions this is exactly get_prices (cache-or-fetch + cache write);
    // with exclusions the cache cannot be served as-is, see collect_prices_excluding_sources
    let results = if exclude.is_empty() {
        let response = handle_get_prices(tokens, max_age_secs, aggregation_method, min_sources_num);
        if response.prices.is_empty() {
            return signed_prices_error(
                format_str,
                response
                    .error
                    .unwrap_or_else(|| "No prices returned".to_string()),
            );
        }
        response.prices
    } else {
        match collect_prices_excluding_sources(
            tokens,
            max_age_secs,
            aggregation_method,
            min_sources_num,
            &exclude,
        ) {
            Ok(r) => r,
            Err(e) => return signed_prices_error(format_str, e),
        }
    };

    // A partial signed payload would be dangerous for a lending protocol: fail the whole
    // request when an asset has no price, or is older than the caller asked for
    let now = current_timestamp();
    let mut priced: Vec<(String, f64, u64)> = Vec::with_capacity(results.len());
    let mut failed: Vec<String> = Vec::new();

    for result in &results {
        match (result.price, result.timestamp) {
            (Some(price), Some(timestamp)) if now.saturating_sub(timestamp) <= max_age_secs => {
                priced.push((result.token.clone(), price, timestamp));
            }
            (Some(_), Some(timestamp)) => failed.push(format!(
                "{}: price is {}s old, older than max_age_secs={}{}",
                result.token,
                now.saturating_sub(timestamp),
                max_age_secs,
                result
                    .error
                    .as_ref()
                    .map(|e| format!(" ({})", e))
                    .unwrap_or_default()
            )),
            _ => failed.push(format!(
                "{}: {}",
                result.token,
                result
                    .error
                    .clone()
                    .unwrap_or_else(|| "no price available".to_string())
            )),
        }
    }

    if !failed.is_empty() {
        return signed_prices_error(
            format_str,
            format!("Refusing to sign a partial feed: {}", failed.join("; ")),
        );
    }

    let entries = match signed_prices::build_entries(&priced, expo) {
        Ok(e) => e,
        Err(e) => return signed_prices_error(format_str, e),
    };

    let (payload, message) = match signed_prices::encode_payload(&entries, format) {
        Ok(p) => p,
        Err(e) => return signed_prices_error(format_str, e),
    };

    let signature = match near_tx::sign_message(&private_key, &message) {
        Ok(s) => s,
        Err(e) => return signed_prices_error(format_str, format!("Signing failed: {}", e)),
    };

    SignedPricesResponse {
        success: true,
        payload: Some(payload),
        signature: Some(signed_prices::encode_signature(&signature)),
        public_key: Some(public_key),
        sig_format: format_str.to_string(),
        error: None,
    }
}

/// Collect prices for a request that excludes some sources
///
/// The cached "price:{token}" entry is aggregated over ALL configured sources, so it must
/// not be served as-is here. StoredPrice.sources keeps the per-source breakdown captured at
/// StoredPrice.timestamp, so while that entry is fresh we re-aggregate the breakdown over
/// the allowed sources only — publish_time then stays the real moment the TEE observed
/// exactly those sources. When the cache is stale (or too few allowed sources remain in it)
/// we fetch fresh with the excluded sources stripped out of the ExchangeConfig.
///
/// Filtered results are deliberately NOT written back to "price:{token}": that key is the
/// canonical all-source price used by get_prices/update_prices and by the on-chain report,
/// and overwriting it with a subset aggregate would silently degrade every other consumer.
/// For the same reason this path sends no Telegram alerts — it is a per-caller view, not
/// the canonical feed; failures are returned to the caller instead.
fn collect_prices_excluding_sources(
    tokens: &[String],
    max_age_secs: u64,
    aggregation_method: AggregationMethod,
    min_sources_num: u8,
    exclude: &[String],
) -> Result<Vec<PriceResult>, String> {
    let configs = load_exchange_configs()?;
    let api_key = env::var("API_KEY").ok();
    let now = current_timestamp();
    let mut results = Vec::new();

    for token_ref in tokens {
        let token = token_ref.to_string();
        let config = match configs.get(&token) {
            Some(c) => c,
            None => {
                results.push(PriceResult {
                    token,
                    price: None,
                    timestamp: None,
                    sources: None,
                    from_cache: None,
                    error: Some("Token not in exchange config".to_string()),
                });
                continue;
            }
        };

        // Re-aggregate the cached per-source breakdown while it is still fresh
        let cached = match storage::get_worker(&StoredPrice::storage_key(&token)) {
            Ok(Some(data)) => serde_json::from_slice::<StoredPrice>(&data).ok(),
            Ok(None) => None,
            Err(e) => {
                results.push(PriceResult {
                    token,
                    price: None,
                    timestamp: None,
                    sources: None,
                    from_cache: None,
                    error: Some(format!("Storage error: {}", e)),
                });
                continue;
            }
        };

        // Same windowed rebuild as the plain path, with the caller's exclusions applied on top
        if let Some(stored) = cached {
            if let Some(view) = windowed_price(
                &stored,
                now,
                max_age_secs,
                exclude,
                aggregation_method,
                min_sources_num,
            ) {
                results.push(PriceResult {
                    token,
                    price: Some(view.price),
                    timestamp: Some(view.timestamp),
                    sources: Some(view.sources),
                    from_cache: Some(true),
                    error: None,
                });
                continue;
            }
        }

        // Fetch fresh from the allowed sources only
        let filtered = signed_prices::filter_exchange_config(config, exclude);
        if signed_prices::configured_sources(&filtered).is_empty() {
            results.push(PriceResult {
                token,
                price: None,
                timestamp: None,
                sources: None,
                from_cache: None,
                error: Some(format!(
                    "every configured source is excluded (excluded: {})",
                    exclude.join(", ")
                )),
            });
            continue;
        }

        let source_prices = shared_sources::fetch_all_sources(&filtered, api_key.as_deref());

        if source_prices.is_empty() {
            results.push(PriceResult {
                token,
                price: None,
                timestamp: None,
                sources: None,
                from_cache: None,
                error: Some(format!(
                    "No sources available (excluded: {})",
                    exclude.join(", ")
                )),
            });
            continue;
        }

        if source_prices.len() < min_sources_num as usize {
            results.push(PriceResult {
                token,
                price: None,
                timestamp: None,
                sources: None,
                from_cache: None,
                error: Some(format!(
                    "Not enough sources: got {}, required {} (excluded: {})",
                    source_prices.len(),
                    min_sources_num,
                    exclude.join(", ")
                )),
            });
            continue;
        }

        let prices: Vec<f64> = source_prices.iter().map(|p| p.price).collect();
        results.push(match signed_prices::aggregate(&prices, aggregation_method) {
            Some(price) => PriceResult {
                token,
                price: Some(price),
                timestamp: Some(current_timestamp()),
                sources: Some(source_prices.iter().map(|p| p.source_name.clone()).collect()),
                from_cache: Some(false),
                error: None,
            },
            // Same rule as the cached branch: no usable number is an error for this asset.
            // handle_get_signed_prices refuses to sign a partial feed, so this asset failing
            // fails the request rather than shipping a zero to a lending market.
            None => PriceResult {
                token,
                price: None,
                timestamp: None,
                sources: None,
                from_cache: None,
                error: Some(format!(
                    "No usable price: {} source(s) answered, none finite",
                    source_prices.len()
                )),
            },
        });
    }

    Ok(results)
}

/// Handle force_update command - anyone can call if they pay
/// Always fetches fresh prices, ignoring cache
fn handle_force_update(
    tokens: &[String],
    aggregation_method: AggregationMethod,
    min_sources_num: u8,
) -> CommandResponse {
    let mut results = Vec::new();

    // Load exchange configs from public storage
    let configs = match load_exchange_configs() {
        Ok(c) => c,
        Err(e) => {
            return CommandResponse {
                success: false,
                prices: vec![],
                error: Some(e),
            };
        }
    };

    // Get universal API key from environment
    let api_key = env::var("API_KEY").ok();

    // Same batched fetch as update_prices — force_update only differs in ignoring the cache
    let batched = shared_sources::fetch_all_sources_batch(
        &configs_for(tokens.iter().map(String::as_str), &configs),
        api_key.as_deref(),
    );

    for token_ref in tokens {
        let token = token_ref.to_string();
        if !configs.contains_key(&token) {
            results.push(PriceResult {
                token,
                price: None,
                timestamp: None,
                sources: None,
                from_cache: None,
                error: Some("Token not in exchange config".to_string()),
            });
            continue;
        }

        let source_prices = batched.get(&token).map(Vec::as_slice).unwrap_or_default();
        match aggregate_and_store_price(&token, source_prices, aggregation_method, min_sources_num) {
            Ok(stored) => {
                results.push(PriceResult {
                    token,
                    price: Some(stored.price),
                    timestamp: Some(stored.timestamp),
                    sources: Some(stored.sources.iter().map(|s| s.name.clone()).collect()),
                    from_cache: Some(false),
                    error: None,
                });
            }
            Err(e) => {
                results.push(PriceResult {
                    token,
                    price: None,
                    timestamp: None,
                    sources: None,
                    from_cache: None,
                    error: Some(e),
                });
            }
        }
    }

    let success = results.iter().all(|r| r.error.is_none());
    CommandResponse {
        success,
        prices: results,
        error: None,
    }
}

/// Handle fetch_external command - fetch price from single external source
/// Does NOT store in public storage, returns directly
///
/// API_KEY secret (if configured in OutLayer) is used for authentication:
/// - CoinGecko: passed as x_cg_pro_api_key query param
/// - Custom: added as Authorization: Bearer header, but only for the hosts in
///   `oracle_example_sources::security::API_KEY_HOSTS` — the URL is caller-supplied
fn handle_fetch_external(token_id: &str, source: &ExternalPriceSource) -> ExternalPriceResponse {
    // Universal API_KEY - works for all sources that need authentication
    let api_key = env::var("API_KEY").ok();
    let timestamp = current_timestamp();

    let result: Result<f64, String> = match source {
        ExternalPriceSource::CoinGecko => {
            shared_sources::fetch_coingecko(token_id, api_key.as_deref())
                .map(|p| p.price)
                .map_err(|e| e.to_string())
        }
        ExternalPriceSource::Binance => shared_sources::fetch_binance(token_id)
            .map(|p| p.price)
            .map_err(|e| e.to_string()),
        ExternalPriceSource::Pyth => shared_sources::fetch_pyth(token_id)
            .map(|p| p.price)
            .map_err(|e| e.to_string()),
        ExternalPriceSource::Custom(config) => {
            sources::fetch_custom(config).map_err(|e| e.to_string())
        }
    };

    match result {
        Ok(price) => ExternalPriceResponse {
            success: true,
            token_id: token_id.to_string(),
            source: source.to_string(),
            price_usd: Some(price),
            timestamp: Some(timestamp),
            error: None,
            warning: Some(
                "Price from single external API, not verified by oracle consensus".to_string(),
            ),
        },
        Err(e) => {
            // Send Telegram alert for API errors
            telegram::send_alert(
                "Oracle API Error",
                &format!("Source: {}\nToken: {}\nError: {}", source, token_id, e),
            );

            ExternalPriceResponse {
                success: false,
                token_id: token_id.to_string(),
                source: source.to_string(),
                price_usd: None,
                timestamp: Some(timestamp),
                error: Some(e),
                warning: None,
            }
        }
    }
}

/// Fetch price from multiple sources and store in public storage
/// Uses shared oracle-example-sources crate for consistency with scheduler
fn fetch_and_store_price(
    token: &str,
    config: &ExchangeConfig,
    api_key: Option<&str>,
    aggregation_method: AggregationMethod,
    min_sources_num: u8,
) -> Result<StoredPrice, String> {
    // Use shared crate's fetch_all_sources with exchange config
    let source_prices = shared_sources::fetch_all_sources(config, api_key);

    aggregate_and_store_price(token, &source_prices, aggregation_method, min_sources_num)
}

/// Collect the exchange configs of the requested tokens, for one batched fetch.
/// Tokens missing from the config are skipped here and reported per token by the caller.
fn configs_for<'a, I>(tokens: I, configs: &HashMap<String, ExchangeConfig>) -> HashMap<String, ExchangeConfig>
where
    I: IntoIterator<Item = &'a str>,
{
    tokens
        .into_iter()
        .filter_map(|token| configs.get(token).map(|config| (token.to_string(), config.clone())))
        .collect()
}

/// Aggregate one token's per-source prices, alert on anomalies and write the cache entry.
///
/// Split out of `fetch_and_store_price` so the batched path reuses the exact same minimum
/// source count, deviation alerting, aggregation and storage behaviour — only the fetch
/// differs between the two.
///
/// `source_prices` is whatever *this* refresh observed, which under tiered refresh is usually a
/// subset of the configured venues. It is merged into the previous record rather than replacing
/// it, so a fast tier that skips Pyth and Chainlink does not erase them. The stored headline
/// price is then aggregated over the merged sources inside `CANONICAL_WINDOW_SECS` — never over
/// the raw merge, or a venue that died ten minutes ago would keep voting.
fn aggregate_and_store_price(
    token: &str,
    source_prices: &[SourcePrice],
    aggregation_method: AggregationMethod,
    min_sources_num: u8,
) -> Result<StoredPrice, String> {
    if source_prices.is_empty() {
        // Alert: no sources returned price
        telegram::send_alert(
            "No Price Sources",
            &format!("Token: {}\nAll configured sources failed to return a price", token),
        );
        return Err(format!("No sources available for token: {}", token));
    }

    let timestamp = current_timestamp();

    let fresh: Vec<SourceInfo> = source_prices
        .iter()
        .map(|p| SourceInfo {
            name: p.source_name.clone(),
            price: p.price,
            timestamp: Some(p.timestamp),
        })
        .collect();

    // Carry over the venues this refresh did not cover. A storage read failure is not fatal:
    // the fresh observations alone are still a valid, if narrower, record.
    let key = StoredPrice::storage_key(token);
    let previous = match storage::get_worker(&key) {
        Ok(Some(data)) => serde_json::from_slice::<StoredPrice>(&data).ok(),
        _ => None,
    };
    let (previous_sources, previous_timestamp) = match &previous {
        Some(stored) => (stored.sources.as_slice(), stored.timestamp),
        None => (&[][..], timestamp),
    };
    let sources = storage_types::merge_source_entries(
        previous_sources,
        previous_timestamp,
        &fresh,
        timestamp,
        SOURCE_RETENTION_SECS,
    );

    // The headline price commits to one window; anything outside it is retained for callers
    // asking for a wider one, but does not contribute here.
    let in_window: Vec<&SourceInfo> = sources
        .iter()
        .filter(|s| {
            timestamp.saturating_sub(s.effective_timestamp(timestamp)) <= CANONICAL_WINDOW_SECS
        })
        .collect();

    // Check minimum sources requirement
    if in_window.len() < min_sources_num as usize {
        return Err(format!(
            "Not enough sources: got {}, required {}",
            in_window.len(),
            min_sources_num
        ));
    }

    // Get all prices
    let prices: Vec<f64> = in_window.iter().map(|s| s.price).collect();

    // Check price deviation between sources — over the set that actually forms the price,
    // so a retained but out-of-window entry cannot raise (or mask) the alert
    let deviation = parsers::price_deviation(&prices);
    if deviation > PRICE_DEVIATION_ALERT_THRESHOLD {
        // Alert: high price deviation
        let source_details: Vec<String> = in_window
            .iter()
            .map(|s| {
                format!(
                    "  {}: ${:.4} ({}s ago)",
                    s.name,
                    s.price,
                    timestamp.saturating_sub(s.effective_timestamp(timestamp))
                )
            })
            .collect();
        telegram::send_alert(
            "High Price Deviation",
            &format!(
                "Token: {}\nDeviation: {:.2}%\nSources:\n{}",
                token,
                deviation,
                source_details.join("\n")
            ),
        );
    }

    // Calculate aggregated price based on method.
    //
    // `None` means every source that answered carried a non-finite number, so there is no
    // price to store. It is reported as a per-token failure and alerted on: this can only
    // happen if a parser let a value through that `check_price` should have rejected, which
    // is an oracle defect, not a market condition. The aggregation used to return 0.0 here,
    // which would have been written to the cache and reported on-chain as a real $0.00 quote.
    let aggregated_price = match signed_prices::aggregate(&prices, aggregation_method) {
        Some(price) => price,
        None => {
            telegram::send_alert(
                "No Usable Price",
                &format!(
                    "Token: {}\n{} source(s) answered but none returned a finite price",
                    token,
                    prices.len()
                ),
            );
            return Err(format!(
                "No usable price for {}: {} source(s) answered, none finite",
                token,
                prices.len()
            ));
        }
    };

    // `in_window` borrows `sources`, which is moved into the record below
    drop(in_window);
    let mut stored =
        StoredPrice::new(aggregated_price, timestamp, sources, aggregation_method.as_str());

    // Carry the on-chain report marker across refreshes. It lives in the same record but is
    // written by the push path, so rebuilding the record from scratch used to reset it to None
    // and silently defeat the per-asset push throttle — harmless while refreshes were rarer
    // than the throttle window, fatal once the fast tier runs every few seconds.
    stored.last_contract_report = previous.as_ref().and_then(|p| p.last_contract_report);

    // Store in public storage
    let json =
        serde_json::to_vec(&stored).map_err(|e| format!("Failed to serialize: {}", e))?;

    storage::set_worker_with_options(&key, &json, Some(false))
        .map_err(|e| format!("Failed to store: {}", e))?;

    Ok(stored)
}

/// Handle fetch_custom_data command - fetch multiple items from external sources
/// Used by custom_call for any data: prices, weather, game data, etc.
fn handle_fetch_custom_data(requests: &[types::CustomDataRequest]) -> types::CustomDataResponse {
    use types::{CustomDataResponse, CustomDataResult, ExternalPriceSource};

    let api_key = env::var("API_KEY").ok();
    let timestamp = current_timestamp();

    let results: Vec<CustomDataResult> = requests
        .iter()
        .map(|req| {
            // Fetch value as serde_json::Value
            let result: Result<serde_json::Value, String> = match &req.source {
                ExternalPriceSource::CoinGecko => {
                    shared_sources::fetch_coingecko(&req.token_id, api_key.as_deref())
                        .map(|p| serde_json::json!(p.price))
                        .map_err(|e| e.to_string())
                }
                ExternalPriceSource::Binance => {
                    shared_sources::fetch_binance(&req.token_id)
                        .map(|p| serde_json::json!(p.price))
                        .map_err(|e| e.to_string())
                }
                ExternalPriceSource::Pyth => {
                    shared_sources::fetch_pyth(&req.token_id)
                        .map(|p| serde_json::json!(p.price))
                        .map_err(|e| e.to_string())
                }
                ExternalPriceSource::Custom(config) => {
                    // fetch_custom_value returns serde_json::Value based on value_type
                    sources::fetch_custom_value(config).map_err(|e| e.to_string())
                }
            };

            match result {
                Ok(value) => CustomDataResult {
                    id: req.id.clone(),
                    value: Some(value),
                    timestamp: Some(timestamp),
                    error: None,
                },
                Err(e) => {
                    telegram::send_alert(
                        "Custom Data Fetch Error",
                        &format!("ID: {}\nToken: {}\nError: {}", req.id, req.token_id, e),
                    );

                    CustomDataResult {
                        id: req.id.clone(),
                        value: None,
                        timestamp: Some(timestamp),
                        error: Some(e),
                    }
                }
            }
        })
        .collect();

    let success = results.iter().all(|r| r.error.is_none());

    CustomDataResponse {
        success,
        results,
        error: None,
    }
}

/// Response for test_telegram command
#[derive(Debug, serde::Serialize)]
struct TestTelegramResponse {
    success: bool,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Handle get_public_key command - returns implicit account ID and public key for a PROTECTED_ key
fn handle_get_public_key(key_name: &str) -> types::PublicKeyResponse {
    if let Err(e) = require_protected_key_name(key_name) {
        return types::PublicKeyResponse {
            success: false,
            implicit_account_id: None,
            public_key: None,
            error: Some(e),
        };
    }
    match env::var(key_name) {
        Ok(private_key) => match near_tx::derive_implicit_account(&private_key) {
            Ok((account_id, public_key)) => types::PublicKeyResponse {
                success: true,
                implicit_account_id: Some(account_id),
                public_key: Some(public_key),
                error: None,
            },
            // Never the parse error: it names the character it choked on and where, so a
            // caller naming one PROTECTED_ variable after another could read them out
            Err(_) => types::PublicKeyResponse {
                success: false,
                implicit_account_id: None,
                public_key: None,
                error: Some(format!("{} does not hold a valid ed25519 key", key_name)),
            },
        },
        Err(_) => types::PublicKeyResponse {
            success: false,
            implicit_account_id: None,
            public_key: None,
            error: Some(format!("{} not found in environment", key_name)),
        },
    }
}

/// Handle test_telegram command - sends a test message to configured Telegram chat
fn handle_test_telegram(custom_message: Option<&str>) -> TestTelegramResponse {
    use std::env;

    // Check if Telegram is configured
    let bot_token = env::var("TELEGRAM_BOT_TOKEN");
    let chat_id = env::var("TELEGRAM_CHAT_ID");

    match (bot_token, chat_id) {
        (Ok(_), Ok(_)) => {
            let message = custom_message.unwrap_or("This is a test message from Oracle WASI");
            telegram::send_alert("Test Alert", message);

            TestTelegramResponse {
                success: true,
                message: "Test alert sent successfully".to_string(),
                error: None,
            }
        }
        (Err(_), _) => TestTelegramResponse {
            success: false,
            message: "Telegram not configured".to_string(),
            error: Some("TELEGRAM_BOT_TOKEN environment variable not set".to_string()),
        },
        (_, Err(_)) => TestTelegramResponse {
            success: false,
            message: "Telegram not configured".to_string(),
            error: Some("TELEGRAM_CHAT_ID environment variable not set".to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(name: &str, price: f64, timestamp: u64) -> SourceInfo {
        SourceInfo {
            name: name.to_string(),
            price,
            timestamp: Some(timestamp),
        }
    }

    /// The contract a consumer relies on: ask for 40 seconds and the answer is built from the
    /// venues seen within 40 seconds, with a `publish_time` no older than that. The slow tier
    /// keeps its entries in the record, but does not get to vote in a window it missed.
    #[test]
    fn a_windowed_price_is_built_only_from_sources_inside_the_window() {
        let stored = StoredPrice::new(
            0.0, // deliberately wrong: nothing may serve the stored scalar
            1_200,
            vec![
                source("mexc", 100.0, 1_195),
                source("okx", 102.0, 1_190),
                source("pyth", 500.0, 1_100),
                source("chainlink", 500.0, 1_090),
            ],
            "median",
        );

        let tight =
            windowed_price(&stored, 1_200, 40, &[], AggregationMethod::Median, 1).unwrap();
        assert_eq!(tight.price, 101.0);
        assert_eq!(tight.timestamp, 1_190);
        assert_eq!(tight.sources.len(), 2);
        assert!(1_200 - tight.timestamp <= 40);

        // widening lets the slow tier back in and drags the honest staleness bound with it
        let wide =
            windowed_price(&stored, 1_200, 120, &[], AggregationMethod::Median, 1).unwrap();
        assert_eq!(wide.sources.len(), 4);
        assert_eq!(wide.timestamp, 1_090);

        // exclusions compose with the window
        let excluded = windowed_price(
            &stored,
            1_200,
            120,
            &["pyth".to_string(), "chainlink".to_string()],
            AggregationMethod::Median,
            1,
        )
        .unwrap();
        assert_eq!(excluded.price, 101.0);

        // too few sources in the window is a refetch signal, never a served price
        assert!(windowed_price(&stored, 1_200, 40, &[], AggregationMethod::Median, 3).is_none());
        assert!(windowed_price(&stored, 1_200, 1, &[], AggregationMethod::Median, 1).is_none());
    }

    /// A record's write time is not its price's age. Under tiered refresh the fast tier touches
    /// a record every few seconds while Pyth and Chainlink entries in it are minutes old, so any
    /// path that reports `StoredPrice.timestamp` as the age of the data understates it — and the
    /// signed feed gates on exactly that number. Erring old can only cause a rejection; erring
    /// fresh gets a stale price signed as current.
    #[test]
    fn the_reported_age_is_the_oldest_source_not_the_write_time() {
        let stored = StoredPrice::new(
            100.0,
            1_200, // written a moment ago by the fast tier
            vec![
                source("mexc", 100.0, 1_199),
                source("chainlink", 100.0, 1_080), // 120s older
            ],
            "median",
        );

        let all: Vec<&SourceInfo> = stored.sources.iter().collect();
        assert_eq!(stored.oldest_timestamp(&all), Some(1_080));
        assert_ne!(
            stored.oldest_timestamp(&all),
            Some(stored.timestamp),
            "the fallback must not inherit the record's write time"
        );

        // and the windowed path agrees: a 40s window sees only the fresh venue
        let view = windowed_price(&stored, 1_200, 40, &[], AggregationMethod::Median, 1).unwrap();
        assert_eq!(view.timestamp, 1_199);
        assert_eq!(view.sources, vec!["mexc".to_string()]);
    }

    /// Two commands still read an environment variable the caller names. `oracle_keys` reached
    /// `env::var` without this check, so an `update_prices` request could pick any secret in
    /// the worker's environment — including one that is not a signing key at all — and then
    /// learn about it from the failure that followed.
    #[test]
    fn key_names_from_a_request_must_be_protected() {
        assert!(require_protected_key_name("PROTECTED_ORACLE_KEY").is_ok());
        assert!(require_protected_key_name("PROTECTED_ORACLE_KEY_A").is_ok());

        assert!(require_protected_key_name("API_KEY").is_err());
        assert!(require_protected_key_name("TELEGRAM_BOT_TOKEN").is_err());
        assert!(require_protected_key_name("").is_err());
        // the prefix is a prefix, not a substring
        assert!(require_protected_key_name("MY_PROTECTED_ORACLE_KEY").is_err());
        assert!(require_protected_key_name("protected_oracle_key").is_err());

        // Every request-facing entry point that still takes a key name applies it
        assert!(handle_get_public_key("API_KEY").error.is_some());
        assert!(report_prices_to_contract("price-oracle.near", "API_KEY", "{}").is_err());
    }

    /// `get_signed_prices` takes no key name at all any more, so the prefix check is not what
    /// protects it — the absence of the parameter is.
    ///
    /// A caller used to pass `key_name`, and `PROTECTED_KEY_RHEA` (the key that signs
    /// `report_prices` transactions) passed the prefix check like any other. This asserts the
    /// signer is fixed, is a TEE-generated key, and is not the transaction key — the property
    /// that lets the feed skip a domain-separation tag.
    #[test]
    fn feed_signing_key_is_pinned_and_transaction_free() {
        assert!(signed_prices::FEED_SIGNING_KEY.starts_with("PROTECTED_"));
        assert!(require_protected_key_name(signed_prices::FEED_SIGNING_KEY).is_ok());

        // The on-chain push signer registered with the oracle contract (DEPLOY.md,
        // INIT_MAINNET.md, scripts/PROPOSALS.md). If these ever became the same string, the
        // feed and NEAR transactions would share a key and the separation would be gone.
        for transaction_key in [
            "PROTECTED_KEY_RHEA",
            "PROTECTED_KEY_NEAR",
            "PROTECTED_KEY_ETH",
            "PROTECTED_ORACLE_KEY",
        ] {
            assert_ne!(
                signed_prices::FEED_SIGNING_KEY, transaction_key,
                "the feed must not be signed with a key that also signs transactions"
            );
        }

        // And the handler cannot be pointed anywhere else: it takes no key argument. Without
        // the secret in the environment it fails on the pinned name, before any fetch.
        let signed = handle_get_signed_prices(
            &["wrap.near".to_string()],
            120,
            None,
            None,
            None,
            AggregationMethod::Median,
            1,
        );
        assert!(!signed.success);
        assert!(signed
            .error
            .unwrap()
            .contains(signed_prices::FEED_SIGNING_KEY));
    }
}
