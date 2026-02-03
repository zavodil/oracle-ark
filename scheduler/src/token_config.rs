//! Token configuration loaded from tokens.json
//!
//! This module provides dynamic token mappings for all price sources.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Configuration for a single token
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
}

/// All tokens configuration
#[derive(Debug, Clone)]
pub struct TokensConfig {
    tokens: HashMap<String, TokenConfig>,
}

impl TokensConfig {
    /// Load tokens from JSON file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read tokens file: {:?}", path.as_ref()))?;
        let tokens: HashMap<String, TokenConfig> = serde_json::from_str(&content)
            .with_context(|| "Failed to parse tokens.json")?;
        Ok(Self { tokens })
    }

    /// Get all token IDs
    pub fn token_ids(&self) -> Vec<String> {
        self.tokens.keys().cloned().collect()
    }

    /// Get config for a specific token
    pub fn get(&self, token_id: &str) -> Option<&TokenConfig> {
        self.tokens.get(token_id)
    }

    /// Get CoinGecko ID for token
    pub fn coingecko_id(&self, token_id: &str) -> Option<&str> {
        self.tokens.get(token_id)?.coingecko.as_deref()
    }

    /// Get Binance symbol for token
    pub fn binance_symbol(&self, token_id: &str) -> Option<&str> {
        self.tokens.get(token_id)?.binance.as_deref()
    }

    /// Get Binance US symbol for token
    pub fn binance_us_symbol(&self, token_id: &str) -> Option<&str> {
        self.tokens.get(token_id)?.binance_us.as_deref()
    }

    /// Get Binance Alpha contract address for token
    pub fn binance_alpha_address(&self, token_id: &str) -> Option<&str> {
        self.tokens.get(token_id)?.binance_alpha.as_deref()
    }

    /// Get Pyth price feed ID for token (with 0x prefix stripped)
    pub fn pyth_id(&self, token_id: &str) -> Option<&str> {
        self.tokens
            .get(token_id)?
            .pyth
            .as_deref()
            .map(|id| id.strip_prefix("0x").unwrap_or(id))
    }

    /// Get Huobi symbol for token
    pub fn huobi_symbol(&self, token_id: &str) -> Option<&str> {
        self.tokens.get(token_id)?.huobi.as_deref()
    }

    /// Get KuCoin symbol for token
    pub fn kucoin_symbol(&self, token_id: &str) -> Option<&str> {
        self.tokens.get(token_id)?.kucoin.as_deref()
    }

    /// Get Gate.io pair for token
    pub fn gate_pair(&self, token_id: &str) -> Option<&str> {
        self.tokens.get(token_id)?.gate.as_deref()
    }

    /// Get Crypto.com instrument for token
    pub fn cryptocom_instrument(&self, token_id: &str) -> Option<&str> {
        self.tokens.get(token_id)?.cryptocom.as_deref()
    }

    /// Check if token is a stablecoin
    pub fn is_stablecoin(&self, token_id: &str) -> bool {
        self.tokens
            .get(token_id)
            .map(|t| t.stablecoin)
            .unwrap_or(false)
    }
}
