//! Signed price feed (pull model) for external consumers.
//!
//! Consumers fetch the feed themselves and verify the Ed25519 signature over the exact
//! bytes we emit — off-chain today, on-chain later. Nothing here touches the chain, so a
//! pull costs us no gas.
//!
//! Signature domain per format:
//! - `json`  — the UTF-8 bytes of the `payload` string (payload is the signed message)
//! - `borsh` — the raw bytes of `base64_decode(payload)`
//!
//! A verifier MUST NOT re-serialize the payload before checking the signature: key order,
//! float formatting and whitespace would all change the bytes.

use crate::types::AggregationMethod;
use base64::Engine;
use borsh::BorshSerialize;
use oracle_ark_sources::{parsers, ExchangeConfig};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

/// Default exponent of the signed feed: real price = `price * 10^expo`
pub const DEFAULT_EXPO: i32 = -8;

/// Accepted exponent range (keeps `10^-expo` and the scaled value inside f64/i64 range)
const MIN_EXPO: i32 = -18;
const MAX_EXPO: i32 = 18;

/// 2^63 — the first f64 value that no longer fits in i64
const I64_OVERFLOW: f64 = 9_223_372_036_854_775_808.0;

/// Source names accepted in `exclude_sources` (must match `SourcePrice.source_name`).
///
/// This list, `filter_exchange_config` and `configured_sources` all enumerate the same set
/// and must stay in step — `every_source_is_filterable` fails the build if they drift.
pub const KNOWN_SOURCES: [&str; 16] = [
    "coingecko",
    "binance",
    "binance_us",
    "binance_alpha",
    "pyth",
    "chainlink",
    "huobi",
    "kucoin",
    "gate",
    "cryptocom",
    "kraken",
    "coinbase",
    "bitstamp",
    "okx",
    "bitget",
    "mexc",
];

/// Serialization format of the signed payload
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SigFormat {
    /// Deterministic JSON object, keys sorted (default)
    Json,
    /// Borsh-serialized `BTreeMap<String, PriceEntry>`, base64-encoded in the response
    Borsh,
}

impl SigFormat {
    /// Parse the requested format, defaulting to JSON when omitted
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.unwrap_or("json") {
            "json" => Ok(SigFormat::Json),
            "borsh" => Ok(SigFormat::Borsh),
            other => Err(format!(
                "Unsupported sig_format: '{}' (expected 'json' or 'borsh')",
                other
            )),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SigFormat::Json => "json",
            SigFormat::Borsh => "borsh",
        }
    }
}

/// One asset in the signed payload.
/// `price` is scaled by `10^-expo`; `publish_time` is the unix second at which the TEE
/// fetched and aggregated the sources behind this price.
#[derive(Debug, Clone, Copy, PartialEq, BorshSerialize)]
pub struct PriceEntry {
    pub price: i64,
    pub expo: i32,
    pub publish_time: i64,
}

/// JSON projection of `PriceEntry` — `price` is an i64 rendered as a JSON string,
/// `expo` and `publish_time` stay JSON numbers.
#[derive(Debug, Serialize)]
struct JsonPriceEntry {
    price: String,
    expo: i32,
    publish_time: i64,
}

/// Validate the requested exponent
pub fn validate_expo(expo: i32) -> Result<(), String> {
    if !(MIN_EXPO..=MAX_EXPO).contains(&expo) {
        return Err(format!(
            "expo {} out of range (expected {}..={})",
            expo, MIN_EXPO, MAX_EXPO
        ));
    }
    Ok(())
}

/// Scale a float price into the i64 carried by the payload: `round(price * 10^-expo)`.
/// Rejects anything that would silently lose the price (non-finite, non-positive,
/// overflowing i64, or underflowing to zero).
pub fn scale_price(price: f64, expo: i32) -> Result<i64, String> {
    if !price.is_finite() || price <= 0.0 {
        return Err(format!("invalid price {}", price));
    }
    let scaled = (price * 10f64.powi(-expo)).round();
    if !scaled.is_finite() || scaled.abs() >= I64_OVERFLOW {
        return Err(format!("price {} does not fit i64 at expo {}", price, expo));
    }
    if scaled == 0.0 {
        return Err(format!("price {} underflows to 0 at expo {}", price, expo));
    }
    Ok(scaled as i64)
}

/// Resolve the key an asset is published under: the alias when the caller supplied one,
/// our own asset_id otherwise.
pub fn client_key(asset_id: &str, aliases: Option<&HashMap<String, String>>) -> String {
    aliases
        .and_then(|map| map.get(asset_id))
        .cloned()
        .unwrap_or_else(|| asset_id.to_string())
}

/// Build the payload entries from `(asset_id, price, timestamp)` triples.
/// Keys are the client-facing keys (after alias remapping) and the `BTreeMap` gives the
/// deterministic ordering the signature depends on.
pub fn build_entries(
    priced: &[(String, f64, u64)],
    aliases: Option<&HashMap<String, String>>,
    expo: i32,
) -> Result<BTreeMap<String, PriceEntry>, String> {
    let mut entries: BTreeMap<String, PriceEntry> = BTreeMap::new();
    // client key -> asset_id it came from, so a colliding alias map is rejected instead of
    // silently publishing only one of the two assets
    let mut origins: BTreeMap<String, String> = BTreeMap::new();

    for (asset_id, price, timestamp) in priced {
        let key = client_key(asset_id, aliases);
        let entry = PriceEntry {
            price: scale_price(*price, expo).map_err(|e| format!("{}: {}", asset_id, e))?,
            expo,
            publish_time: i64::try_from(*timestamp)
                .map_err(|_| format!("{}: timestamp {} does not fit i64", asset_id, timestamp))?,
        };

        if let Some(previous) = origins.get(&key) {
            if previous != asset_id {
                return Err(format!(
                    "alias collision: '{}' and '{}' both map to client key '{}'",
                    previous, asset_id, key
                ));
            }
        }
        origins.insert(key.clone(), asset_id.clone());
        entries.insert(key, entry);
    }

    Ok(entries)
}

/// Encode the payload and return `(payload_string, bytes_to_sign)`.
/// For JSON the two are the same bytes; for borsh the payload is the base64 of the bytes.
pub fn encode_payload(
    entries: &BTreeMap<String, PriceEntry>,
    format: SigFormat,
) -> Result<(String, Vec<u8>), String> {
    match format {
        SigFormat::Json => {
            let json: BTreeMap<&str, JsonPriceEntry> = entries
                .iter()
                .map(|(key, entry)| {
                    (
                        key.as_str(),
                        JsonPriceEntry {
                            price: entry.price.to_string(),
                            expo: entry.expo,
                            publish_time: entry.publish_time,
                        },
                    )
                })
                .collect();
            let payload = serde_json::to_string(&json)
                .map_err(|e| format!("Failed to serialize payload: {}", e))?;
            let bytes = payload.as_bytes().to_vec();
            Ok((payload, bytes))
        }
        SigFormat::Borsh => {
            let bytes = borsh::to_vec(entries)
                .map_err(|e| format!("Failed to borsh-serialize payload: {}", e))?;
            let payload = base64::engine::general_purpose::STANDARD.encode(&bytes);
            Ok((payload, bytes))
        }
    }
}

/// Base64-encode the 64-byte Ed25519 signature for the response
pub fn encode_signature(signature: &[u8; 64]) -> String {
    base64::engine::general_purpose::STANDARD.encode(signature)
}

/// Validate and normalize `exclude_sources`.
/// Unknown names are rejected: silently ignoring a typo would leave a source the caller
/// believes is excluded still contributing to the signed price.
pub fn validate_exclusions(exclude: &[String]) -> Result<Vec<String>, String> {
    let mut normalized: Vec<String> = Vec::with_capacity(exclude.len());
    for source in exclude {
        let name = source.trim().to_ascii_lowercase();
        if !KNOWN_SOURCES.contains(&name.as_str()) {
            return Err(format!(
                "Unknown source in exclude_sources: '{}' (known: {})",
                source,
                KNOWN_SOURCES.join(", ")
            ));
        }
        if !normalized.contains(&name) {
            normalized.push(name);
        }
    }
    Ok(normalized)
}

/// Check whether a source name is in the (already normalized) exclusion list
pub fn is_excluded(source_name: &str, exclude: &[String]) -> bool {
    exclude.iter().any(|name| name.eq_ignore_ascii_case(source_name))
}

/// Return a copy of `config` with every excluded source cleared.
/// `fetch_all_sources` picks sources purely by which fields are `Some`, so clearing a
/// field removes that source from the fetch. Names are validated by `validate_exclusions`.
pub fn filter_exchange_config(config: &ExchangeConfig, exclude: &[String]) -> ExchangeConfig {
    let mut filtered = config.clone();
    for source in exclude {
        match source.to_ascii_lowercase().as_str() {
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
            // A source missing from the arms above would be silently NOT excluded: the
            // caller believes it was filtered out while it still feeds the signed price.
            // `every_source_is_filterable` exists to keep this arm unreachable.
            _ => {}
        }
    }
    filtered
}

/// List the sources a config still has configured (used for actionable error messages)
pub fn configured_sources(config: &ExchangeConfig) -> Vec<&'static str> {
    let mut sources = Vec::new();
    if config.coingecko.is_some() {
        sources.push("coingecko");
    }
    if config.binance.is_some() {
        sources.push("binance");
    }
    if config.binance_us.is_some() {
        sources.push("binance_us");
    }
    if config.binance_alpha.is_some() {
        sources.push("binance_alpha");
    }
    if config.pyth.is_some() {
        sources.push("pyth");
    }
    if config.chainlink.is_some() {
        sources.push("chainlink");
    }
    if config.huobi.is_some() {
        sources.push("huobi");
    }
    if config.kucoin.is_some() {
        sources.push("kucoin");
    }
    if config.gate.is_some() {
        sources.push("gate");
    }
    if config.cryptocom.is_some() {
        sources.push("cryptocom");
    }
    if config.kraken.is_some() {
        sources.push("kraken");
    }
    if config.coinbase.is_some() {
        sources.push("coinbase");
    }
    if config.bitstamp.is_some() {
        sources.push("bitstamp");
    }
    if config.okx.is_some() {
        sources.push("okx");
    }
    if config.bitget.is_some() {
        sources.push("bitget");
    }
    if config.mexc.is_some() {
        sources.push("mexc");
    }
    sources
}

/// Set every source field of an `ExchangeConfig` to `Some`, so a test can assert that a
/// source is actually removable rather than only that it is spelled correctly somewhere.
///
/// This is written as an exhaustive struct literal ON PURPOSE: adding a field to
/// `ExchangeConfig` breaks compilation here, which is the earliest possible warning that
/// `KNOWN_SOURCES` / `filter_exchange_config` / `configured_sources` need the new venue too.
#[cfg(test)]
fn all_sources_configured() -> ExchangeConfig {
    ExchangeConfig {
        coingecko: Some("near".to_string()),
        binance: Some("NEARUSDT".to_string()),
        binance_us: Some("NEARUSD".to_string()),
        binance_alpha: Some("0xabc".to_string()),
        pyth: Some("0xdef".to_string()),
        chainlink: Some("0x0123".to_string()),
        huobi: Some("nearusdt".to_string()),
        kucoin: Some("NEAR-USDT".to_string()),
        gate: Some("near_usdt".to_string()),
        cryptocom: Some("NEAR_USDT".to_string()),
        kraken: Some("NEARUSD".to_string()),
        coinbase: Some("NEAR-USD".to_string()),
        bitstamp: Some("NEAR/USD".to_string()),
        okx: Some("NEAR-USDT".to_string()),
        bitget: Some("NEARUSDT".to_string()),
        mexc: Some("NEARUSDT".to_string()),
        stablecoin: false,
        decimals: Some(24),
    }
}

/// Aggregate prices with the requested method (same math as `fetch_and_store_price`)
pub fn aggregate(prices: &mut [f64], method: AggregationMethod) -> f64 {
    match method {
        AggregationMethod::Average => parsers::average(prices),
        AggregationMethod::Median => parsers::median(prices),
        AggregationMethod::WeightedAverage => parsers::weighted_average(prices),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::near_tx;
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    fn priced(asset_id: &str, price: f64, timestamp: u64) -> (String, f64, u64) {
        (asset_id.to_string(), price, timestamp)
    }

    #[test]
    fn scales_price_to_i64_at_default_expo() {
        assert_eq!(scale_price(0.99920522, DEFAULT_EXPO).unwrap(), 99_920_522);
        assert_eq!(scale_price(1.0, DEFAULT_EXPO).unwrap(), 100_000_000);
        assert_eq!(scale_price(2.5, DEFAULT_EXPO).unwrap(), 250_000_000);
        assert_eq!(scale_price(65_432.1, DEFAULT_EXPO).unwrap(), 6_543_210_000_000);
        // rounds to nearest instead of truncating (truncation would give 199_999_999)
        assert_eq!(scale_price(1.999999999, DEFAULT_EXPO).unwrap(), 200_000_000);
        // other exponents scale accordingly
        assert_eq!(scale_price(2.5, -6).unwrap(), 2_500_000);
        assert_eq!(scale_price(2.5e9, 3).unwrap(), 2_500_000);
        // and anything that would lose the price is an error, never a silent 0
        assert!(scale_price(f64::NAN, DEFAULT_EXPO).is_err());
        assert!(scale_price(0.0, DEFAULT_EXPO).is_err());
        assert!(scale_price(-1.0, DEFAULT_EXPO).is_err());
        assert!(scale_price(1e30, DEFAULT_EXPO).is_err());
        assert!(scale_price(1e-12, DEFAULT_EXPO).is_err());
        assert!(validate_expo(DEFAULT_EXPO).is_ok());
        assert!(validate_expo(-19).is_err());
    }

    #[test]
    fn payload_key_order_is_deterministic() {
        let forward = vec![
            priced("wrap.near", 2.5, 1784720718),
            priced("aurora", 3000.0, 1784720718),
            priced("usdt.tether-token.near", 1.0, 1784720718),
        ];
        let mut reversed = forward.clone();
        reversed.reverse();

        let first = encode_payload(
            &build_entries(&forward, None, DEFAULT_EXPO).unwrap(),
            SigFormat::Json,
        )
        .unwrap()
        .0;
        let second = encode_payload(
            &build_entries(&reversed, None, DEFAULT_EXPO).unwrap(),
            SigFormat::Json,
        )
        .unwrap()
        .0;

        assert_eq!(first, second);
        assert_eq!(
            first,
            concat!(
                r#"{"aurora":{"price":"300000000000","expo":-8,"publish_time":1784720718},"#,
                r#""usdt.tether-token.near":{"price":"100000000","expo":-8,"publish_time":1784720718},"#,
                r#""wrap.near":{"price":"250000000","expo":-8,"publish_time":1784720718}}"#
            )
        );
    }

    #[test]
    fn aliases_rename_asset_ids() {
        let mut aliases = HashMap::new();
        aliases.insert("aurora".to_string(), "eth.bridge.near".to_string());

        let entries = build_entries(
            &[priced("aurora", 3000.0, 7), priced("wrap.near", 2.5, 7)],
            Some(&aliases),
            DEFAULT_EXPO,
        )
        .unwrap();

        assert!(entries.contains_key("eth.bridge.near"));
        assert!(!entries.contains_key("aurora"));
        assert!(entries.contains_key("wrap.near"));
        assert_eq!(entries["eth.bridge.near"].price, 300_000_000_000);

        // two assets mapped onto one client key must fail, not silently drop one
        let mut colliding = HashMap::new();
        colliding.insert("aurora".to_string(), "wrap.near".to_string());
        assert!(build_entries(
            &[priced("aurora", 3000.0, 7), priced("wrap.near", 2.5, 7)],
            Some(&colliding),
            DEFAULT_EXPO,
        )
        .is_err());
    }

    #[test]
    fn signature_verifies_against_derived_public_key() {
        // Deterministic 32-byte seed in NEAR "ed25519:<base58>" form
        let private_key = format!("ed25519:{}", bs58::encode([7u8; 32]).into_string());
        let (_, public_key) = near_tx::derive_implicit_account(&private_key).unwrap();

        let entries =
            build_entries(&[priced("wrap.near", 2.5, 1784720718)], None, DEFAULT_EXPO).unwrap();
        let (payload, message) = encode_payload(&entries, SigFormat::Json).unwrap();
        assert_eq!(message, payload.as_bytes());

        let signature = near_tx::sign_message(&private_key, &message).unwrap();

        // Exactly what a client does with the response: decode public_key, verify signature
        let key_bytes: [u8; 32] = bs58::decode(public_key.strip_prefix("ed25519:").unwrap())
            .into_vec()
            .unwrap()
            .try_into()
            .unwrap();
        let verifying_key = VerifyingKey::from_bytes(&key_bytes).unwrap();
        verifying_key
            .verify(&message, &Signature::from_bytes(&signature))
            .expect("signature must verify over the exact payload bytes");

        // the base64 we hand out decodes back to the same 64 bytes
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encode_signature(&signature))
            .unwrap();
        assert_eq!(decoded, signature.to_vec());

        // a mutated payload must not verify
        let tampered = payload.replace("250000000", "250000001");
        assert!(verifying_key
            .verify(tampered.as_bytes(), &Signature::from_bytes(&signature))
            .is_err());
    }

    #[test]
    fn borsh_payload_signs_the_decoded_bytes() {
        let entries =
            build_entries(&[priced("wrap.near", 2.5, 1784720718)], None, DEFAULT_EXPO).unwrap();
        let (payload, message) = encode_payload(&entries, SigFormat::Borsh).unwrap();

        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&payload)
            .unwrap();
        assert_eq!(decoded, message);
        assert_eq!(borsh::to_vec(&entries).unwrap(), message);
    }

    #[test]
    fn exclusions_are_validated_and_applied() {
        assert_eq!(
            validate_exclusions(&["Pyth".to_string(), "pyth".to_string()]).unwrap(),
            vec!["pyth".to_string()]
        );
        assert!(validate_exclusions(&["pyht".to_string()]).is_err());

        let config = ExchangeConfig {
            coingecko: Some("near".to_string()),
            binance: Some("NEARUSDT".to_string()),
            pyth: Some("0xabc".to_string()),
            ..Default::default()
        };
        let filtered = filter_exchange_config(&config, &["pyth".to_string()]);
        assert!(filtered.pyth.is_none());
        assert_eq!(filtered.binance.as_deref(), Some("NEARUSDT"));
        assert_eq!(configured_sources(&filtered), vec!["coingecko", "binance"]);

        assert!(is_excluded("pyth", &["pyth".to_string()]));
        assert!(!is_excluded("binance", &["pyth".to_string()]));
    }

    /// Locks the wire format the client sends: command tag, field names and defaults
    #[test]
    fn request_json_parses_into_the_command() {
        use crate::types::OracleCommand;

        let request = r#"{
            "command": "get_signed_prices",
            "tokens": ["wrap.near", "aurora"],
            "max_age_secs": 60,
            "key_name": "PROTECTED_ORACLE_KEY",
            "aliases": {"aurora": "eth.bridge.near"},
            "exclude_sources": ["pyth"]
        }"#;

        match serde_json::from_str::<OracleCommand>(request).unwrap() {
            OracleCommand::GetSignedPrices {
                tokens,
                max_age_secs,
                key_name,
                aliases,
                sig_format,
                expo,
                exclude_sources,
                aggregation_method,
                min_sources_num,
            } => {
                assert_eq!(tokens, vec!["wrap.near", "aurora"]);
                assert_eq!(max_age_secs, 60);
                assert_eq!(key_name, "PROTECTED_ORACLE_KEY");
                assert_eq!(
                    aliases.unwrap().get("aurora").map(String::as_str),
                    Some("eth.bridge.near")
                );
                // omitted fields fall back to the documented defaults
                assert_eq!(sig_format, None);
                assert_eq!(expo, None);
                assert_eq!(exclude_sources, Some(vec!["pyth".to_string()]));
                assert_eq!(aggregation_method, AggregationMethod::Median);
                assert_eq!(min_sources_num, 1);
            }
            other => panic!("wrong command parsed: {:?}", other),
        }
    }


    /// `KNOWN_SOURCES`, `filter_exchange_config` and `configured_sources` enumerate the same
    /// set in three places. When they drift the failure is silent and dangerous: a source
    /// absent from `filter_exchange_config` falls through its `_ => {}` arm, so a caller that
    /// asked to exclude it still gets it in the signed price. This pins all three together.
    #[test]
    fn every_source_is_filterable() {
        let all = all_sources_configured();

        // 1. configured_sources() sees exactly the sources KNOWN_SOURCES advertises
        let configured = configured_sources(&all);
        let mut expected = KNOWN_SOURCES.to_vec();
        let mut actual = configured.clone();
        expected.sort_unstable();
        actual.sort_unstable();
        assert_eq!(
            actual, expected,
            "configured_sources() and KNOWN_SOURCES disagree — a venue was added to one but \
             not the other"
        );

        // 2. every known source is accepted by validate_exclusions
        for source in KNOWN_SOURCES {
            assert!(
                validate_exclusions(&[source.to_string()]).is_ok(),
                "'{}' is in KNOWN_SOURCES but rejected by validate_exclusions",
                source
            );
        }

        // 3. and excluding it actually CLEARS its field, rather than falling through the
        //    catch-all arm and leaving the source contributing to the signed price
        for source in KNOWN_SOURCES {
            let filtered = filter_exchange_config(&all, &[source.to_string()]);
            let left = configured_sources(&filtered);
            assert!(
                !left.contains(&source),
                "excluding '{}' did not clear its ExchangeConfig field — filter_exchange_config \
                 is missing a match arm and the source still feeds the signed price",
                source
            );
            assert_eq!(
                left.len(),
                KNOWN_SOURCES.len() - 1,
                "excluding '{}' changed more than one source: {:?}",
                source,
                left
            );
        }

        // 4. excluding everything leaves nothing configured
        let none = filter_exchange_config(
            &all,
            &KNOWN_SOURCES.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        );
        assert!(configured_sources(&none).is_empty());
    }

    #[test]
    fn sig_format_defaults_to_json() {
        assert_eq!(SigFormat::parse(None).unwrap(), SigFormat::Json);
        assert_eq!(SigFormat::parse(Some("json")).unwrap(), SigFormat::Json);
        assert_eq!(SigFormat::parse(Some("borsh")).unwrap(), SigFormat::Borsh);
        assert!(SigFormat::parse(Some("protobuf")).is_err());
    }
}
