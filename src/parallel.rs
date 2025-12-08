use crate::sources::fetch_price_with_config;
use crate::types::{DataRequest, SourcePrice, ExecutionConfig};
use futures::future::join_all;
use futures::stream::{self, StreamExt};
use std::time::{Duration, Instant};

/// Fetch a single price source asynchronously
async fn fetch_price_async(
    source_name: String,
    id: String,
    api_key: Option<String>,
    custom_config: Option<crate::types::CustomSourceConfig>,
    timeout_secs: u64,
) -> Result<SourcePrice, String> {
    let source_name_clone = source_name.clone();

    // Execute the HTTP request with timeout
    // Note: We use blocking operation in async context since wasi-http-client is synchronous
    let start = Instant::now();

    // Create a timeout future
    let timeout_duration = Duration::from_secs(timeout_secs);

    // Since wasi-http-client is blocking, we need to handle it differently
    // In WASI, we can't use spawn_blocking as it requires Send trait
    // Instead, we'll call the function directly and rely on the HTTP client's internal timeout
    let result = fetch_price_with_config(
        &source_name,
        &id,
        api_key.as_deref(),
        custom_config.as_ref()
    );

    let elapsed = start.elapsed();

    // Check if request took too long
    if elapsed > timeout_duration {
        Err(format!("{}: Request timeout after {} seconds", source_name_clone, timeout_secs))
    } else {
        match result {
            Ok(price) => Ok(price),
            Err(e) => Err(format!("{}: {}", source_name_clone, e)),
        }
    }
}

/// Fetch prices from all sources in parallel with concurrency limit
pub async fn fetch_prices_parallel(
    data_req: &DataRequest,
    coingecko_key: Option<&str>,
    coinmarketcap_key: Option<&str>,
    twelvedata_key: Option<&str>,
    config: &ExecutionConfig,
) -> (Vec<SourcePrice>, Vec<String>) {
    let mut source_prices = Vec::new();
    let mut errors = Vec::new();

    // Create futures for all sources
    let mut futures = Vec::new();

    for source_config in &data_req.sources {
        // Determine which id to use for this source
        let id = source_config.id.clone().unwrap_or_else(|| data_req.id.clone());

        // Get API key for this source
        let api_key = match source_config.name.as_str() {
            "coingecko" => coingecko_key.map(|s| s.to_string()),
            "coinmarketcap" => coinmarketcap_key.map(|s| s.to_string()),
            "twelvedata" => twelvedata_key.map(|s| s.to_string()),
            _ => None,
        };

        let source_name = source_config.name.clone();
        let custom_config = source_config.custom.clone();
        let timeout_secs = config.request_timeout_secs;

        // Create async task for this source
        let future = fetch_price_async(source_name, id, api_key, custom_config, timeout_secs);
        futures.push(future);
    }

    // Execute with concurrency limit
    let results: Vec<Result<SourcePrice, String>> = stream::iter(futures)
        .buffer_unordered(config.max_concurrent_requests)
        .collect()
        .await;

    // Separate successes and errors
    for result in results {
        match result {
            Ok(price) => source_prices.push(price),
            Err(e) => errors.push(e),
        }
    }

    (source_prices, errors)
}

/// Process multiple data requests in parallel
pub async fn process_data_requests_parallel(
    requests: Vec<DataRequest>,
    max_deviation: f64,
    coingecko_key: Option<&str>,
    coinmarketcap_key: Option<&str>,
    twelvedata_key: Option<&str>,
    config: &ExecutionConfig,
) -> Vec<crate::types::DataResponse> {
    // Create futures for all data requests
    let futures = requests.into_iter().map(|data_req| {
        let coingecko_key = coingecko_key.map(|s| s.to_string());
        let coinmarketcap_key = coinmarketcap_key.map(|s| s.to_string());
        let twelvedata_key = twelvedata_key.map(|s| s.to_string());
        let config = config.clone();

        async move {
            // Fetch prices from all sources for this token
            let (source_prices, errors) = fetch_prices_parallel(
                &data_req,
                coingecko_key.as_deref(),
                coinmarketcap_key.as_deref(),
                twelvedata_key.as_deref(),
                &config
            ).await;

            // Process results using existing logic
            process_fetched_data(data_req, source_prices, errors, max_deviation)
        }
    });

    // Execute all data requests in parallel
    join_all(futures).await
}

/// Process fetched data into response
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