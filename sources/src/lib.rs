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

    /// The venues this asset has configured, by the names used everywhere else.
    ///
    /// Lives next to the struct so there is exactly one mapping from a source name onto a
    /// field. The worker's exclusion filter and the scheduler's tier selection both consume it;
    /// a venue added to the struct but forgotten here would look unconfigured to both.
    pub fn configured_sources(&self) -> Vec<&'static str> {
        let mut sources = Vec::new();
        if self.coingecko.is_some() {
            sources.push("coingecko");
        }
        if self.binance.is_some() {
            sources.push("binance");
        }
        if self.binance_us.is_some() {
            sources.push("binance_us");
        }
        if self.binance_alpha.is_some() {
            sources.push("binance_alpha");
        }
        if self.pyth.is_some() {
            sources.push("pyth");
        }
        if self.chainlink.is_some() {
            sources.push("chainlink");
        }
        if self.huobi.is_some() {
            sources.push("huobi");
        }
        if self.kucoin.is_some() {
            sources.push("kucoin");
        }
        if self.gate.is_some() {
            sources.push("gate");
        }
        if self.cryptocom.is_some() {
            sources.push("cryptocom");
        }
        if self.kraken.is_some() {
            sources.push("kraken");
        }
        if self.coinbase.is_some() {
            sources.push("coinbase");
        }
        if self.bitstamp.is_some() {
            sources.push("bitstamp");
        }
        if self.okx.is_some() {
            sources.push("okx");
        }
        if self.bitget.is_some() {
            sources.push("bitget");
        }
        if self.mexc.is_some() {
            sources.push("mexc");
        }
        sources
    }

    /// Whether this asset configures any of the named venues (case-insensitive).
    /// Used to decide which assets participate in a tier.
    pub fn configures_any(&self, names: &[String]) -> bool {
        self.configured_sources()
            .iter()
            .any(|source| names.iter().any(|n| n.eq_ignore_ascii_case(source)))
    }

    /// A copy with the named venues cleared.
    ///
    /// Fetching is driven purely by which fields are `Some`, so clearing a field removes that
    /// venue from the fetch. A name this does not recognise leaves the config untouched, which
    /// would silently keep a source the caller believes is gone — callers must validate names
    /// first (the worker rejects unknown ones outright).
    pub fn without_sources(&self, names: &[String]) -> Self {
        let mut filtered = self.clone();
        for name in names {
            match name.to_ascii_lowercase().as_str() {
                "coingecko" => filtered.coingecko = None,
                "binance" => filtered.binance = None,
                "binance_us" => filtered.binance_us = None,
                "binance_alpha" => filtered.binance_alpha = None,
                "pyth" => filtered.pyth = None,
                "chainlink" => filtered.chainlink = None,
                "huobi" => filtered.huobi = None,
                "kucoin" => filtered.kucoin = None,
                "gate" => filtered.gate = None,
                "cryptocom" => filtered.cryptocom = None,
                "kraken" => filtered.kraken = None,
                "coinbase" => filtered.coinbase = None,
                "bitstamp" => filtered.bitstamp = None,
                "okx" => filtered.okx = None,
                "bitget" => filtered.bitget = None,
                "mexc" => filtered.mexc = None,
                _ => {}
            }
        }
        filtered
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
