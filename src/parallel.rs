use crate::sources::fetch_price_with_config;
use crate::types::{DataRequest, SourcePrice, ExecutionConfig};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Result from a source fetch operation
type FetchResult = Result<SourcePrice, String>;

/// Fetch prices from all sources in parallel with concurrency limit
pub fn fetch_prices_parallel(
    data_req: &DataRequest,
    coingecko_key: Option<&str>,
    coinmarketcap_key: Option<&str>,
    twelvedata_key: Option<&str>,
    config: &ExecutionConfig,
) -> (Vec<SourcePrice>, Vec<String>) {
    let mut source_prices = Vec::new();
    let mut errors = Vec::new();

    // Shared results collector
    let results = Arc::new(Mutex::new(Vec::new()));

    // Process sources in batches to limit concurrency
    let sources = &data_req.sources;

    for chunk in sources.chunks(config.max_concurrent_requests) {
        let mut chunk_threads = Vec::new();

        for source_config in chunk {
            // Clone needed values for the thread
            let source_config = source_config.clone();
            let data_req_id = data_req.id.clone();
            let results_clone = Arc::clone(&results);
            let request_timeout_secs = config.request_timeout_secs;

            // Get API key for this source
            let api_key = match source_config.name.as_str() {
                "coingecko" => coingecko_key.map(|s| s.to_string()),
                "coinmarketcap" => coinmarketcap_key.map(|s| s.to_string()),
                "twelvedata" => twelvedata_key.map(|s| s.to_string()),
                _ => None,
            };

            // Spawn thread for this source
            let handle = thread::spawn(move || {
                let id = source_config.id.clone().unwrap_or(data_req_id);
                let source_name = source_config.name.clone();

                // Apply timeout check
                let start = Instant::now();
                let result = fetch_price_with_config(
                    &source_config.name,
                    &id,
                    api_key.as_deref(),
                    source_config.custom.as_ref()
                );

                let elapsed = start.elapsed();

                // Store result
                let fetch_result: FetchResult = if elapsed > Duration::from_secs(request_timeout_secs) {
                    Err(format!("{}: Request timeout after {} seconds", source_name, request_timeout_secs))
                } else {
                    match result {
                        Ok(price) => Ok(price),
                        Err(e) => Err(format!("{}: {}", source_name, e)),
                    }
                };

                // Add to shared results
                if let Ok(mut results) = results_clone.lock() {
                    results.push(fetch_result);
                }
            });

            chunk_threads.push(handle);
        }

        // Wait for this batch to complete before starting next batch
        for handle in chunk_threads {
            if let Ok(_) = handle.join() {
                // Thread completed
            }
        }
    }

    // Collect results
    if let Ok(results) = results.lock() {
        for result in results.iter() {
            match result {
                Ok(price) => source_prices.push(price.clone()),
                Err(e) => errors.push(e.clone()),
            }
        }
    }

    (source_prices, errors)
}

/// Process multiple data requests in parallel
pub fn process_data_requests_parallel(
    requests: Vec<DataRequest>,
    max_deviation: f64,
    coingecko_key: Option<&str>,
    coinmarketcap_key: Option<&str>,
    twelvedata_key: Option<&str>,
    config: &ExecutionConfig,
) -> Vec<crate::types::DataResponse> {
    let results = Arc::new(Mutex::new(Vec::new()));
    let mut threads = Vec::new();

    // Process requests in parallel
    for (index, data_req) in requests.into_iter().enumerate() {
        let results_clone = Arc::clone(&results);
        let coingecko_key = coingecko_key.map(|s| s.to_string());
        let coinmarketcap_key = coinmarketcap_key.map(|s| s.to_string());
        let twelvedata_key = twelvedata_key.map(|s| s.to_string());
        let config = config.clone();

        let handle = thread::spawn(move || {
            // Fetch prices from all sources for this token
            let (source_prices, errors) = fetch_prices_parallel(
                &data_req,
                coingecko_key.as_deref(),
                coinmarketcap_key.as_deref(),
                twelvedata_key.as_deref(),
                &config
            );

            // Process results using existing logic
            let response = process_fetched_data(data_req, source_prices, errors, max_deviation);

            // Store result with index to maintain order
            if let Ok(mut results) = results_clone.lock() {
                results.push((index, response));
            }
        });

        threads.push(handle);
    }

    // Wait for all threads to complete
    for handle in threads {
        let _ = handle.join();
    }

    // Sort results by index to maintain original order
    let mut sorted_results = Vec::new();
    if let Ok(mut results) = results.lock() {
        results.sort_by_key(|&(idx, _)| idx);
        sorted_results = results.drain(..).map(|(_, response)| response).collect();
    }

    sorted_results
}

/// Process fetched data into response (extracted from main.rs process_data_request)
fn process_fetched_data(
    data_req: DataRequest,
    source_prices: Vec<SourcePrice>,
    errors: Vec<String>,
    max_deviation: f64,
) -> crate::types::DataResponse {
    use crate::aggregation;
    use crate::types::{DataResponse, DataValue, PriceData};

    // Check if we have enough successful responses
    if source_prices.len() < data_req.min_sources_num {
        let error_msg = format!(
            "Not enough sources responded ({}/{}). Errors: {}",
            source_prices.len(),
            data_req.min_sources_num,
            errors.join(", ")
        );

        return DataResponse {
            id: data_req.id.clone(),
            data: None,
            message: Some(error_msg),
        };
    }

    // Determine if we have numeric values for aggregation
    let has_numeric = source_prices.iter().any(|p| p.value.as_number().is_some());

    // Use the latest timestamp from all sources
    let latest_timestamp = source_prices.iter().map(|p| p.timestamp).max().unwrap_or(0);

    // Collect source names
    let source_names: Vec<String> = source_prices.iter().map(|p| p.source_name.clone()).collect();

    // Build error message if any sources failed (but we still have enough)
    let message = if !errors.is_empty() {
        Some(errors.join(", "))
    } else {
        None
    };

    // Get final value: aggregate if numeric, otherwise take first value
    let final_value = if has_numeric {
        // Check price deviation for numeric values
        let deviation = aggregation::calculate_price_deviation(&source_prices);
        if deviation > max_deviation {
            let error_msg = format!(
                "Price deviation too high: {:.2}% (max: {:.2}%)",
                deviation, max_deviation
            );

            return DataResponse {
                id: data_req.id.clone(),
                data: None,
                message: Some(error_msg),
            };
        }

        // Aggregate numeric values
        match aggregation::aggregate_prices(&source_prices, &data_req.aggregation_method) {
            Ok(price) => DataValue::Number(price),
            Err(e) => {
                return DataResponse {
                    id: data_req.id.clone(),
                    data: None,
                    message: Some(format!("Aggregation failed: {}", e)),
                };
            }
        }
    } else {
        // No numeric values - return first value as-is (text or boolean)
        source_prices[0].value.clone()
    };

    // Build detailed message with source prices for numeric aggregation
    let detailed_message = if has_numeric && source_prices.len() > 1 {
        let source_details: Vec<String> = source_prices.iter()
            .filter_map(|p| {
                p.value.as_number().map(|n| format!("{}: {:.6}", p.source_name, n))
            })
            .collect();

        let aggregation_label = match data_req.aggregation_method {
            crate::types::AggregationMethod::Average => "avg",
            crate::types::AggregationMethod::Median => "median",
            crate::types::AggregationMethod::WeightedAvg => "weighted",
        };

        if let DataValue::Number(final_price) = final_value {
            let details = source_details.join(", ");
            let agg_info = format!("{}, {}: {:.6}", details, aggregation_label, final_price);

            // Add error info if any sources failed
            if !errors.is_empty() {
                Some(format!("{}. Errors: {}", agg_info, errors.join(", ")))
            } else {
                Some(agg_info)
            }
        } else {
            message
        }
    } else {
        message
    };

    DataResponse {
        id: data_req.id.clone(),
        data: Some(PriceData {
            value: final_value,
            timestamp: latest_timestamp,
            sources: source_names,
        }),
        message: detailed_message,
    }
}