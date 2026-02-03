mod asset;
mod ema;
mod oracle;
mod owner;
mod upgrade;
mod utils;

pub use crate::asset::*;
pub use crate::ema::*;
pub use crate::oracle::*;
pub use crate::utils::*;

use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    env, ext_contract, log, near_bindgen, AccountId, Gas, IntoStorageKey, NearToken,
    PanicOnDefault, Promise, PromiseError, PromiseOrValue, Timestamp,
};

const NO_DEPOSIT: NearToken = NearToken::from_yoctonear(0);

const GAS_FOR_PROMISE: Gas = Gas::from_tgas(10);
const GAS_FOR_CALLBACK: Gas = Gas::from_tgas(30);

const NEAR_CLAIM_DURATION: u64 = 24 * 60 * 60 * 10u64.pow(9);
const SAFETY_MARGIN_NEAR_CLAIM: u128 = 1_000_000_000_000_000_000_000_000; // 1 NEAR

/// Minimum deposit to cover OutLayer execution cost (0.01 NEAR)
const MIN_OUTLAYER_DEPOSIT: u128 = 10_000_000_000_000_000_000_000;

/// Minimum contract balance required to subsidize OutLayer calls (20 NEAR)
const MIN_BALANCE_FOR_SUBSIDY: u128 = 20_000_000_000_000_000_000_000_000;

/// Default deposit for subsidized OutLayer calls (0.02 NEAR)
const SUBSIDIZED_OUTLAYER_DEPOSIT: u128 = 20_000_000_000_000_000_000_000;

/// Default max instructions for OutLayer execution (10 billion)
const DEFAULT_MAX_INSTRUCTIONS: u64 = 10_000_000_000;

/// Default max memory for OutLayer execution (128 MB)
const DEFAULT_MAX_MEMORY_MB: u32 = 128;

/// Default max execution time for OutLayer execution (60 seconds)
const DEFAULT_MAX_EXECUTION_SECONDS: u64 = 60;

pub type DurationSec = u32;

#[derive(BorshSerialize)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    Oracles,
    Assets,
}

impl IntoStorageKey for StorageKey {
    fn into_storage_key(self) -> Vec<u8> {
        near_sdk::borsh::to_vec(&self).unwrap()
    }
}

/// Execution source for OutLayer request_execution
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub enum ExecutionSource {
    GitHub {
        repo: String,
        commit: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        build_target: Option<String>,
    },
    WasmUrl {
        url: String,
        hash: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        build_target: Option<String>,
    },
    Project {
        project_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        version_key: Option<String>,
    },
}

/// Resource limits for OutLayer execution
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct ResourceLimits {
    #[serde(default = "default_max_instructions")]
    pub max_instructions: Option<u64>,
    #[serde(default = "default_max_memory_mb")]
    pub max_memory_mb: Option<u32>,
    #[serde(default = "default_max_execution_seconds")]
    pub max_execution_seconds: Option<u64>,
}

fn default_max_instructions() -> Option<u64> {
    Some(DEFAULT_MAX_INSTRUCTIONS)
}

fn default_max_memory_mb() -> Option<u32> {
    Some(DEFAULT_MAX_MEMORY_MB)
}

fn default_max_execution_seconds() -> Option<u64> {
    Some(DEFAULT_MAX_EXECUTION_SECONDS)
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_instructions: Some(DEFAULT_MAX_INSTRUCTIONS),
            max_memory_mb: Some(DEFAULT_MAX_MEMORY_MB),
            max_execution_seconds: Some(DEFAULT_MAX_EXECUTION_SECONDS),
        }
    }
}

/// Response format for OutLayer execution
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub enum ResponseFormat {
    Bytes,
    #[default]
    Text,
    Json,
}

/// External contract interface for OutLayer
#[ext_contract(ext_outlayer)]
#[allow(dead_code)]
trait OutLayer {
    fn request_execution(
        &mut self,
        source: ExecutionSource,
        resource_limits: Option<ResourceLimits>,
        input_data: Option<String>,
        secrets_ref: Option<serde_json::Value>,
        response_format: Option<ResponseFormat>,
        payer_account_id: Option<AccountId>,
        params: Option<serde_json::Value>,
    );
}

/// External contract interface for price receiver (DeFi callbacks)
#[ext_contract(ext_price_receiver)]
pub trait ExtPriceReceiver {
    fn oracle_on_call(&mut self, sender_id: AccountId, data: PriceData, msg: String);
}

/// External contract interface for custom data receiver
#[ext_contract(ext_custom_receiver)]
pub trait ExtCustomReceiver {
    fn on_custom_data(&mut self, sender_id: AccountId, data: Vec<CustomDataResult>, msg: String);
}

/// Result for a single custom data request
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct CustomDataResult {
    /// ID from the request (e.g., "steam:elden_ring")
    pub id: String,
    /// Value as JSON (number, string, etc. based on value_type)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    /// Timestamp when data was fetched
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<u64>,
    /// Error message if fetch failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// External contract interface for self callbacks
#[ext_contract(ext_self)]
#[allow(dead_code)]
trait ExtSelf {
    fn on_outlayer_result(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        asset_ids: Vec<AssetId>,
        msg: String,
        #[callback_result] result: Result<Option<WasiPriceResponse>, PromiseError>,
    ) -> Promise;

    fn on_custom_call_result(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        msg: String,
        #[callback_result] result: Result<Option<WasiCustomDataResponse>, PromiseError>,
    ) -> Promise;

    fn on_request_price_result(
        &mut self,
        asset_ids: Vec<AssetId>,
        #[callback_result] result: Result<Option<WasiPriceResponse>, PromiseError>,
    ) -> PriceData;

    fn on_request_custom_data_result(
        &mut self,
        #[callback_result] result: Result<Option<WasiCustomDataResponse>, PromiseError>,
    ) -> Vec<CustomDataResult>;
}

/// Price source for external (non-whitelisted) token queries
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
#[serde(rename_all = "lowercase")]
pub enum PriceSource {
    CoinGecko,
    Binance,
    Pyth,
    /// Custom source - fetch from any URL with JSON path extraction
    /// Pass API key via secrets (API_KEY environment variable in WASI)
    Custom(CustomSourceConfig),
}

/// Configuration for custom price source
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct CustomSourceConfig {
    /// HTTP URL to fetch data from
    pub url: String,
    /// JSON path to extract value (dot notation, e.g. "data.price" or "1245620.data.price_overview.final")
    pub json_path: String,
    /// Type of value to extract: "number", "string", "boolean" (default: "number")
    #[serde(default = "default_value_type")]
    pub value_type: String,
    /// HTTP method: "GET" or "POST" (default: "GET")
    #[serde(default = "default_http_method")]
    pub method: String,
    /// Optional HTTP headers as key-value pairs
    #[serde(default)]
    pub headers: Vec<(String, String)>,
}

fn default_value_type() -> String {
    "number".to_string()
}

fn default_http_method() -> String {
    "GET".to_string()
}

/// Request for custom data (any external source: prices, weather, game data, etc.)
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct CustomDataRequest {
    /// Identifier for the data in callback (e.g., "steam:elden_ring", "weather:nyc")
    pub id: String,
    /// Token/query identifier for the source (required for coingecko/binance/pyth, optional for custom)
    #[serde(default)]
    pub token_id: String,
    /// Data source to query
    pub source: PriceSource,
}

impl std::fmt::Display for PriceSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PriceSource::CoinGecko => write!(f, "coingecko"),
            PriceSource::Binance => write!(f, "binance"),
            PriceSource::Pyth => write!(f, "pyth"),
            PriceSource::Custom(_) => write!(f, "custom"),
        }
    }
}

/// Response from WASI oracle
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct WasiPriceResponse {
    pub success: bool,
    pub prices: Vec<WasiPriceResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct WasiPriceResult {
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

/// WASI response for custom_call
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct WasiCustomDataResponse {
    pub success: bool,
    pub results: Vec<WasiCustomDataResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct WasiCustomDataResult {
    /// ID from the request
    pub id: String,
    /// Value as JSON (number, string, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
#[borsh(crate = "near_sdk::borsh")]
pub struct Contract {
    pub oracles: UnorderedMap<AccountId, VOracle>,

    pub assets: UnorderedMap<AssetId, VAsset>,

    pub recency_duration_sec: DurationSec,

    pub owner_id: AccountId,

    pub near_claim_amount: u128,

    /// OutLayer contract ID for WASI execution
    pub outlayer_contract_id: Option<AccountId>,

    /// OutLayer project code source (repo URL, commit, etc.)
    pub outlayer_code_source: Option<String>,

    /// If true and contract balance > MIN_BALANCE_FOR_SUBSIDY, contract pays for OutLayer calls
    pub subsidize_outlayer_calls: bool,

    /// OutLayer secrets profile name (e.g., "default")
    pub secrets_profile: Option<String>,

    /// OutLayer secrets account ID (e.g., "zavodil2.testnet")
    pub secrets_account_id: Option<AccountId>,
}

#[derive(Serialize, Deserialize, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct PriceData {
    #[serde(with = "u64_dec_format")]
    #[schemars(with = "String")]
    pub timestamp: Timestamp,
    pub recency_duration_sec: DurationSec,

    pub prices: Vec<AssetOptionalPrice>,
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(
        recency_duration_sec: DurationSec,
        owner_id: AccountId,
        near_claim_amount: U128,
    ) -> Self {
        Self {
            oracles: UnorderedMap::new(StorageKey::Oracles),
            assets: UnorderedMap::new(StorageKey::Assets),
            recency_duration_sec,
            owner_id,
            near_claim_amount: near_claim_amount.into(),
            outlayer_contract_id: None,
            outlayer_code_source: None,
            subsidize_outlayer_calls: false,
            secrets_profile: None,
            secrets_account_id: None,
        }
    }

    /// Configure OutLayer integration (owner only)
    ///
    /// # Arguments
    /// * `outlayer_contract_id` - OutLayer contract (e.g., "outlayer.near")
    /// * `code_source` - JSON string with ExecutionSource (Project, GitHub, or WasmUrl)
    /// * `secrets_profile` - Optional secrets profile name (e.g., "default")
    /// * `secrets_account_id` - Optional account ID for secrets (required with secrets_profile)
    #[payable]
    pub fn configure_outlayer(
        &mut self,
        outlayer_contract_id: AccountId,
        code_source: String,
        secrets_profile: Option<String>,
        secrets_account_id: Option<AccountId>,
    ) {
        self.assert_owner();
        self.outlayer_contract_id = Some(outlayer_contract_id);
        self.outlayer_code_source = Some(code_source);
        self.secrets_profile = secrets_profile;
        self.secrets_account_id = secrets_account_id;
        log!("OutLayer configured");
    }

    /// Remove price data from removed oracle.
    pub fn clean_oracle_data(&mut self, account_id: AccountId, asset_ids: Vec<AssetId>) {
        assert!(self.internal_get_oracle(&account_id).is_none());
        for asset_id in asset_ids {
            let mut asset = self.internal_get_asset(&asset_id).expect("Unknown asset");
            if asset.remove_report(&account_id) {
                self.internal_set_asset(&asset_id, asset);
            }
        }
    }

    pub fn get_oracle(&self, account_id: AccountId) -> Option<Oracle> {
        self.internal_get_oracle(&account_id)
    }

    pub fn get_oracles(
        &self,
        from_index: Option<u64>,
        limit: Option<u64>,
    ) -> Vec<(AccountId, Oracle)> {
        unordered_map_pagination(&self.oracles, from_index, limit)
    }

    pub fn get_assets(&self, from_index: Option<u64>, limit: Option<u64>) -> Vec<(AssetId, Asset)> {
        unordered_map_pagination(&self.assets, from_index, limit)
    }

    pub fn get_asset(&self, asset_id: AssetId) -> Option<Asset> {
        self.internal_get_asset(&asset_id)
    }

    /// Returns cached prices for whitelisted assets.
    /// If any asset has `price: null` — call `request_price_data` with a deposit
    /// to automatically fetch missing prices from OutLayer.
    pub fn get_price_data(&self, asset_ids: Option<Vec<AssetId>>) -> PriceData {
        let asset_ids = asset_ids.unwrap_or_else(|| self.assets.keys().collect());
        let timestamp = env::block_timestamp();
        let timestamp_cut = timestamp.saturating_sub(to_nano(self.recency_duration_sec));
        let min_num_recent_reports = std::cmp::max(1, (self.oracles.len() + 1) / 2) as usize;

        PriceData {
            timestamp,
            recency_duration_sec: self.recency_duration_sec,
            prices: asset_ids
                .into_iter()
                .map(|asset_id| {
                    // EMA for a specific asset, e.g. wrap.near#3600 is 1 hour EMA for wrap.near
                    if let Some((base_asset_id, period_sec)) = asset_id.split_once('#') {
                        let period_sec: DurationSec =
                            period_sec.parse().expect("Failed to parse EMA period");
                        let asset = self.internal_get_asset(&base_asset_id.to_string());
                        AssetOptionalPrice {
                            asset_id,
                            price: asset.and_then(|asset| {
                                asset
                                    .emas
                                    .into_iter()
                                    .find(|ema| ema.period_sec == period_sec)
                                    .filter(|ema| ema.timestamp >= timestamp_cut)
                                    .and_then(|ema| ema.price)
                            }),
                        }
                    } else {
                        let asset = self.internal_get_asset(&asset_id);
                        AssetOptionalPrice {
                            asset_id,
                            price: asset.and_then(|asset| {
                                asset.median_price(timestamp_cut, min_num_recent_reports)
                            }),
                        }
                    }
                })
                .collect(),
        }
    }

    /// Returns price data for a given oracle ID and given list of asset IDs.
    /// If recency_duration_sec is given, then it uses the given duration instead of the one from
    /// the contract config.
    pub fn get_oracle_price_data(
        &self,
        account_id: AccountId,
        asset_ids: Option<Vec<AssetId>>,
        recency_duration_sec: Option<DurationSec>,
    ) -> PriceData {
        let asset_ids = asset_ids.unwrap_or_else(|| self.assets.keys().collect());
        let timestamp = env::block_timestamp();
        let recency_duration_sec = recency_duration_sec.unwrap_or(self.recency_duration_sec);
        let timestamp_cut = timestamp.saturating_sub(to_nano(recency_duration_sec));

        let oracle_id: AccountId = account_id;
        PriceData {
            timestamp,
            recency_duration_sec,
            prices: asset_ids
                .into_iter()
                .map(|asset_id| {
                    let asset = self.internal_get_asset(&asset_id);
                    AssetOptionalPrice {
                        asset_id,
                        price: asset.and_then(|asset| {
                            asset
                                .reports
                                .into_iter()
                                .find(|report| report.oracle_id == oracle_id)
                                .filter(|report| report.timestamp >= timestamp_cut)
                                .map(|report| report.price)
                        }),
                    }
                })
                .collect(),
        }
    }

    pub fn report_prices(&mut self, prices: Vec<AssetPrice>, claim_near: Option<bool>) {
        assert!(!prices.is_empty());
        let oracle_id = env::predecessor_account_id();
        let timestamp = env::block_timestamp();

        // Oracle stats
        let mut oracle = self.internal_get_oracle(&oracle_id).expect("Not an oracle");
        oracle.last_report = timestamp;
        oracle.price_reports += prices.len() as u64;

        if claim_near.unwrap_or(false) && oracle.last_near_claim + NEAR_CLAIM_DURATION <= timestamp
        {
            let liquid_balance = env::account_balance().as_yoctonear()
                + env::account_locked_balance().as_yoctonear()
                - env::storage_byte_cost().as_yoctonear() * u128::from(env::storage_usage());
            if liquid_balance > self.near_claim_amount + SAFETY_MARGIN_NEAR_CLAIM {
                oracle.last_near_claim = timestamp;
                Promise::new(oracle_id.clone()).transfer(NearToken::from_yoctonear(self.near_claim_amount));
            }
        }

        self.internal_set_oracle(&oracle_id, oracle);

        // Updating prices
        for AssetPrice { asset_id, price } in prices {
            price.assert_valid();
            if let Some(mut asset) = self.internal_get_asset(&asset_id) {
                asset.remove_report(&oracle_id);
                asset.add_report(Report {
                    oracle_id: oracle_id.clone(),
                    timestamp,
                    price,
                });
                if !asset.emas.is_empty() {
                    let timestamp_cut =
                        timestamp.saturating_sub(to_nano(self.recency_duration_sec));
                    let min_num_recent_reports =
                        std::cmp::max(1, (self.oracles.len() + 1) / 2) as usize;
                    if let Some(median_price) =
                        asset.median_price(timestamp_cut, min_num_recent_reports)
                    {
                        for ema in asset.emas.iter_mut() {
                            ema.recompute(median_price, timestamp);
                        }
                    }
                }
                self.internal_set_asset(&asset_id, asset);
            } else {
                log!("Warning! Unknown asset ID: {}", asset_id);
            }
        }
    }

    /// Call oracle to get price data and invoke callback on receiver
    ///
    /// If prices are fresh in cache - returns immediately with callback
    /// If prices are stale and OutLayer is configured - calls WASI to fetch fresh prices
    ///
    /// # Arguments
    /// * `receiver_id` - Account to receive price data callback
    /// * `asset_ids` - List of assets to get prices for (None = all assets)
    /// * `msg` - Message to pass to receiver callback
    /// * `resource_limits` - Optional resource limits for OutLayer execution
    ///                       (default: 10B instructions, 128MB memory, 60s timeout)
    ///
    /// # Payment
    /// - If `subsidize_outlayer_calls` is enabled and contract has > 20 NEAR: no deposit required
    /// - Otherwise: minimum 0.01 NEAR if OutLayer fetch is needed (prices are stale)
    /// - Minimum 1 yoctoNEAR for immediate response (prices are fresh)
    #[payable]
    pub fn oracle_call(
        &mut self,
        receiver_id: AccountId,
        asset_ids: Option<Vec<AssetId>>,
        msg: String,
        resource_limits: Option<ResourceLimits>,
    ) -> Promise {
        let sender_id = env::predecessor_account_id();
        let attached = env::attached_deposit();

        // Get current price data
        let price_data = self.get_price_data(asset_ids.clone());

        // Check if all prices are available (fresh)
        let all_fresh = price_data.prices.iter().all(|p| p.price.is_some());

        if all_fresh {
            // Prices are fresh - return immediately
            let remaining_gas = env::prepaid_gas().saturating_sub(env::used_gas());
            assert!(remaining_gas >= GAS_FOR_PROMISE);

            // Refund deposit if caller attached NEAR but prices were fresh
            if attached.as_yoctonear() > 0 {
                Promise::new(sender_id.clone()).transfer(attached);
            }

            ext_price_receiver::ext(receiver_id)
                .with_attached_deposit(NO_DEPOSIT)
                .with_static_gas(remaining_gas.saturating_sub(GAS_FOR_PROMISE))
                .oracle_on_call(sender_id, price_data, msg)
        } else {
            let asset_ids_list: Vec<String> = asset_ids
                .clone()
                .unwrap_or_else(|| self.assets.keys().collect());
            let input_data = serde_json::json!({
                "command": "get_prices",
                "tokens": asset_ids_list,
                "max_age_secs": self.recency_duration_sec
            })
            .to_string();

            self.call_outlayer(attached, sender_id.clone(), resource_limits, input_data)
                .then(
                    ext_self::ext(env::current_account_id())
                        .with_static_gas(GAS_FOR_CALLBACK)
                        .on_outlayer_result(
                            sender_id,
                            receiver_id,
                            asset_ids.unwrap_or_default(),
                            msg,
                        ),
                )
        }
    }

    /// Request price data directly without external callback.
    ///
    /// If prices are fresh in cache — returns immediately as value.
    /// If prices are stale — calls OutLayer WASI to fetch fresh prices and returns them.
    ///
    /// # Arguments
    /// * `asset_ids` - List of assets to get prices for (None = all assets)
    /// * `resource_limits` - Optional resource limits for OutLayer execution
    ///
    /// # Payment
    /// - If `subsidize_outlayer_calls` is enabled and contract has > 20 NEAR: no deposit required
    /// - Otherwise: minimum 0.01 NEAR if OutLayer fetch is needed (prices are stale)
    #[payable]
    pub fn request_price_data(
        &mut self,
        asset_ids: Option<Vec<AssetId>>,
        resource_limits: Option<ResourceLimits>,
    ) -> PromiseOrValue<PriceData> {
        let sender_id = env::predecessor_account_id();
        let attached = env::attached_deposit();

        let price_data = self.get_price_data(asset_ids.clone());
        let all_fresh = price_data.prices.iter().all(|p| p.price.is_some());

        if all_fresh {
            // Refund deposit if caller attached NEAR but prices were fresh
            if attached.as_yoctonear() > 0 {
                Promise::new(sender_id).transfer(attached);
            }
            PromiseOrValue::Value(price_data)
        } else {
            let asset_ids_list: Vec<String> = asset_ids
                .clone()
                .unwrap_or_else(|| self.assets.keys().collect());
            let input_data = serde_json::json!({
                "command": "get_prices",
                "tokens": asset_ids_list,
                "max_age_secs": self.recency_duration_sec
            })
            .to_string();

            PromiseOrValue::Promise(
                self.call_outlayer(attached, sender_id, resource_limits, input_data)
                    .then(
                        ext_self::ext(env::current_account_id())
                            .with_static_gas(GAS_FOR_CALLBACK)
                            .on_request_price_result(asset_ids.unwrap_or_default()),
                    ),
            )
        }
    }

    /// Fetch custom data from external sources and invoke callback on receiver
    ///
    /// Use this for any external data: prices, weather, game data, API responses, etc.
    /// Data is fetched via OutLayer WASI and forwarded to your contract's `on_custom_data` callback.
    ///
    /// # Arguments
    /// * `receiver_id` - Contract to receive the callback
    /// * `custom_data_request` - List of data to fetch, each with id, token_id, and source
    /// * `msg` - Arbitrary message passed to callback
    /// * `resource_limits` - Optional OutLayer execution limits
    ///
    /// # Example
    /// ```ignore
    /// custom_call(
    ///     receiver_id: "myapp.near",
    ///     custom_data_request: [
    ///         { id: "gold", token_id: "gold", source: "coingecko" },
    ///         { id: "elden_ring", token_id: "1245620", source: { custom: { url: "...", json_path: "..." } } }
    ///     ],
    ///     msg: "my_action"
    /// )
    /// ```
    #[payable]
    pub fn custom_call(
        &mut self,
        receiver_id: AccountId,
        custom_data_request: Vec<CustomDataRequest>,
        msg: String,
        resource_limits: Option<ResourceLimits>,
    ) -> Promise {
        assert!(!custom_data_request.is_empty(), "custom_data_request cannot be empty");

        let sender_id = env::predecessor_account_id();
        let attached = env::attached_deposit();

        let input_data = serde_json::json!({
            "command": "fetch_custom_data",
            "requests": custom_data_request
        })
        .to_string();

        self.call_outlayer(attached, sender_id.clone(), resource_limits, input_data)
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .on_custom_call_result(sender_id, receiver_id, msg),
            )
    }

    /// Request custom data directly without external callback.
    ///
    /// Fetches data from external sources via OutLayer WASI and returns results directly.
    ///
    /// # Arguments
    /// * `custom_data_request` - List of data to fetch
    /// * `resource_limits` - Optional OutLayer execution limits
    ///
    /// # Payment
    /// - If `subsidize_outlayer_calls` is enabled and contract has > 20 NEAR: no deposit required
    /// - Otherwise: minimum 0.01 NEAR
    #[payable]
    pub fn request_custom_data(
        &mut self,
        custom_data_request: Vec<CustomDataRequest>,
        resource_limits: Option<ResourceLimits>,
    ) -> Promise {
        assert!(
            !custom_data_request.is_empty(),
            "custom_data_request cannot be empty"
        );

        let sender_id = env::predecessor_account_id();
        let attached = env::attached_deposit();

        let input_data = serde_json::json!({
            "command": "fetch_custom_data",
            "requests": custom_data_request
        })
        .to_string();

        self.call_outlayer(attached, sender_id, resource_limits, input_data)
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .on_request_custom_data_result(),
            )
    }

    /// Callback to handle custom_call result
    #[private]
    pub fn on_custom_call_result(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        msg: String,
        #[callback_result] result: Result<Option<WasiCustomDataResponse>, PromiseError>,
    ) -> Promise {
        match result {
            Ok(Some(wasi_response)) => {
                let results: Vec<CustomDataResult> = wasi_response
                    .results
                    .into_iter()
                    .map(|r| CustomDataResult {
                        id: r.id,
                        value: r.value,
                        timestamp: r.timestamp,
                        error: r.error,
                    })
                    .collect();

                if !wasi_response.success {
                    log!("WASI custom_call failed: {:?}", wasi_response.error);
                }

                ext_custom_receiver::ext(receiver_id)
                    .with_attached_deposit(NO_DEPOSIT)
                    .on_custom_data(sender_id, results, msg)
            }
            Ok(None) => {
                log!("WASI returned no data");
                ext_custom_receiver::ext(receiver_id)
                    .with_attached_deposit(NO_DEPOSIT)
                    .on_custom_data(sender_id, vec![], msg)
            }
            Err(e) => {
                log!("Promise error from OutLayer: {:?}", e);
                ext_custom_receiver::ext(receiver_id)
                    .with_attached_deposit(NO_DEPOSIT)
                    .on_custom_data(sender_id, vec![], msg)
            }
        }
    }

    /// Callback to handle OutLayer result
    #[private]
    pub fn on_outlayer_result(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        asset_ids: Vec<AssetId>,
        msg: String,
        #[callback_result] result: Result<Option<WasiPriceResponse>, PromiseError>,
    ) -> Promise {
        match result {
            Ok(Some(wasi_response)) => {
                if !wasi_response.success {
                    log!("WASI execution failed: {:?}", wasi_response.error);
                } else {
                    self.update_prices_from_wasi_response(&wasi_response);
                }

                let price_data = self.get_price_data(Some(asset_ids));
                ext_price_receiver::ext(receiver_id)
                    .with_attached_deposit(NO_DEPOSIT)
                    .oracle_on_call(sender_id, price_data, msg)
            }

            Ok(None) => {
                log!("OutLayer execution failed - received None");
                let price_data = self.get_price_data(Some(asset_ids));
                ext_price_receiver::ext(receiver_id)
                    .with_attached_deposit(NO_DEPOSIT)
                    .oracle_on_call(sender_id, price_data, msg)
            }

            Err(promise_error) => {
                log!("Promise error from OutLayer: {:?}", promise_error);
                let price_data = self.get_price_data(Some(asset_ids));
                ext_price_receiver::ext(receiver_id)
                    .with_attached_deposit(NO_DEPOSIT)
                    .oracle_on_call(sender_id, price_data, msg)
            }
        }
    }

    /// Callback for request_price_data — returns PriceData directly
    #[private]
    pub fn on_request_price_result(
        &mut self,
        asset_ids: Vec<AssetId>,
        #[callback_result] result: Result<Option<WasiPriceResponse>, PromiseError>,
    ) -> PriceData {
        match result {
            Ok(Some(wasi_response)) => {
                if !wasi_response.success {
                    log!("WASI execution failed: {:?}", wasi_response.error);
                } else {
                    self.update_prices_from_wasi_response(&wasi_response);
                }
                self.get_price_data(Some(asset_ids))
            }
            Ok(None) => {
                log!("OutLayer execution failed - received None");
                self.get_price_data(Some(asset_ids))
            }
            Err(promise_error) => {
                log!("Promise error from OutLayer: {:?}", promise_error);
                self.get_price_data(Some(asset_ids))
            }
        }
    }

    /// Callback for request_custom_data — returns Vec<CustomDataResult> directly
    #[private]
    pub fn on_request_custom_data_result(
        &mut self,
        #[callback_result] result: Result<Option<WasiCustomDataResponse>, PromiseError>,
    ) -> Vec<CustomDataResult> {
        match result {
            Ok(Some(wasi_response)) => {
                if !wasi_response.success {
                    log!("WASI custom data failed: {:?}", wasi_response.error);
                }
                wasi_response
                    .results
                    .into_iter()
                    .map(|r| CustomDataResult {
                        id: r.id,
                        value: r.value,
                        timestamp: r.timestamp,
                        error: r.error,
                    })
                    .collect()
            }
            Ok(None) => {
                log!("WASI returned no data");
                vec![]
            }
            Err(e) => {
                log!("Promise error from OutLayer: {:?}", e);
                vec![]
            }
        }
    }

    /// Resolve OutLayer config, check subsidy, determine deposit/payer.
    /// Returns a Promise for the OutLayer request_execution call.
    fn call_outlayer(
        &self,
        attached: NearToken,
        sender_id: AccountId,
        resource_limits: Option<ResourceLimits>,
        input_data: String,
    ) -> Promise {
        let outlayer_contract_id = self
            .outlayer_contract_id
            .clone()
            .expect("OutLayer not configured");
        let code_source_str = self
            .outlayer_code_source
            .clone()
            .expect("OutLayer code source not configured");

        let contract_balance = env::account_balance().as_yoctonear();
        let can_subsidize =
            self.subsidize_outlayer_calls && contract_balance > MIN_BALANCE_FOR_SUBSIDY;

        let (outlayer_deposit, payer_account_id): (NearToken, Option<AccountId>) =
            if can_subsidize {
                (
                    NearToken::from_yoctonear(SUBSIDIZED_OUTLAYER_DEPOSIT),
                    None,
                )
            } else {
                assert!(
                    attached.as_yoctonear() >= MIN_OUTLAYER_DEPOSIT,
                    "Requires at least 0.01 NEAR for OutLayer execution"
                );
                (attached, Some(sender_id))
            };

        let execution_source: ExecutionSource =
            serde_json::from_str(&code_source_str).expect("Invalid code source JSON");
        let limits = resource_limits.unwrap_or_default();
        let secrets_ref = self.build_secrets_ref();

        ext_outlayer::ext(outlayer_contract_id)
            .with_attached_deposit(outlayer_deposit)
            .with_unused_gas_weight(1)
            .request_execution(
                execution_source,
                Some(limits),
                Some(input_data),
                secrets_ref,
                Some(ResponseFormat::Json),
                payer_account_id,
                None,
            )
    }

    /// Update contract state with prices from WASI response.
    /// Used by both on_outlayer_result and on_request_price_result.
    fn update_prices_from_wasi_response(&mut self, wasi_response: &WasiPriceResponse) {
        let timestamp = env::block_timestamp();
        let oracle_id = env::current_account_id();

        for price_result in &wasi_response.prices {
            if let Some(price_f64) = price_result.price {
                let multiplier = (price_f64 * 100_000_000.0) as u128;
                let price = Price {
                    multiplier,
                    decimals: 8,
                };

                if let Some(mut asset) = self.internal_get_asset(&price_result.token) {
                    asset.remove_report(&oracle_id);
                    asset.add_report(Report {
                        oracle_id: oracle_id.clone(),
                        timestamp,
                        price,
                    });

                    if !asset.emas.is_empty() {
                        let timestamp_cut =
                            timestamp.saturating_sub(to_nano(self.recency_duration_sec));
                        let min_num_recent_reports =
                            std::cmp::max(1, (self.oracles.len() + 1) / 2) as usize;
                        if let Some(median_price) =
                            asset.median_price(timestamp_cut, min_num_recent_reports)
                        {
                            for ema in asset.emas.iter_mut() {
                                ema.recompute(median_price, timestamp);
                            }
                        }
                    }

                    self.internal_set_asset(&price_result.token, asset);
                }
            }
        }
    }

    /// Build secrets_ref JSON for OutLayer if both profile and account_id are configured
    fn build_secrets_ref(&self) -> Option<serde_json::Value> {
        match (&self.secrets_profile, &self.secrets_account_id) {
            (Some(profile), Some(account_id)) => Some(serde_json::json!({
                "profile": profile,
                "account_id": account_id
            })),
            _ => None,
        }
    }
}

