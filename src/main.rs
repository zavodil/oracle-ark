mod near_tx;
mod sources;
mod storage_types;
mod telegram;
mod tokens;
mod types;

use oracle_ark_sources::parsers;
use oracle_ark_sources::sources::sync as shared_sources;
use outlayer::storage;
use storage_types::{SourceInfo, StoredPrice};
use types::*;
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
                update_contract,
                contract_id,
                aggregation_method,
                min_sources_num,
                oracle_keys,
            } => {
                let response = handle_update_prices(
                    &tokens,
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

/// Handle update_prices command (triggered by scheduler)
/// Fetches prices from sources IN TEE and stores in public storage
fn handle_update_prices(
    tokens: &[String],
    update_contract: bool,
    contract_id: Option<&str>,
    aggregation_method: AggregationMethod,
    min_sources_num: u8,
    oracle_keys: Option<&std::collections::HashMap<String, String>>,
) -> CommandResponse {
    let mut results = Vec::new();

    // Filter to only allowed tokens
    let (allowed, rejected) = tokens::filter_allowed(tokens);
    for token in rejected {
        results.push(PriceResult {
            token,
            price: None,
            timestamp: None,
            sources: None,
            from_cache: None,
            error: Some("Token not in allowed list".to_string()),
        });
    }

    // Get universal API key from environment (used for CoinGecko, etc.)
    let api_key = env::var("API_KEY").ok();

    for token in allowed {
        match fetch_and_store_price(&token, api_key.as_deref(), aggregation_method, min_sources_num) {
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
    if oracle_ark_sources::CHAINLINK_DISABLED.load(std::sync::atomic::Ordering::Relaxed) {
        telegram::send_alert(
            "Chainlink Disabled",
            "All Chainlink Ethereum RPCs failed. Chainlink source disabled for this run.\nPrices still available from other sources.",
        );
    }

    // If update_contract is true, sign and send report_prices tx to the oracle contract
    if update_contract {
        if let Some(contract_id) = contract_id {
            // Build AssetPrice entries from successful results
            // Contract Price format: { multiplier: u128 as string, decimals: u8 }
            // We use decimals=8, so multiplier = price * 10^8
            let prices_for_contract: Vec<(String, serde_json::Value)> = results.iter()
                .filter(|r| r.price.is_some() && r.error.is_none())
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
                let mut by_key: std::collections::HashMap<String, Vec<serde_json::Value>> =
                    std::collections::HashMap::new();

                if let Some(keys) = oracle_keys {
                    // Only push assets that have explicit key assignments
                    for (asset_id, price_json) in &prices_for_contract {
                        if let Some(key_name) = keys.get(asset_id) {
                            by_key.entry(key_name.clone()).or_default().push(price_json.clone());
                        }
                        // Assets without key mapping are warm-only (not pushed to contract)
                    }
                } else {
                    // No oracle_keys provided — push all with default key
                    let default_key = "PROTECTED_ORACLE_KEY".to_string();
                    for (_, price_json) in &prices_for_contract {
                        by_key.entry(default_key.clone()).or_default().push(price_json.clone());
                    }
                }

                // Send one transaction per key
                for (key_name, prices) in &by_key {
                    let args = serde_json::json!({ "prices": prices });
                    match report_prices_to_contract(contract_id, key_name, &args.to_string()) {
                        Ok(hash) => eprintln!("Contract updated via {}: {}", key_name, hash),
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
                                    prices.iter().filter_map(|p| p["asset_id"].as_str()).collect::<Vec<_>>(),
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

/// Sign and send report_prices transaction to the oracle contract
/// Uses a PROTECTED_ key (TEE-generated) and derives implicit account from it
fn report_prices_to_contract(contract_id: &str, key_name: &str, args: &str) -> Result<String, String> {
    let signer_key = env::var(key_name)
        .map_err(|_| format!("{} not set", key_name))?;
    let rpc_url = env::var("NEAR_RPC_URL")
        .unwrap_or_else(|_| "https://rpc.mainnet.near.org".to_string());

    // Derive implicit account ID from the protected key
    let (signer_id, _) = near_tx::derive_implicit_account(&signer_key)
        .map_err(|e| format!("Failed to derive implicit account from {}: {}", key_name, e))?;

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

/// Handle get_prices command (blockchain requests)
/// Returns cached prices if fresh, otherwise fetches new ones
fn handle_get_prices(
    tokens: &[String],
    max_age_secs: u64,
    aggregation_method: AggregationMethod,
    min_sources_num: u8,
) -> CommandResponse {
    let mut results = Vec::new();
    let now = current_timestamp();

    // Filter to only allowed tokens
    let (allowed, rejected) = tokens::filter_allowed(tokens);
    for token in rejected {
        results.push(PriceResult {
            token,
            price: None,
            timestamp: None,
            sources: None,
            from_cache: None,
            error: Some("Token not in allowed list".to_string()),
        });
    }

    // Get universal API key for potential fresh fetches
    let api_key = env::var("API_KEY").ok();

    for token in allowed {
        let key = StoredPrice::storage_key(&token);

        // Try to read from public storage
        match storage::get_worker(&key) {
            Ok(Some(data)) => {
                match serde_json::from_slice::<StoredPrice>(&data) {
                    Ok(stored) => {
                        if stored.is_fresh(now, max_age_secs) {
                            // Return cached price
                            results.push(PriceResult {
                                token,
                                price: Some(stored.price),
                                timestamp: Some(stored.timestamp),
                                sources: Some(stored.sources.iter().map(|s| s.name.clone()).collect()),
                                from_cache: Some(true),
                                error: None,
                            });
                        } else {
                            // Cached price is stale, fetch fresh
                            match fetch_and_store_price(&token, api_key.as_deref(), aggregation_method, min_sources_num) {
                                Ok(new_stored) => {
                                    results.push(PriceResult {
                                        token,
                                        price: Some(new_stored.price),
                                        timestamp: Some(new_stored.timestamp),
                                        sources: Some(
                                            new_stored.sources.iter().map(|s| s.name.clone()).collect(),
                                        ),
                                        from_cache: Some(false),
                                        error: None,
                                    });
                                }
                                Err(e) => {
                                    // Return stale price with warning
                                    results.push(PriceResult {
                                        token,
                                        price: Some(stored.price),
                                        timestamp: Some(stored.timestamp),
                                        sources: Some(
                                            stored.sources.iter().map(|s| s.name.clone()).collect(),
                                        ),
                                        from_cache: Some(true),
                                        error: Some(format!("Using stale cache, fetch failed: {}", e)),
                                    });
                                }
                            }
                        }
                    }
                    Err(e) => {
                        // Corrupted cache, fetch fresh
                        match fetch_and_store_price(&token, api_key.as_deref(), aggregation_method, min_sources_num) {
                            Ok(new_stored) => {
                                results.push(PriceResult {
                                    token,
                                    price: Some(new_stored.price),
                                    timestamp: Some(new_stored.timestamp),
                                    sources: Some(
                                        new_stored.sources.iter().map(|s| s.name.clone()).collect(),
                                    ),
                                    from_cache: Some(false),
                                    error: None,
                                });
                            }
                            Err(fetch_err) => {
                                results.push(PriceResult {
                                    token,
                                    price: None,
                                    timestamp: None,
                                    sources: None,
                                    from_cache: None,
                                    error: Some(format!(
                                        "Cache parse error: {}, fetch error: {}",
                                        e, fetch_err
                                    )),
                                });
                            }
                        }
                    }
                }
            }
            Ok(None) => {
                // No cached price, fetch fresh
                match fetch_and_store_price(&token, api_key.as_deref(), aggregation_method, min_sources_num) {
                    Ok(new_stored) => {
                        results.push(PriceResult {
                            token,
                            price: Some(new_stored.price),
                            timestamp: Some(new_stored.timestamp),
                            sources: Some(new_stored.sources.iter().map(|s| s.name.clone()).collect()),
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
                            error: Some(format!("No cache and fetch failed: {}", e)),
                        });
                    }
                }
            }
            Err(e) => {
                results.push(PriceResult {
                    token,
                    price: None,
                    timestamp: None,
                    sources: None,
                    from_cache: None,
                    error: Some(format!("Storage error: {}", e)),
                });
            }
        }
    }

    let success = results.iter().all(|r| r.price.is_some());
    CommandResponse {
        success,
        prices: results,
        error: None,
    }
}

/// Handle force_update command - anyone can call if they pay
/// Always fetches fresh prices, ignoring cache
fn handle_force_update(
    tokens: &[String],
    aggregation_method: AggregationMethod,
    min_sources_num: u8,
) -> CommandResponse {
    let mut results = Vec::new();

    // Filter to only allowed tokens
    let (allowed, rejected) = tokens::filter_allowed(tokens);
    for token in rejected {
        results.push(PriceResult {
            token,
            price: None,
            timestamp: None,
            sources: None,
            from_cache: None,
            error: Some("Token not in allowed list".to_string()),
        });
    }

    // Get universal API key from environment
    let api_key = env::var("API_KEY").ok();

    for token in allowed {
        match fetch_and_store_price(&token, api_key.as_deref(), aggregation_method, min_sources_num) {
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
/// - Custom: added as Authorization: Bearer header
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
/// Uses shared oracle-ark-sources crate for consistency with scheduler
fn fetch_and_store_price(
    token: &str,
    api_key: Option<&str>,
    aggregation_method: AggregationMethod,
    min_sources_num: u8,
) -> Result<StoredPrice, String> {
    // Use shared crate's fetch_all_sources for consistency with scheduler
    let source_prices = shared_sources::fetch_all_sources(token, api_key);

    if source_prices.is_empty() {
        // Alert: no sources returned price
        telegram::send_alert(
            "No Price Sources",
            &format!("Token: {}\nAll configured sources failed to return a price", token),
        );
        return Err(format!("No sources available for token: {}", token));
    }

    // Check minimum sources requirement
    if source_prices.len() < min_sources_num as usize {
        return Err(format!(
            "Not enough sources: got {}, required {}",
            source_prices.len(),
            min_sources_num
        ));
    }

    // Get all prices
    let mut prices: Vec<f64> = source_prices.iter().map(|p| p.price).collect();

    // Check price deviation between sources
    let deviation = parsers::price_deviation(&prices);
    if deviation > PRICE_DEVIATION_ALERT_THRESHOLD {
        // Alert: high price deviation
        let source_details: Vec<String> = source_prices
            .iter()
            .map(|p| format!("  {}: ${:.4}", p.source_name, p.price))
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

    // Calculate aggregated price based on method
    let aggregated_price = match aggregation_method {
        AggregationMethod::Average => parsers::average(&prices),
        AggregationMethod::Median => parsers::median(&mut prices),
        AggregationMethod::WeightedAverage => parsers::weighted_average(&prices),
    };

    let timestamp = current_timestamp();

    // Build source info
    let sources: Vec<SourceInfo> = source_prices
        .iter()
        .map(|p| SourceInfo {
            name: p.source_name.clone(),
            price: p.price,
            timestamp: Some(p.timestamp),
        })
        .collect();

    let stored = StoredPrice::new(aggregated_price, timestamp, sources, aggregation_method.as_str());

    // Store in public storage
    let key = StoredPrice::storage_key(token);
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
    if !key_name.starts_with("PROTECTED_") {
        return types::PublicKeyResponse {
            success: false,
            implicit_account_id: None,
            public_key: None,
            error: Some("key_name must start with PROTECTED_".to_string()),
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
            Err(e) => types::PublicKeyResponse {
                success: false,
                implicit_account_id: None,
                public_key: None,
                error: Some(format!("Key derivation failed: {}", e)),
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
