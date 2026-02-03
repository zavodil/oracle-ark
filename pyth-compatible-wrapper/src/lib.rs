use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    env, ext_contract, log, near_bindgen, require, AccountId, BorshStorageKey, NearToken,
    PanicOnDefault, Promise,
};
use std::collections::HashMap;

/// Deposit that this contract attaches to oracle_call to cover OutLayer execution (0.02 NEAR).
const ORACLE_CALL_DEPOSIT: NearToken = NearToken::from_millinear(20);

// =============================================================================
// Oracle-Ark types (from price-oracle.near)
// =============================================================================

/// Oracle-Ark price format: multiplier * 10^(-decimals) = USD price.
/// Example: Price { multiplier: 450_000_000, decimals: 8 } = $4.50
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct OracleArkPrice {
    #[serde(with = "u128_dec_format")]
    #[schemars(with = "String")]
    pub multiplier: u128,
    pub decimals: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct AssetOptionalPrice {
    pub asset_id: String,
    pub price: Option<OracleArkPrice>,
}

/// Price data returned by Oracle-Ark's oracle_call callback.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct PriceData {
    /// Block timestamp in nanoseconds.
    #[serde(with = "u64_dec_format")]
    #[schemars(with = "String")]
    pub timestamp: u64,
    pub recency_duration_sec: u32,
    pub prices: Vec<AssetOptionalPrice>,
}

// =============================================================================
// Pyth-compatible types (matching pyth-oracle.near API)
// =============================================================================

/// Pyth Price — exact match of pyth-sdk Price type.
/// Actual USD price = price * 10^expo.
/// Example: BTC at $67,123.45 ± $12.50:
///   Price { price: 6712345, conf: 1250, expo: -2, publish_time: 1706900000 }
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct Price {
    /// Price value (signed).
    pub price: i64,
    /// Confidence interval (uncertainty). Always 0 for Oracle-Ark (single aggregated price).
    pub conf: u64,
    /// Exponent: actual_price = price * 10^expo.
    pub expo: i32,
    /// Unix timestamp (seconds) of last publish.
    pub publish_time: i64,
}

// =============================================================================
// Cross-contract interface to Oracle-Ark
// =============================================================================

#[ext_contract(ext_oracle)]
#[allow(dead_code)]
trait Oracle {
    fn oracle_call(
        &mut self,
        receiver_id: AccountId,
        asset_ids: Option<Vec<String>>,
        msg: String,
        resource_limits: Option<serde_json::Value>,
    );
}

// =============================================================================
// Borsh-serializable cached price for on-chain storage
// =============================================================================

#[derive(BorshSerialize, BorshDeserialize)]
#[borsh(crate = "near_sdk::borsh")]
pub struct CachedPrice {
    pub price: i64,
    pub conf: u64,
    pub expo: i32,
    pub publish_time: i64,
}

impl CachedPrice {
    fn to_pyth_price(&self) -> Price {
        Price {
            price: self.price,
            conf: self.conf,
            expo: self.expo,
            publish_time: self.publish_time,
        }
    }
}

// =============================================================================
// Storage keys
// =============================================================================

#[derive(BorshSerialize, BorshStorageKey)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    PriceIdToAsset,
    AssetToPriceId,
    CachedPrices,
}

// =============================================================================
// Contract state
// =============================================================================

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
#[borsh(crate = "near_sdk::borsh")]
pub struct PythWrapper {
    /// Oracle-Ark contract to read prices from (e.g. price-oracle.near).
    oracle_contract_id: AccountId,

    /// Mapping: Pyth PriceIdentifier (64-char hex) -> Oracle-Ark asset_id.
    /// Example: "c415de8d..." -> "wrap.near"
    price_id_to_asset: UnorderedMap<String, String>,

    /// Reverse mapping: Oracle-Ark asset_id -> Pyth PriceIdentifier (hex).
    asset_to_price_id: UnorderedMap<String, String>,

    /// Cached prices in Pyth format, keyed by PriceIdentifier hex.
    cached_prices: UnorderedMap<String, CachedPrice>,

    /// Staleness threshold in seconds (default ~60s, matches Pyth behavior).
    stale_threshold: u64,

    /// Contract owner who can manage price mappings.
    owner_id: AccountId,
}

// =============================================================================
// Contract implementation
// =============================================================================

#[near_bindgen]
impl PythWrapper {
    /// Initialize the wrapper contract.
    ///
    /// * `oracle_contract_id` — Oracle-Ark contract (e.g. "price-oracle.near")
    /// * `stale_threshold` — Max age of prices in seconds before they're considered stale
    #[init]
    pub fn new(oracle_contract_id: AccountId, stale_threshold: u64) -> Self {
        let mut contract = Self {
            oracle_contract_id,
            price_id_to_asset: UnorderedMap::new(StorageKey::PriceIdToAsset),
            asset_to_price_id: UnorderedMap::new(StorageKey::AssetToPriceId),
            cached_prices: UnorderedMap::new(StorageKey::CachedPrices),
            stale_threshold,
            owner_id: env::predecessor_account_id(),
        };

        // Pre-populate default mainnet Pyth price_id -> Oracle-Ark asset_id mappings.
        let defaults: &[(&str, &str)] = &[
            // NEAR/USD
            ("c415de8d2efa7db216527dff4b60e8f3a5311c740dadb233e13e12547e226750", "wrap.near"),
            // ETH/USD
            ("ff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace", "aurora"),
            // BTC/USD
            ("e62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43", "nbtc.bridge.near"),
            // USDT/USD
            ("2b89b9dc8fdf9f34709a5b106b472f0f39bb6ca9ce04b0fd7f2e971688e2e53b", "usdt.tether-token.near"),
            // USDC/USD
            ("eaa020c61cc479712813461ce153894a96a6c00b21ed0cfc2798d1f9a9e9c94a", "17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1"),
        ];
        for &(price_id, asset_id) in defaults {
            contract.price_id_to_asset.insert(&price_id.to_string(), &asset_id.to_string());
            contract.asset_to_price_id.insert(&asset_id.to_string(), &price_id.to_string());
        }

        contract
    }

    // =========================================================================
    // Admin methods (owner only, require 1 yoctoNEAR deposit)
    // =========================================================================

    /// Add a mapping between a Pyth PriceIdentifier and an Oracle-Ark asset_id.
    /// Requires exactly 1 yoctoNEAR deposit for access key security.
    #[payable]
    pub fn add_price_mapping(&mut self, price_id_hex: String, asset_id: String) {
        self.assert_owner();
        near_sdk::assert_one_yocto();
        require!(
            price_id_hex.len() == 64,
            "price_id_hex must be 64 hex characters"
        );
        require!(
            price_id_hex.chars().all(|c| c.is_ascii_hexdigit()),
            "price_id_hex must contain only hex characters"
        );
        self.price_id_to_asset.insert(&price_id_hex, &asset_id);
        self.asset_to_price_id.insert(&asset_id, &price_id_hex);
        log!("Added price mapping: {} -> {}", price_id_hex, asset_id);
    }

    /// Remove a price mapping by Pyth PriceIdentifier.
    #[payable]
    pub fn remove_price_mapping(&mut self, price_id_hex: String) {
        self.assert_owner();
        near_sdk::assert_one_yocto();
        if let Some(asset_id) = self.price_id_to_asset.remove(&price_id_hex) {
            self.asset_to_price_id.remove(&asset_id);
            self.cached_prices.remove(&price_id_hex);
            log!("Removed price mapping: {} -> {}", price_id_hex, asset_id);
        }
    }

    /// Set the staleness threshold in seconds.
    #[payable]
    pub fn set_stale_threshold(&mut self, threshold_sec: u64) {
        self.assert_owner();
        near_sdk::assert_one_yocto();
        self.stale_threshold = threshold_sec;
        log!("Stale threshold set to {} seconds", threshold_sec);
    }

    /// Set the Oracle-Ark contract ID.
    #[payable]
    pub fn set_oracle_contract_id(&mut self, contract_id: AccountId) {
        self.assert_owner();
        near_sdk::assert_one_yocto();
        self.oracle_contract_id = contract_id.clone();
        log!("Oracle contract ID set to {}", contract_id);
    }

    // =========================================================================
    // Pyth-compatible view methods
    // =========================================================================

    /// Get the latest price for a feed. Returns None if the price is stale
    /// (older than stale_threshold seconds).
    pub fn get_price(&self, price_identifier: String) -> Option<Price> {
        self.internal_get_price_no_older_than(&price_identifier, self.stale_threshold)
    }

    /// Get the latest price WITHOUT staleness check. May return very old data.
    pub fn get_price_unsafe(&self, price_identifier: String) -> Option<Price> {
        self.cached_prices
            .get(&price_identifier)
            .map(|cp| cp.to_pyth_price())
    }

    /// Get the latest price only if published within `age` seconds.
    pub fn get_price_no_older_than(&self, price_id: String, age: u64) -> Option<Price> {
        self.internal_get_price_no_older_than(&price_id, age)
    }

    /// Get EMA price with staleness check.
    /// Oracle-Ark does not provide separate EMA data, returns same as get_price.
    pub fn get_ema_price(&self, price_id: String) -> Option<Price> {
        self.internal_get_price_no_older_than(&price_id, self.stale_threshold)
    }

    /// Get EMA price without staleness check.
    pub fn get_ema_price_unsafe(&self, price_id: String) -> Option<Price> {
        self.cached_prices
            .get(&price_id)
            .map(|cp| cp.to_pyth_price())
    }

    /// Get EMA price only if published within `age` seconds.
    pub fn get_ema_price_no_older_than(&self, price_id: String, age: u64) -> Option<Price> {
        self.internal_get_price_no_older_than(&price_id, age)
    }

    /// Check if a price feed exists (has a mapping configured).
    pub fn price_feed_exists(&self, price_identifier: String) -> bool {
        self.price_id_to_asset.get(&price_identifier).is_some()
    }

    /// Get the staleness threshold in seconds.
    pub fn get_stale_threshold(&self) -> u64 {
        self.stale_threshold
    }

    /// Batch: get prices for multiple feeds with staleness check.
    pub fn list_prices(&self, price_ids: Vec<String>) -> HashMap<String, Option<Price>> {
        price_ids
            .into_iter()
            .map(|id| {
                let price = self.internal_get_price_no_older_than(&id, self.stale_threshold);
                (id, price)
            })
            .collect()
    }

    /// Batch: get prices without staleness check.
    pub fn list_prices_unsafe(&self, price_ids: Vec<String>) -> HashMap<String, Option<Price>> {
        price_ids
            .into_iter()
            .map(|id| {
                let price = self
                    .cached_prices
                    .get(&id)
                    .map(|cp| cp.to_pyth_price());
                (id, price)
            })
            .collect()
    }

    /// Batch: get prices no older than stale_threshold.
    pub fn list_prices_no_older_than(
        &self,
        price_ids: Vec<String>,
    ) -> HashMap<String, Option<Price>> {
        price_ids
            .into_iter()
            .map(|id| {
                let price = self.internal_get_price_no_older_than(&id, self.stale_threshold);
                (id, price)
            })
            .collect()
    }

    // =========================================================================
    // Pyth-compatible mutating methods
    // =========================================================================

    /// Update price feeds. In real Pyth this accepts Wormhole VAA data.
    /// In this wrapper, we ignore the data and trigger a price refresh from Oracle-Ark.
    /// Protocols that call update_price_feeds before get_price will still work.
    #[payable]
    pub fn update_price_feeds(&mut self, _data: String) {
        let asset_ids: Vec<String> = self.price_id_to_asset.iter().map(|(_, v)| v).collect();
        if asset_ids.is_empty() {
            return;
        }
        ext_oracle::ext(self.oracle_contract_id.clone())
            .with_attached_deposit(ORACLE_CALL_DEPOSIT)
            .with_unused_gas_weight(1)
            .oracle_call(
                env::current_account_id(),
                Some(asset_ids),
                String::new(),
                None,
            );
    }

    /// Estimate fee for update_price_feeds.
    /// Returns 1 yoctoNEAR — Oracle-Ark prices are already on-chain, no expensive update needed.
    pub fn get_update_fee_estimate(&self, _data: String) -> U128 {
        U128(1)
    }

    // =========================================================================
    // Oracle-Ark callback
    // =========================================================================

    /// Callback from Oracle-Ark contract with fresh price data.
    /// Called automatically after oracle_call resolves.
    #[allow(unused_variables)]
    pub fn oracle_on_call(
        &mut self,
        sender_id: AccountId,
        data: PriceData,
        msg: String,
    ) {
        require!(
            env::predecessor_account_id() == self.oracle_contract_id,
            "Callback only from oracle contract"
        );

        // Oracle-Ark timestamp is in nanoseconds, Pyth uses seconds.
        let timestamp_sec = (data.timestamp / 1_000_000_000) as i64;
        let mut updated_count = 0u32;

        for asset_price in &data.prices {
            if let Some(ref oracle_price) = asset_price.price {
                if let Some(price_id) = self.asset_to_price_id.get(&asset_price.asset_id) {
                    let cached = CachedPrice {
                        price: oracle_price.multiplier as i64,
                        conf: 0,
                        expo: -(oracle_price.decimals as i32),
                        publish_time: timestamp_sec,
                    };
                    self.cached_prices.insert(&price_id, &cached);
                    updated_count += 1;
                }
            }
        }

        log!(
            "Pyth wrapper: updated {} prices (timestamp: {})",
            updated_count,
            timestamp_sec
        );
    }

    // =========================================================================
    // Refresh prices from Oracle-Ark
    // =========================================================================

    /// Trigger a price refresh from Oracle-Ark.
    /// Anyone can call this. The contract pays 0.02 NEAR from its own balance for the oracle call.
    #[payable]
    pub fn refresh_prices(&mut self) -> Promise {
        let asset_ids: Vec<String> = self.price_id_to_asset.iter().map(|(_, v)| v).collect();
        require!(!asset_ids.is_empty(), "No price mappings configured");

        ext_oracle::ext(self.oracle_contract_id.clone())
            .with_attached_deposit(ORACLE_CALL_DEPOSIT)
            .with_unused_gas_weight(1)
            .oracle_call(
                env::current_account_id(),
                Some(asset_ids),
                String::new(),
                None,
            )
    }

    // =========================================================================
    // Introspection view methods
    // =========================================================================

    /// Get the Oracle-Ark contract ID.
    pub fn get_oracle_contract_id(&self) -> AccountId {
        self.oracle_contract_id.clone()
    }

    /// Get the contract owner.
    pub fn get_owner(&self) -> AccountId {
        self.owner_id.clone()
    }

    /// Look up the Oracle-Ark asset_id for a given Pyth price_id.
    pub fn get_price_mapping(&self, price_id_hex: String) -> Option<String> {
        self.price_id_to_asset.get(&price_id_hex)
    }

    /// Get all configured price_id -> asset_id mappings.
    pub fn get_all_mappings(&self) -> Vec<(String, String)> {
        self.price_id_to_asset.iter().collect()
    }

    // =========================================================================
    // Internal helpers
    // =========================================================================

    fn assert_owner(&self) {
        require!(
            env::predecessor_account_id() == self.owner_id,
            "Only owner can call this method"
        );
    }

    fn internal_get_price_no_older_than(
        &self,
        price_id: &str,
        max_age_sec: u64,
    ) -> Option<Price> {
        let cached = self.cached_prices.get(&price_id.to_string())?;
        let now_sec = env::block_timestamp() / 1_000_000_000;
        if now_sec.saturating_sub(cached.publish_time as u64) > max_age_sec {
            return None;
        }
        Some(cached.to_pyth_price())
    }
}

// =============================================================================
// Serde helpers for u128/u64 decimal string format (Oracle-Ark compatibility)
// =============================================================================

mod u128_dec_format {
    use near_sdk::serde::de;
    use near_sdk::serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(num: &u128, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&num.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u128, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

mod u64_dec_format {
    use near_sdk::serde::de;
    use near_sdk::serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(num: &u64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&num.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}
