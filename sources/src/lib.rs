//! Shared price sources for oracle-ark
//!
//! This crate provides price fetching logic that works with both:
//! - WASI (sync, using wasi-http-client)
//! - Scheduler (async, using reqwest)
//!
//! # Features
//! - `wasi` - Enable sync WASI client support
//! - `async` - Enable async client support (for scheduler)

pub mod parsers;
pub mod security;
pub mod sources;

pub use parsers::*;

use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;

/// Last working Chainlink RPC index (shared across calls within a single WASI run)
pub static LAST_CHAINLINK_RPC: AtomicUsize = AtomicUsize::new(0);

/// If all Chainlink RPCs failed, disable further attempts for this run
pub static CHAINLINK_DISABLED: AtomicBool = AtomicBool::new(false);

/// Price result from a source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourcePrice {
    pub source_name: String,
    pub price: f64,
    pub timestamp: u64,
}

/// Per-asset exchange configuration.
/// Stored as opaque JSON in the oracle contract, parsed by WASI/scheduler.
/// Unknown fields are silently ignored (allows adding new exchanges without code changes).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExchangeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coingecko: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binance: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binance_us: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binance_alpha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pyth: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chainlink: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub huobi: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kucoin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cryptocom: Option<String>,
    /// Kraken pair in its CANONICAL spelling (`XXBTZUSD`, `XETHZUSD`, `NEARUSD`).
    /// Kraken answers with the canonical name whatever alias you ask for, so storing the
    /// canonical form here is what makes the batch lookup an exact match.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kraken: Option<String>,
    /// Coinbase Exchange product id (`NEAR-USD`) — quoted in real fiat USD
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coinbase: Option<String>,
    /// Bitstamp pair WITH the slash (`BTC/USD`), as the all-ticker endpoint reports it.
    /// The single-pair URL form (`btcusd`) is derived from this, not stored separately.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitstamp: Option<String>,
    /// OKX instrument id (`NEAR-USDT`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub okx: Option<String>,
    /// Bitget symbol (`NEARUSDT`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitget: Option<String>,
    /// MEXC symbol (`NEARUSDT`) — this endpoint quotes USDT pairs only
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mexc: Option<String>,
    #[serde(default)]
    pub stablecoin: bool,
    /// Token decimals (used by UI, not by price fetching)
    #[serde(default)]
    pub decimals: Option<u8>,
}

impl ExchangeConfig {
    /// Get Pyth ID with 0x prefix stripped
    pub fn pyth_id(&self) -> Option<&str> {
        self.pyth
            .as_deref()
            .map(|s| s.strip_prefix("0x").unwrap_or(s))
    }
}

/// Custom source configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomSourceConfig {
    pub url: String,
    pub json_path: String,
    #[serde(default = "default_value_type")]
    pub value_type: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
}

fn default_value_type() -> String {
    "number".to_string()
}

fn default_method() -> String {
    "GET".to_string()
}

/// HTTP response for parsing
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn json<T: for<'de> Deserialize<'de>>(&self) -> anyhow::Result<T> {
        Ok(serde_json::from_slice(&self.body)?)
    }
}
