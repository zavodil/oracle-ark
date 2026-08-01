use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{env, ext_contract, log, near_bindgen, AccountId, NearToken, PanicOnDefault, Promise};

/// Deposit that this contract attaches to oracle_call to cover OutLayer execution (0.02 NEAR)
const ORACLE_CALL_DEPOSIT: NearToken = NearToken::from_millinear(20);

/// Price precision: 8 decimals (same as oracle contract)
const PRICE_DECIMALS: u8 = 8;

/// Price type matching oracle-example/contract
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct Price {
    #[serde(with = "u128_dec_format")]
    #[schemars(with = "String")]
    pub multiplier: u128,
    pub decimals: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct AssetOptionalPrice {
    pub asset_id: String,
    pub price: Option<Price>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct PriceData {
    #[serde(with = "u64_dec_format")]
    #[schemars(with = "String")]
    pub timestamp: u64,
    pub recency_duration_sec: u32,
    pub prices: Vec<AssetOptionalPrice>,
}

/// Stored prediction
#[derive(BorshDeserialize, BorshSerialize)]
#[borsh(crate = "near_sdk::borsh")]
pub struct Prediction {
    pub token_id: String,
    /// Predicted price as multiplier with decimals=8
    pub predicted_multiplier: u128,
}

/// Cross-contract call to the oracle contract
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

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
#[borsh(crate = "near_sdk::borsh")]
pub struct Contract {
    pub oracle_contract_id: AccountId,
    pub predictions: LookupMap<AccountId, Prediction>,
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(oracle_contract_id: AccountId) -> Self {
        Self {
            oracle_contract_id,
            predictions: LookupMap::new(b"p"),
        }
    }

    /// Simple API: get_price("wrap.near")
    ///
    /// Calls oracle_call on the oracle contract, which fetches prices
    /// (from cache or via OutLayer WASI) and sends callback to this contract.
    ///
    /// This contract pays for the oracle call from its own balance (0.02 NEAR per call).
    /// No deposit required from the user.
    pub fn get_price(&mut self, token_id: String) -> Promise {
        ext_oracle::ext(self.oracle_contract_id.clone())
            .with_attached_deposit(ORACLE_CALL_DEPOSIT)
            .with_unused_gas_weight(1)
            .oracle_call(
                env::current_account_id(),
                Some(vec![token_id]),
                String::new(),
                None,
            )
    }

    /// Prediction market: guess the price of a token.
    ///
    /// Call `predict("wrap.near", 4.52)` to bet that NEAR costs $4.52.
    /// Then call `resolve()` to fetch the actual price and get the verdict.
    pub fn predict(&mut self, token_id: String, predicted_price: f64) {
        assert!(predicted_price > 0.0, "Price must be positive");
        let account_id = env::predecessor_account_id();
        let predicted_multiplier = (predicted_price * 10f64.powi(PRICE_DECIMALS as i32)) as u128;
        self.predictions.insert(
            &account_id,
            &Prediction {
                token_id: token_id.clone(),
                predicted_multiplier,
            },
        );
        log!(
            "{} predicted {} = {} USD",
            account_id,
            token_id,
            predicted_price
        );
    }

    /// Resolve the prediction: fetch actual price from oracle and compare.
    ///
    /// Returns verdict in logs: "higher", "lower", or "correct" (within 1%).
    pub fn resolve(&mut self) -> Promise {
        let account_id = env::predecessor_account_id();
        let prediction = self
            .predictions
            .get(&account_id)
            .expect("No prediction found. Call predict() first");

        ext_oracle::ext(self.oracle_contract_id.clone())
            .with_attached_deposit(ORACLE_CALL_DEPOSIT)
            .with_unused_gas_weight(1)
            .oracle_call(
                env::current_account_id(),
                Some(vec![prediction.token_id.clone()]),
                format!("resolve:{}", account_id),
                None,
            )
    }

    /// Callback from the oracle contract with price data.
    /// Handles both get_price (msg="") and resolve (msg="resolve:<account_id>").
    #[allow(unused_variables)]
    pub fn oracle_on_call(
        &mut self,
        sender_id: AccountId,
        data: PriceData,
        msg: String,
    ) -> Option<Price> {
        assert_eq!(
            env::predecessor_account_id(),
            self.oracle_contract_id,
            "Callback only from oracle contract"
        );

        let asset_price = match data.prices.first() {
            Some(ap) => ap,
            None => {
                log!("No price data received");
                return None;
            }
        };

        let price = match asset_price.price {
            Some(ref p) => p,
            None => {
                log!("No price available for {}", asset_price.asset_id);
                return None;
            }
        };

        let price_f64 = price.multiplier as f64 / 10f64.powi(price.decimals as i32);

        if let Some(predictor_str) = msg.strip_prefix("resolve:") {
            let predictor: AccountId = predictor_str.parse().expect("Invalid account in msg");
            let prediction = self
                .predictions
                .remove(&predictor)
                .expect("Prediction not found");

            let predicted_f64 =
                prediction.predicted_multiplier as f64 / 10f64.powi(PRICE_DECIMALS as i32);
            let diff_pct = ((price_f64 - predicted_f64) / price_f64 * 100.0).abs();

            let verdict = if diff_pct < 1.0 {
                "correct"
            } else if price_f64 > predicted_f64 {
                "higher"
            } else {
                "lower"
            };

            log!(
                "Prediction result for {}: {} predicted {} = {} USD, actual = {} USD -> {}",
                predictor,
                prediction.token_id,
                predictor,
                predicted_f64,
                price_f64,
                verdict
            );
        } else {
            log!(
                "Price for {}: {} (multiplier: {}, decimals: {})",
                asset_price.asset_id,
                price_f64,
                price.multiplier,
                price.decimals
            );
        }

        Some(price.clone())
    }
}

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
