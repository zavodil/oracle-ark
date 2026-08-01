use crate::*;
use std::collections::HashMap;

/// Pyth-compatible price type.
/// Actual USD price = price * 10^expo.
///
/// Example: NEAR at $5.25
///   PythPrice { price: 525000000, conf: 0, expo: -8, publish_time: 1706900000 }
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct PythPrice {
    /// Price value (signed). Same as Oracle Example multiplier.
    pub price: i64,
    /// Confidence interval. Always 0 for Oracle Example (single aggregated price).
    pub conf: u64,
    /// Exponent: actual_price = price * 10^expo. Equals -(decimals).
    pub expo: i32,
    /// Unix timestamp (seconds) of the most recent price report.
    pub publish_time: i64,
}

#[near_bindgen]
impl Contract {
    // =========================================================================
    // Pyth-compatible view methods
    // =========================================================================

    /// Get the latest price for a feed. Returns None if the price is stale
    /// (older than `pyth_stale_threshold` seconds).
    /// Compatible with pyth-oracle.near `get_price`.
    pub fn get_price(&self, price_identifier: String) -> Option<PythPrice> {
        self.internal_get_pyth_price(&price_identifier, self.pyth_stale_threshold)
    }

    /// Get the latest price WITHOUT staleness check. May return very old data.
    /// Compatible with pyth-oracle.near `get_price_unsafe`.
    pub fn get_price_unsafe(&self, price_identifier: String) -> Option<PythPrice> {
        self.internal_get_pyth_price_no_stale_check(&price_identifier)
    }

    /// Get the latest price only if published within `age` seconds.
    /// Compatible with pyth-oracle.near `get_price_no_older_than`.
    pub fn get_price_no_older_than(&self, price_id: String, age: u64) -> Option<PythPrice> {
        self.internal_get_pyth_price(&price_id, age)
    }

    /// Get EMA price with staleness check.
    /// Uses the first configured EMA period for the asset, or falls back to median.
    /// Compatible with pyth-oracle.near `get_ema_price`.
    pub fn get_ema_price(&self, price_id: String) -> Option<PythPrice> {
        self.internal_get_pyth_ema_price(&price_id, self.pyth_stale_threshold)
    }

    /// Get EMA price without staleness check.
    pub fn get_ema_price_unsafe(&self, price_id: String) -> Option<PythPrice> {
        self.internal_get_pyth_ema_price_no_stale_check(&price_id)
    }

    /// Get EMA price only if published within `age` seconds.
    pub fn get_ema_price_no_older_than(&self, price_id: String, age: u64) -> Option<PythPrice> {
        self.internal_get_pyth_ema_price(&price_id, age)
    }

    /// Check if a price feed exists (has a mapping configured).
    pub fn price_feed_exists(&self, price_identifier: String) -> bool {
        self.pyth_price_id_to_asset
            .get(&price_identifier)
            .is_some()
    }

    /// Get the Pyth staleness threshold in seconds.
    pub fn get_stale_threshold(&self) -> u64 {
        self.pyth_stale_threshold
    }

    /// Estimate fee for update_price_feeds.
    /// Returns 1 yoctoNEAR — prices are computed on-chain, no expensive update needed.
    pub fn get_update_fee_estimate(&self, _data: String) -> U128 {
        U128(1)
    }

    /// Batch: get prices for multiple feeds with staleness check.
    pub fn list_prices(&self, price_ids: Vec<String>) -> HashMap<String, Option<PythPrice>> {
        price_ids
            .into_iter()
            .map(|id| {
                let price = self.internal_get_pyth_price(&id, self.pyth_stale_threshold);
                (id, price)
            })
            .collect()
    }

    /// Batch: get prices without staleness check.
    pub fn list_prices_unsafe(&self, price_ids: Vec<String>) -> HashMap<String, Option<PythPrice>> {
        price_ids
            .into_iter()
            .map(|id| {
                let price = self.internal_get_pyth_price_no_stale_check(&id);
                (id, price)
            })
            .collect()
    }

    /// Batch: get prices no older than stale_threshold.
    pub fn list_prices_no_older_than(
        &self,
        price_ids: Vec<String>,
    ) -> HashMap<String, Option<PythPrice>> {
        price_ids
            .into_iter()
            .map(|id| {
                let price = self.internal_get_pyth_price(&id, self.pyth_stale_threshold);
                (id, price)
            })
            .collect()
    }

    /// Look up the Oracle Example asset_id for a given Pyth price_id.
    pub fn get_price_mapping(&self, price_id_hex: String) -> Option<String> {
        self.pyth_price_id_to_asset.get(&price_id_hex)
    }

    /// Get all configured price_id -> asset_id mappings.
    pub fn get_all_price_mappings(&self) -> Vec<(String, String)> {
        self.pyth_price_id_to_asset.iter().collect()
    }

    // =========================================================================
    // Pyth admin methods (owner only)
    // =========================================================================

}

// =============================================================================
// Internal helpers
// =============================================================================

impl Contract {
    /// Get Pyth price for a price_id with max age check.
    /// Computes median price on-the-fly from asset reports.
    fn internal_get_pyth_price(
        &self,
        price_id: &str,
        max_age_sec: u64,
    ) -> Option<PythPrice> {
        let asset_id = self.pyth_price_id_to_asset.get(&price_id.to_string())?;
        let asset = self.internal_get_asset(&asset_id)?;

        let timestamp = env::block_timestamp();
        let timestamp_cut = timestamp.saturating_sub(to_nano(max_age_sec as u32));
        let min_reports = std::cmp::max(1, (self.oracles.len() + 1) / 2) as usize;

        let price = asset.median_price(timestamp_cut, min_reports)?;

        let latest_report_ts = asset
            .reports
            .iter()
            .filter(|r| r.timestamp >= timestamp_cut)
            .map(|r| r.timestamp)
            .max()
            .unwrap_or(timestamp);

        Some(PythPrice {
            price: price.multiplier as i64,
            conf: 0,
            expo: -(price.decimals as i32),
            publish_time: (latest_report_ts / 1_000_000_000) as i64,
        })
    }

    /// Get Pyth price without staleness check.
    fn internal_get_pyth_price_no_stale_check(&self, price_id: &str) -> Option<PythPrice> {
        let asset_id = self.pyth_price_id_to_asset.get(&price_id.to_string())?;
        let asset = self.internal_get_asset(&asset_id)?;

        // Use recency_duration_sec as the cutoff (contract-level config)
        let timestamp = env::block_timestamp();
        let timestamp_cut = timestamp.saturating_sub(to_nano(self.recency_duration_sec));
        let min_reports = std::cmp::max(1, (self.oracles.len() + 1) / 2) as usize;

        // Try with recency check first, then without any filter
        let price = asset
            .median_price(timestamp_cut, min_reports)
            .or_else(|| asset.median_price(0, 1))?;

        let latest_report_ts = asset
            .reports
            .iter()
            .map(|r| r.timestamp)
            .max()
            .unwrap_or(timestamp);

        Some(PythPrice {
            price: price.multiplier as i64,
            conf: 0,
            expo: -(price.decimals as i32),
            publish_time: (latest_report_ts / 1_000_000_000) as i64,
        })
    }

    /// Get EMA price in Pyth format with max age check.
    fn internal_get_pyth_ema_price(
        &self,
        price_id: &str,
        max_age_sec: u64,
    ) -> Option<PythPrice> {
        let asset_id = self.pyth_price_id_to_asset.get(&price_id.to_string())?;
        let asset = self.internal_get_asset(&asset_id)?;

        let timestamp = env::block_timestamp();
        let timestamp_cut = timestamp.saturating_sub(to_nano(max_age_sec as u32));

        // Find the first EMA that is fresh enough
        let ema = asset
            .emas
            .iter()
            .find(|e| e.timestamp >= timestamp_cut && e.price.is_some())?;

        let ema_price = ema.price.as_ref()?;

        Some(PythPrice {
            price: ema_price.multiplier as i64,
            conf: 0,
            expo: -(ema_price.decimals as i32),
            publish_time: (ema.timestamp / 1_000_000_000) as i64,
        })
    }

    /// Get EMA price without staleness check.
    fn internal_get_pyth_ema_price_no_stale_check(&self, price_id: &str) -> Option<PythPrice> {
        let asset_id = self.pyth_price_id_to_asset.get(&price_id.to_string())?;
        let asset = self.internal_get_asset(&asset_id)?;

        // Find the first EMA with a price
        let ema = asset.emas.iter().find(|e| e.price.is_some())?;
        let ema_price = ema.price.as_ref()?;

        Some(PythPrice {
            price: ema_price.multiplier as i64,
            conf: 0,
            expo: -(ema_price.decimals as i32),
            publish_time: (ema.timestamp / 1_000_000_000) as i64,
        })
    }
}
