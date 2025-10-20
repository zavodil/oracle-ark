mod aggregation;
mod sources;
mod types;

use sources::fetch_price_with_config;
use types::*;
use std::env;
use std::io::{self, Read, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read input from stdin
    let mut input_string = String::new();
    io::stdin().read_to_string(&mut input_string)?;

    // Parse JSON request
    let request: OracleRequest = serde_json::from_str(&input_string)?;

    // Validate: check max tokens limit
    if request.requests.len() > MAX_TOKENS_PER_REQUEST {
        let error = format!(
            "Too many tokens requested: {} (max: {})",
            request.requests.len(),
            MAX_TOKENS_PER_REQUEST
        );
        print!("{}", error);
        io::stdout().flush()?;
        return Ok(());
    }

    // Get API keys from environment (encrypted secrets)
    let coingecko_key = env::var("COINGECKO_API_KEY").ok();
    let coinmarketcap_key = env::var("COINMARKETCAP_API_KEY").ok();
    let twelvedata_key = env::var("TWELVEDATA_API_KEY").ok();

    let mut data_responses = Vec::new();

    // Process each token sequentially
    for data_req in request.requests {
        let response = process_data_request(
            &data_req,
            request.max_price_deviation_percent,
            coingecko_key.as_deref(),
            coinmarketcap_key.as_deref(),
            twelvedata_key.as_deref(),
        );

        data_responses.push(response);
    }

    // Build response
    let oracle_response = OracleResponse {
        results: data_responses,
    };

    // Output JSON response to stdout
    let output = serde_json::to_string(&oracle_response)?;
    print!("{}", output);
    io::stdout().flush()?;

    Ok(())
}

/// Process single token request
fn process_data_request(
    data_req: &DataRequest,
    max_deviation: f64,
    coingecko_key: Option<&str>,
    coinmarketcap_key: Option<&str>,
    twelvedata_key: Option<&str>,
) -> DataResponse {
    let mut source_prices: Vec<SourcePrice> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    // Fetch prices from all sources sequentially
    for source_config in &data_req.sources {
        // Determine which id to use for this source
        let id = source_config
            .id
            .as_ref()
            .unwrap_or(&data_req.id);

        // Get API key for this source
        let api_key = match source_config.name.as_str() {
            "coingecko" => coingecko_key,
            "coinmarketcap" => coinmarketcap_key,
            "twelvedata" => twelvedata_key,
            _ => None,
        };

        // Fetch price from source (with custom config support)
        match fetch_price_with_config(&source_config.name, id, api_key, source_config.custom.as_ref()) {
            Ok(price) => source_prices.push(price),
            Err(e) => errors.push(format!("{}: {}", source_config.name, e)),
        }
    }

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
            Ok(price) => types::DataValue::Number(price),
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
            types::AggregationMethod::Average => "avg",
            types::AggregationMethod::Median => "median",
            types::AggregationMethod::WeightedAvg => "weighted",
        };

        if let types::DataValue::Number(final_price) = final_value {
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
