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
pub mod sources;

pub use parsers::*;

use serde::{Deserialize, Serialize};

/// Price result from a source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourcePrice {
    pub source_name: String,
    pub price: f64,
    pub timestamp: u64,
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

/// Token mappings for different exchanges
/// Token IDs are NEAR token contract IDs or simple names
pub mod token_map {
    use std::collections::HashMap;
    use std::sync::LazyLock;

    /// Map NEAR token IDs to CoinGecko IDs
    pub static COINGECKO_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
        let mut m = HashMap::new();
        // Core tokens
        m.insert("wrap.near", "near");
        m.insert("usdt.tether-token.near", "tether");
        m.insert("17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1", "usd-coin"); // USDC
        m.insert("aurora", "ethereum"); // Aurora wraps ETH
        // Bridged tokens
        m.insert("nbtc.bridge.near", "bitcoin");
        m.insert("2260fac5e5542a773aa44fbcfedf7c193bc2c599.factory.bridge.near", "wrapped-bitcoin"); // WBTC
        m.insert("6b175474e89094c44da98b954eedeac495271d0f.factory.bridge.near", "dai");
        m.insert("aaaaaa20d9e0e2461697782ef11675f668207961.factory.bridge.near", "aurora-near"); // AURORA token
        m.insert("853d955acef822db058eb8505911ed77f175b99e.factory.bridge.near", "frax");
        m.insert("4691937a7508860f876c9c0a2a617e7d9e945d4b.factory.bridge.near", "woo-network");
        m.insert("22.contract.portalbridge.near", "solana");
        m.insert("zec.omft.near", "zcash");
        m
    });

    /// Map NEAR token IDs to Binance symbols
    pub static BINANCE_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
        let mut m = HashMap::new();
        m.insert("wrap.near", "NEARUSDT");
        m.insert("17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1", "USDCUSDT"); // USDC
        m.insert("aurora", "ETHUSDT"); // Aurora wraps ETH
        m.insert("nbtc.bridge.near", "BTCUSDT");
        m.insert("2260fac5e5542a773aa44fbcfedf7c193bc2c599.factory.bridge.near", "WBTCUSDT");
        m.insert("6b175474e89094c44da98b954eedeac495271d0f.factory.bridge.near", "DAIUSDT");
        m.insert("4691937a7508860f876c9c0a2a617e7d9e945d4b.factory.bridge.near", "WOOUSDT");
        m.insert("22.contract.portalbridge.near", "SOLUSDT");
        m.insert("zec.omft.near", "ZECUSDT");
        // Note: AURORA, FRAX not on Binance
        m
    });

    /// Map NEAR token IDs to Binance US symbols (same format as Binance)
    pub static BINANCE_US_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
        let mut m = HashMap::new();
        m.insert("wrap.near", "NEARUSD");
        m.insert("17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1", "USDCUSD"); // USDC
        m.insert("aurora", "ETHUSD"); // Aurora wraps ETH
        m.insert("nbtc.bridge.near", "BTCUSD");
        m.insert("22.contract.portalbridge.near", "SOLUSD");
        m.insert("6b175474e89094c44da98b954eedeac495271d0f.factory.bridge.near", "DAIUSD"); // DAI
        m.insert("zec.omft.near", "ZECUSD"); // ZEC
        // Note: Binance US has fewer pairs than global Binance
        m
    });

    /// Map NEAR token IDs to Binance Alpha contract addresses
    pub static BINANCE_ALPHA_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
        let mut m = HashMap::new();
        // Rhea token on BSC
        m.insert("token.rhealab.near", "0x4c067de26475e1cefee8b8d1f6e2266b33a2372e");
        m
    });

    /// Map NEAR token IDs to Pyth price feed IDs (without 0x prefix)
    pub static PYTH_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
        let mut m = HashMap::new();
        // From user's list - these are the actual Pyth feed IDs
        m.insert("wrap.near", "c415de8d2efa7db216527dff4b60e8f3a5311c740dadb233e13e12547e226750");
        m.insert("17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1", "eaa020c61cc479712813461ce153894a96a6c00b21ed0cfc2798d1f9a9e9c94a"); // USDC
        m.insert("nbtc.bridge.near", "e62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43"); // BTC
        m.insert("2260fac5e5542a773aa44fbcfedf7c193bc2c599.factory.bridge.near", "c9d8b075a5c69303365ae23633d4e085199bf5c520a3b90fed1322a0342ffc33"); // WBTC
        m.insert("aurora", "ff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace"); // ETH
        m.insert("usdt.tether-token.near", "2b89b9dc8fdf9f34709a5b106b472f0f39bb6ca9ce04b0fd7f2e971688e2e53b");
        m.insert("6b175474e89094c44da98b954eedeac495271d0f.factory.bridge.near", "b0948a5e5313200c632b51bb5ca32f6de0d36e9950a942d19751e833f70dabfd"); // DAI
        m.insert("aaaaaa20d9e0e2461697782ef11675f668207961.factory.bridge.near", "2f7c4f738d498585065a4b87b637069ec99474597da7f0ca349ba8ac3ba9cac5"); // AURORA
        m.insert("853d955acef822db058eb8505911ed77f175b99e.factory.bridge.near", "7c53208632935ba5122c3cf65a0f4b3e72ba4955b49ad6ba0acf3d9ba405aef3"); // FRAX
        m.insert("4691937a7508860f876c9c0a2a617e7d9e945d4b.factory.bridge.near", "b82449fd728133488d2d41131cffe763f9c1693b73c544d9ef6aaa371060dd25"); // WOO
        m.insert("22.contract.portalbridge.near", "ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d"); // SOL
        m.insert("zec.omft.near", "be9b59d178f0d6a97ab4c343bff2aa69caa1eaae3e9048a65788c529b125bb24"); // ZEC
        m.insert("token.rhealab.near", "ded2a0d2624278a32c56725397cc98b24ddb83d8c4d2ce108b1fc44b1d8de22b"); // rhea (Pyth only)
        // m.insert("YU", "f42978c0e26f9f3148e4b43f62891475dde489e145ea5248749cd007dcc35fb6"); // YU - unknown token
        m
    });

    /// Map to Huobi symbols (lowercase, no separator)
    pub static HUOBI_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
        let mut m = HashMap::new();
        m.insert("wrap.near", "nearusdt");
        m.insert("nbtc.bridge.near", "btcusdt");
        m.insert("aurora", "ethusdt");
        m.insert("22.contract.portalbridge.near", "solusdt");
        m.insert("zec.omft.near", "zecusdt");
        m
    });

    /// Map to KuCoin symbols
    pub static KUCOIN_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
        let mut m = HashMap::new();
        m.insert("wrap.near", "NEAR-USDT");
        m.insert("nbtc.bridge.near", "BTC-USDT");
        m.insert("aurora", "ETH-USDT");
        m.insert("6b175474e89094c44da98b954eedeac495271d0f.factory.bridge.near", "DAI-USDT");
        m.insert("22.contract.portalbridge.near", "SOL-USDT");
        m.insert("zec.omft.near", "ZEC-USDT");
        m.insert("4691937a7508860f876c9c0a2a617e7d9e945d4b.factory.bridge.near", "WOO-USDT");
        m
    });

    /// Map to Gate.io pairs
    pub static GATE_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
        let mut m = HashMap::new();
        m.insert("wrap.near", "near_usdt");
        m.insert("nbtc.bridge.near", "btc_usdt");
        m.insert("aurora", "eth_usdt");
        m.insert("22.contract.portalbridge.near", "sol_usdt");
        m.insert("zec.omft.near", "zec_usdt");
        m.insert("aaaaaa20d9e0e2461697782ef11675f668207961.factory.bridge.near", "aurora_usdt");
        m
    });

    /// Map to Crypto.com instruments
    pub static CRYPTOCOM_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
        let mut m = HashMap::new();
        m.insert("wrap.near", "NEAR_USDT");
        m.insert("nbtc.bridge.near", "BTC_USDT");
        m.insert("aurora", "ETH_USDT");
        m.insert("22.contract.portalbridge.near", "SOL_USDT");
        m
    });

    pub fn get_coingecko_id(token: &str) -> Option<&'static str> {
        COINGECKO_MAP.get(token).copied()
    }

    pub fn get_binance_symbol(token: &str) -> Option<&'static str> {
        BINANCE_MAP.get(token).copied()
    }

    pub fn get_binance_us_symbol(token: &str) -> Option<&'static str> {
        BINANCE_US_MAP.get(token).copied()
    }

    pub fn get_binance_alpha_address(token: &str) -> Option<&'static str> {
        BINANCE_ALPHA_MAP.get(token).copied()
    }

    pub fn get_pyth_id(token: &str) -> Option<&'static str> {
        PYTH_MAP.get(token).copied()
    }

    pub fn get_huobi_symbol(token: &str) -> Option<&'static str> {
        HUOBI_MAP.get(token).copied()
    }

    pub fn get_kucoin_symbol(token: &str) -> Option<&'static str> {
        KUCOIN_MAP.get(token).copied()
    }

    pub fn get_gate_pair(token: &str) -> Option<&'static str> {
        GATE_MAP.get(token).copied()
    }

    pub fn get_cryptocom_instrument(token: &str) -> Option<&'static str> {
        CRYPTOCOM_MAP.get(token).copied()
    }
}
