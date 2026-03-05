use serde::{Deserialize, Serialize};

// Default max age for cached prices (2 minutes)
pub const DEFAULT_MAX_AGE_SECS: u64 = 120;

// Price deviation threshold for alerts (5%)
pub const PRICE_DEVIATION_ALERT_THRESHOLD: f64 = 5.0;

// Default minimum number of sources required
pub const DEFAULT_MIN_SOURCES: u8 = 1;

/// Aggregation method for combining prices from multiple sources
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AggregationMethod {
    /// Arithmetic mean of all prices
    Average,
    /// Median value (default, more robust against outliers)
    #[default]
    Median,
    /// Weighted average (currently same as average, can be extended)
    WeightedAverage,
}

impl AggregationMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            AggregationMethod::Average => "average",
            AggregationMethod::Median => "median",
            AggregationMethod::WeightedAverage => "weighted_average",
        }
    }
}

fn default_aggregation() -> AggregationMethod {
    AggregationMethod::default()
}

fn default_min_sources() -> u8 {
    DEFAULT_MIN_SOURCES
}

/// Command enum for routing different request types
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum OracleCommand {
    /// Update prices in public storage (triggered by scheduler)
    /// WASI fetches prices from sources and stores them
    UpdatePrices {
        /// Tokens to update (e.g., ["bitcoin", "ethereum"])
        tokens: Vec<String>,
        /// Whether to also call report_prices on the contract
        #[serde(default)]
        update_contract: bool,
        /// Contract ID to update (if update_contract is true)
        #[serde(skip_serializing_if = "Option::is_none")]
        contract_id: Option<String>,
        /// How to aggregate prices from multiple sources (default: median)
        #[serde(default = "default_aggregation")]
        aggregation_method: AggregationMethod,
        /// Minimum number of sources required (default: 1)
        #[serde(default = "default_min_sources")]
        min_sources_num: u8,
        /// Per-asset oracle key mapping: asset_id -> PROTECTED_ env var name
        /// E.g., {"wrap.near": "PROTECTED_ORACLE_KEY_A", "usdt.tether-token.near": "PROTECTED_ORACLE_KEY_B"}
        /// Assets not in this map use the default PROTECTED_ORACLE_KEY
        /// If omitted, all assets use PROTECTED_ORACLE_KEY
        #[serde(default)]
        oracle_keys: Option<std::collections::HashMap<String, String>>,
    },

    /// Get prices (for blockchain requests via yield/resume)
    /// Returns cached prices if fresh, otherwise fetches new ones
    GetPrices {
        /// Whitelisted tokens to get prices for
        tokens: Vec<String>,
        /// Maximum age of cached price in seconds (default: 120)
        #[serde(default = "default_max_age")]
        max_age_secs: u64,
        /// How to aggregate prices from multiple sources (default: median)
        #[serde(default = "default_aggregation")]
        aggregation_method: AggregationMethod,
        /// Minimum number of sources required (default: 1)
        #[serde(default = "default_min_sources")]
        min_sources_num: u8,
    },

    /// Force update prices - anyone can call if they pay for execution
    /// Always fetches fresh prices from sources, ignoring cache
    ForceUpdate {
        /// Tokens to force update
        tokens: Vec<String>,
        /// How to aggregate prices from multiple sources (default: median)
        #[serde(default = "default_aggregation")]
        aggregation_method: AggregationMethod,
        /// Minimum number of sources required (default: 1)
        #[serde(default = "default_min_sources")]
        min_sources_num: u8,
    },

    /// Fetch price for any token from external API (not whitelisted)
    /// Returns price directly without storing in public storage
    FetchExternal {
        /// Token identifier (depends on source):
        /// - CoinGecko: "bitcoin", "ethereum", "near"
        /// - Binance: "BTCUSDT", "ETHUSDT", "NEARUSDT"
        /// - Pyth: price feed ID
        token_id: String,
        /// Which API source to use
        source: ExternalPriceSource,
    },

    /// Fetch custom data from external sources (for custom_call)
    /// Used for any data: prices, weather, game data, API responses, etc.
    FetchCustomData {
        /// List of data requests
        requests: Vec<CustomDataRequest>,
    },

    /// Test Telegram alert delivery
    TestTelegram {
        /// Optional custom message to send
        #[serde(default)]
        message: Option<String>,
    },

    /// Get public key and implicit account ID for a PROTECTED_ key
    /// Used by DAO to verify TEE worker identity before registering as oracle
    GetPublicKey {
        /// Environment variable name (default: "PROTECTED_ORACLE_KEY")
        #[serde(default = "default_oracle_key_name")]
        key_name: String,
    },

    /// Sync asset exchange configs to public storage.
    /// Called by the oracle contract after DAO config updates.
    /// WASI stores the full config map in public storage key "config:assets".
    SyncAssetConfigs {
        /// Full config map: asset_id -> exchange config JSON string
        configs: std::collections::HashMap<String, String>,
    },
}

/// External price source for non-whitelisted tokens
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExternalPriceSource {
    CoinGecko,
    Binance,
    Pyth,
    /// Custom source - fetch from any URL with JSON path extraction
    /// Use API_KEY environment variable (via secrets) to pass API keys
    Custom(CustomSourceConfig),
}

impl std::fmt::Display for ExternalPriceSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExternalPriceSource::CoinGecko => write!(f, "coingecko"),
            ExternalPriceSource::Binance => write!(f, "binance"),
            ExternalPriceSource::Pyth => write!(f, "pyth"),
            ExternalPriceSource::Custom(_) => write!(f, "custom"),
        }
    }
}

/// Response for external price fetch (single token from single source)
#[derive(Debug, Serialize, Deserialize)]
pub struct ExternalPriceResponse {
    pub success: bool,
    pub token_id: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Warning about single-source price
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

fn default_max_age() -> u64 {
    DEFAULT_MAX_AGE_SECS
}

/// Response for new command format
#[derive(Debug, Serialize, Deserialize)]
pub struct CommandResponse {
    pub success: bool,
    pub prices: Vec<PriceResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Single price result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceResult {
    pub token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_cache: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Custom source configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomSourceConfig {
    /// HTTP URL to fetch data from
    pub url: String,

    /// JSON path to extract value (dot notation, e.g. "data.price" or "rates.USD")
    pub json_path: String,

    /// Type of value to extract: "number", "string", "boolean" (default: "number")
    #[serde(default = "default_value_type")]
    pub value_type: String,

    /// Optional HTTP method (default: GET)
    #[serde(default = "default_http_method")]
    pub method: String,

    /// Optional HTTP headers as key-value pairs
    #[serde(default)]
    pub headers: Vec<(String, String)>,

    /// Optional JSON body for POST/PUT requests
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
}

fn default_oracle_key_name() -> String {
    "PROTECTED_ORACLE_KEY".to_string()
}

fn default_value_type() -> String {
    "number".to_string()
}

fn default_http_method() -> String {
    "GET".to_string()
}

/// Request for custom data (matches contract's CustomDataRequest)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomDataRequest {
    /// Identifier for the data in callback
    pub id: String,
    /// Token/query identifier for the source (required for coingecko/binance/pyth, optional for custom)
    #[serde(default)]
    pub token_id: String,
    /// Data source to query
    pub source: ExternalPriceSource,
}

/// Response for fetch_custom_data command
#[derive(Debug, Serialize, Deserialize)]
pub struct CustomDataResponse {
    pub success: bool,
    pub results: Vec<CustomDataResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Single custom data result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomDataResult {
    pub id: String,
    /// Value as JSON (number, string, etc. based on value_type)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Response for sync_asset_configs command
#[derive(Debug, Serialize, Deserialize)]
pub struct SyncResponse {
    pub success: bool,
    pub count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Response for get_public_key command
#[derive(Debug, Serialize, Deserialize)]
pub struct PublicKeyResponse {
    pub success: bool,
    /// NEAR implicit account ID (hex-encoded ed25519 public key, 64 chars)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implicit_account_id: Option<String>,
    /// Public key in NEAR format (ed25519:base58...)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
