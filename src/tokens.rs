//! Token configuration embedded at compile time
//!
//! Loads tokens.json and provides validation for allowed tokens.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;

/// Token configuration from tokens.json
#[derive(Debug, Clone, Deserialize)]
pub struct TokenConfig {
    pub decimals: u8,
    #[serde(default)]
    pub stablecoin: bool,
    pub coingecko: Option<String>,
    pub binance: Option<String>,
    pub binance_us: Option<String>,
    pub binance_alpha: Option<String>,
    pub huobi: Option<String>,
    pub cryptocom: Option<String>,
    pub kucoin: Option<String>,
    pub gate: Option<String>,
    pub pyth: Option<String>,
    pub chainlink: Option<String>,
}

/// Embedded tokens.json at compile time
const TOKENS_JSON: &str = include_str!("../tokens.json");

/// Parsed tokens configuration
static TOKENS: LazyLock<HashMap<String, TokenConfig>> = LazyLock::new(|| {
    serde_json::from_str(TOKENS_JSON).expect("Invalid tokens.json")
});

/// Check if a token is in the allowed list
pub fn is_allowed(token_id: &str) -> bool {
    TOKENS.contains_key(token_id)
}

/// Get list of all allowed token IDs
pub fn allowed_tokens() -> Vec<String> {
    TOKENS.keys().cloned().collect()
}

/// Get config for a specific token
pub fn get_config(token_id: &str) -> Option<&'static TokenConfig> {
    TOKENS.get(token_id)
}

/// Filter tokens to only include allowed ones, returns (allowed, rejected)
pub fn filter_allowed(tokens: &[String]) -> (Vec<String>, Vec<String>) {
    let mut allowed = Vec::new();
    let mut rejected = Vec::new();

    for token in tokens {
        if is_allowed(token) {
            allowed.push(token.clone());
        } else {
            rejected.push(token.clone());
        }
    }

    (allowed, rejected)
}
