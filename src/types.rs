use serde::{Deserialize, Serialize};

// Maximum number of tokens allowed per request
pub const MAX_TOKENS_PER_REQUEST: usize = 10;

/// Aggregation method for combining prices from multiple sources
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregationMethod {
    Average,     // Arithmetic mean
    Median,      // Median value (protection against outliers)
    WeightedAvg, // Weighted average (currently uses equal weights)
}

/// Price source configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PriceSource {
    /// Source name: "coingecko", "coinmarketcap", "twelvedata", "exchangerate-api", "custom"
    pub name: String,

    /// Token ID specific to this source (null means use top-level token_id)
    pub token_id: Option<String>,

    /// Custom source configuration (only for "custom" source)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom: Option<CustomSourceConfig>,
}

/// Value type for custom sources
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ValueType {
    Number,  // f64
    String,  // String (stored in separate field)
    Boolean, // bool (converted to 1.0/0.0 for aggregation)
}

impl Default for ValueType {
    fn default() -> Self {
        ValueType::Number
    }
}

/// Custom source configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomSourceConfig {
    /// HTTP URL to fetch data from
    pub url: String,

    /// JSON path to extract value (dot notation, e.g. "data.price" or "rates.USD")
    pub json_path: String,

    /// Type of value to extract (default: number)
    #[serde(default)]
    pub value_type: ValueType,

    /// Optional HTTP method (default: GET)
    #[serde(default = "default_http_method")]
    pub method: String,

    /// Optional HTTP headers as key-value pairs
    #[serde(default)]
    pub headers: Vec<(String, String)>,
}

fn default_http_method() -> String {
    "GET".to_string()
}

/// Token price request
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TokenRequest {
    /// Main token identifier
    pub token_id: String,

    /// List of price sources to query
    pub sources: Vec<PriceSource>,

    /// Method to aggregate prices from multiple sources (default: average)
    #[serde(default = "default_aggregation_method")]
    pub aggregation_method: AggregationMethod,

    /// Minimum number of sources that must respond successfully (default: 1)
    #[serde(default = "default_min_sources")]
    pub min_sources_num: usize,
}

fn default_aggregation_method() -> AggregationMethod {
    AggregationMethod::Average
}

fn default_min_sources() -> usize {
    1
}

/// Main request structure
#[derive(Debug, Deserialize, Serialize)]
pub struct OracleRequest {
    /// List of tokens to fetch prices for
    pub tokens: Vec<TokenRequest>,

    /// Maximum allowed price deviation between sources (percentage)
    pub max_price_deviation_percent: f64,
}

/// Data value type - can be number, text, or boolean
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DataValue {
    Number(f64),
    Text(String),
    Boolean(bool),
}

impl DataValue {
    /// Get numeric value (for aggregation)
    pub fn as_number(&self) -> Option<f64> {
        match self {
            DataValue::Number(n) => Some(*n),
            DataValue::Boolean(b) => Some(if *b { 1.0 } else { 0.0 }),
            DataValue::Text(_) => None,
        }
    }
}

/// Data for a token (can be numeric, text, or boolean value)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceData {
    /// Value (number, text, or boolean)
    pub value: DataValue,

    /// Unix timestamp when the data was fetched
    pub timestamp: u64,

    /// List of sources that successfully returned data
    pub sources: Vec<String>,
}

/// Response for a single token
#[derive(Debug, Serialize, Deserialize)]
pub struct TokenResponse {
    /// Token identifier
    pub token: String,

    /// Price data (None if failed to fetch)
    pub data: Option<PriceData>,

    /// Error/info message (None if successful)
    pub message: Option<String>,
}

/// Main response structure
#[derive(Debug, Serialize, Deserialize)]
pub struct OracleResponse {
    /// List of token responses
    pub tokens: Vec<TokenResponse>,
}

/// Internal structure for source data result
#[derive(Debug, Clone)]
pub struct SourcePrice {
    pub source_name: String,
    pub value: DataValue,
    pub timestamp: u64,
}
