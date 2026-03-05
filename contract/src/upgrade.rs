use crate::*;

/// Pending upgrade code blob with deposit tracking.
#[derive(BorshSerialize, BorshDeserialize)]
#[borsh(crate = "near_sdk::borsh")]
pub struct PendingUpgrade {
    pub code: Vec<u8>,
    pub uploader: AccountId,
    /// Storage deposit paid by uploader (yoctoNEAR), refunded on removal.
    pub deposit: u128,
}

/// Previous contract state (deployed on-chain, before Pyth/Council were added).
#[derive(BorshDeserialize)]
#[borsh(crate = "near_sdk::borsh")]
pub struct OldContract {
    pub oracles: UnorderedMap<AccountId, VOracle>,
    pub assets: UnorderedMap<AssetId, VAsset>,
    pub recency_duration_sec: DurationSec,
    pub owner_id: AccountId,
    pub near_claim_amount: u128,
    pub outlayer_contract_id: Option<AccountId>,
    pub outlayer_code_source: Option<String>,
    pub subsidize_outlayer_calls: bool,
    pub secrets_profile: Option<String>,
    pub secrets_account_id: Option<AccountId>,
}

/// State after migrate_state2 (has asset_oracle_keys, but no asset_exchange_configs).
#[derive(BorshDeserialize)]
#[borsh(crate = "near_sdk::borsh")]
pub struct OldContractV3 {
    pub oracles: UnorderedMap<AccountId, VOracle>,
    pub assets: UnorderedMap<AssetId, VAsset>,
    pub recency_duration_sec: DurationSec,
    pub owner_id: AccountId,
    pub near_claim_amount: u128,
    pub outlayer_contract_id: Option<AccountId>,
    pub outlayer_code_source: Option<String>,
    pub subsidize_outlayer_calls: bool,
    pub secrets_profile: Option<String>,
    pub secrets_account_id: Option<AccountId>,
    pub pyth_price_id_to_asset: UnorderedMap<String, String>,
    pub pyth_asset_to_price_id: UnorderedMap<String, String>,
    pub pyth_stale_threshold: u64,
    pub council_members: Vec<AccountId>,
    pub council_threshold: u32,
    pub proposals: UnorderedMap<u64, council::VProposal>,
    pub next_proposal_id: u64,
    pub paused: bool,
    pub asset_oracle_keys: UnorderedMap<AssetId, String>,
    pub pending_upgrade_codes: UnorderedMap<String, PendingUpgrade>,
}

/// State after migrate_state (has Pyth/Council/Pause, but no asset_oracle_keys).
#[derive(BorshDeserialize)]
#[borsh(crate = "near_sdk::borsh")]
pub struct OldContractV2 {
    pub oracles: UnorderedMap<AccountId, VOracle>,
    pub assets: UnorderedMap<AssetId, VAsset>,
    pub recency_duration_sec: DurationSec,
    pub owner_id: AccountId,
    pub near_claim_amount: u128,
    pub outlayer_contract_id: Option<AccountId>,
    pub outlayer_code_source: Option<String>,
    pub subsidize_outlayer_calls: bool,
    pub secrets_profile: Option<String>,
    pub secrets_account_id: Option<AccountId>,
    pub pyth_price_id_to_asset: UnorderedMap<String, String>,
    pub pyth_asset_to_price_id: UnorderedMap<String, String>,
    pub pyth_stale_threshold: u64,
    pub council_members: Vec<AccountId>,
    pub council_threshold: u32,
    pub proposals: UnorderedMap<u64, council::VProposal>,
    pub next_proposal_id: u64,
    pub paused: bool,
}

#[near_bindgen]
impl Contract {
    /// Migration from pre-Pyth/Council state.
    #[private]
    #[init(ignore_state)]
    pub fn migrate_state() -> Self {
        let old: OldContract = env::state_read().unwrap();
        Self {
            oracles: old.oracles,
            assets: old.assets,
            recency_duration_sec: old.recency_duration_sec,
            owner_id: old.owner_id,
            near_claim_amount: old.near_claim_amount,
            outlayer_contract_id: old.outlayer_contract_id,
            outlayer_code_source: old.outlayer_code_source,
            subsidize_outlayer_calls: old.subsidize_outlayer_calls,
            secrets_profile: old.secrets_profile,
            secrets_account_id: old.secrets_account_id,
            pyth_price_id_to_asset: UnorderedMap::new(StorageKey::PythPriceIdToAsset),
            pyth_asset_to_price_id: UnorderedMap::new(StorageKey::PythAssetToPriceId),
            pyth_stale_threshold: 60,
            council_members: vec![],
            council_threshold: 1,
            proposals: UnorderedMap::new(StorageKey::Proposals),
            next_proposal_id: 0,
            paused: false,
            asset_oracle_keys: UnorderedMap::new(StorageKey::AssetOracleKeys),
            pending_upgrade_codes: UnorderedMap::new(StorageKey::PendingUpgradeCodes),
            asset_exchange_configs: UnorderedMap::new(StorageKey::AssetExchangeConfigs),
        }
    }

    /// Migration from post-Pyth/Council state: adds asset_oracle_keys + pending_upgrade_codes.
    #[private]
    #[init(ignore_state)]
    pub fn migrate_state2() -> Self {
        let old: OldContractV2 = env::state_read().unwrap();
        Self {
            oracles: old.oracles,
            assets: old.assets,
            recency_duration_sec: old.recency_duration_sec,
            owner_id: old.owner_id,
            near_claim_amount: old.near_claim_amount,
            outlayer_contract_id: old.outlayer_contract_id,
            outlayer_code_source: old.outlayer_code_source,
            subsidize_outlayer_calls: old.subsidize_outlayer_calls,
            secrets_profile: old.secrets_profile,
            secrets_account_id: old.secrets_account_id,
            pyth_price_id_to_asset: old.pyth_price_id_to_asset,
            pyth_asset_to_price_id: old.pyth_asset_to_price_id,
            pyth_stale_threshold: old.pyth_stale_threshold,
            council_members: old.council_members,
            council_threshold: old.council_threshold,
            proposals: old.proposals,
            next_proposal_id: old.next_proposal_id,
            paused: old.paused,
            asset_oracle_keys: UnorderedMap::new(StorageKey::AssetOracleKeys),
            pending_upgrade_codes: UnorderedMap::new(StorageKey::PendingUpgradeCodes),
            asset_exchange_configs: UnorderedMap::new(StorageKey::AssetExchangeConfigs),
        }
    }

    /// Migration from post-asset_oracle_keys state: adds asset_exchange_configs.
    #[private]
    #[init(ignore_state)]
    pub fn migrate_state3() -> Self {
        let old: OldContractV3 = env::state_read().unwrap();
        Self {
            oracles: old.oracles,
            assets: old.assets,
            recency_duration_sec: old.recency_duration_sec,
            owner_id: old.owner_id,
            near_claim_amount: old.near_claim_amount,
            outlayer_contract_id: old.outlayer_contract_id,
            outlayer_code_source: old.outlayer_code_source,
            subsidize_outlayer_calls: old.subsidize_outlayer_calls,
            secrets_profile: old.secrets_profile,
            secrets_account_id: old.secrets_account_id,
            pyth_price_id_to_asset: old.pyth_price_id_to_asset,
            pyth_asset_to_price_id: old.pyth_asset_to_price_id,
            pyth_stale_threshold: old.pyth_stale_threshold,
            council_members: old.council_members,
            council_threshold: old.council_threshold,
            proposals: old.proposals,
            next_proposal_id: old.next_proposal_id,
            paused: old.paused,
            asset_oracle_keys: old.asset_oracle_keys,
            pending_upgrade_codes: old.pending_upgrade_codes,
            asset_exchange_configs: UnorderedMap::new(StorageKey::AssetExchangeConfigs),
        }
    }

    /// Returns semver of this contract.
    pub fn get_version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    /// Upload WASM code for a pending DAO upgrade.
    /// Council member only. Attach NEAR to cover storage (~1 NEAR per 100KB).
    /// After uploading, create a proposal with `UpgradeContract { code_hash }`.
    /// Takes raw WASM bytes as function call input.
    #[payable]
    pub fn upload_upgrade_code(&mut self) -> String {
        let caller = env::predecessor_account_id();
        self.assert_council_member(&caller);

        let code = env::input().expect("No code provided");
        assert!(!code.is_empty(), "Empty code");

        let hash_bytes = env::sha256(&code);
        let code_hash = bytes_to_hex(&hash_bytes);
        assert!(
            self.pending_upgrade_codes.get(&code_hash).is_none(),
            "Code with this hash already uploaded"
        );

        let code_len = code.len();
        let attached = env::attached_deposit().as_yoctonear();

        // Measure storage cost (deposit is u128 fixed-size, value doesn't affect measurement)
        let storage_before = env::storage_usage();
        self.pending_upgrade_codes.insert(
            &code_hash,
            &PendingUpgrade {
                code,
                uploader: caller.clone(),
                deposit: attached, // store what they paid; refund excess below
            },
        );
        let storage_cost = u128::from(env::storage_usage() - storage_before)
            * env::storage_byte_cost().as_yoctonear();
        assert!(
            attached >= storage_cost,
            "Not enough deposit for storage. Need {} yoctoNEAR, got {}",
            storage_cost,
            attached
        );

        // Refund excess, keep only storage_cost as deposit
        let refund = attached - storage_cost;
        if refund > 1 {
            // Update stored deposit to actual cost
            let mut entry = self.pending_upgrade_codes.get(&code_hash).unwrap();
            entry.deposit = storage_cost;
            self.pending_upgrade_codes.insert(&code_hash, &entry);

            Promise::new(caller.clone()).transfer(NearToken::from_yoctonear(refund));
        }

        log!(
            "Upgrade code uploaded by {} (hash: {}, size: {} bytes, deposit: {} yoctoNEAR)",
            caller,
            code_hash,
            code_len,
            if refund > 1 { storage_cost } else { attached }
        );
        code_hash
    }

    /// Remove pending upgrade code by hash. Council member only.
    /// Refunds storage deposit to the original uploader.
    #[payable]
    pub fn remove_pending_upgrade_code(&mut self, code_hash: String) {
        near_sdk::assert_one_yocto();
        let caller = env::predecessor_account_id();
        self.assert_council_member(&caller);
        let entry = self
            .pending_upgrade_codes
            .remove(&code_hash)
            .expect("No pending code with this hash");
        if entry.deposit > 0 {
            Promise::new(entry.uploader.clone()).transfer(NearToken::from_yoctonear(entry.deposit));
        }
        log!(
            "Pending upgrade code removed: {} (refunded {} to {})",
            code_hash,
            entry.deposit,
            entry.uploader
        );
    }

    /// View: list all pending upgrade code hashes.
    pub fn get_pending_upgrade_hashes(&self) -> Vec<String> {
        self.pending_upgrade_codes.keys().collect()
    }
}
