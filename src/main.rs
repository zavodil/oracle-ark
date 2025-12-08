mod aggregation;
mod sources;
mod types;
mod parallel;

use types::*;
use std::env;
use std::io::{self, Read, Write};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    // Get execution config or use defaults
    let config = request.config.unwrap_or_default();

    // Process all data requests in parallel (concurrent async)
    let data_responses = parallel::process_data_requests_parallel(
        request.requests,
        request.max_price_deviation_percent,
        coingecko_key.as_deref(),
        coinmarketcap_key.as_deref(),
        twelvedata_key.as_deref(),
        &config,
    ).await;

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
