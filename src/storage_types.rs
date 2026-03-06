//! Types for storing prices in public storage

use serde::{Deserialize, Serialize};

/// Price stored in public storage
/// Key format: "price:{token_id}" (e.g., "price:bitcoin", "price:wrap.near")
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPrice {
    /// Aggregated price value in USD
    pub price: f64,

    /// Unix timestamp when the price was fetched (seconds)
    pub timestamp: u64,

    /// Information about sources that contributed to this price
    pub sources: Vec<SourceInfo>,

    /// Aggregation method used ("average", "median", "weighted_avg")
    pub aggregation_method: String,

    /// Unix timestamp of last successful report_prices to contract (seconds)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_contract_report: Option<u64>,
}

/// Information about a single price source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    /// Source name (e.g., "coingecko", "binance", "pyth")
    pub name: String,

    /// Price from this source
    pub price: f64,

    /// Timestamp from this source (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<u64>,
}

impl StoredPrice {
    /// Create a new StoredPrice
    pub fn new(
        price: f64,
        timestamp: u64,
        sources: Vec<SourceInfo>,
        aggregation_method: &str,
    ) -> Self {
        Self {
            price,
            timestamp,
            sources,
            aggregation_method: aggregation_method.to_string(),
            last_contract_report: None,
        }
    }

    /// Check if the price is fresh (within max_age_secs)
    pub fn is_fresh(&self, current_timestamp: u64, max_age_secs: u64) -> bool {
        current_timestamp.saturating_sub(self.timestamp) <= max_age_secs
    }

    /// Get storage key for a token
    pub fn storage_key(token: &str) -> String {
        format!("price:{}", token)
    }
}
