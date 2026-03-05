use crate::*;
use near_sdk::assert_one_yocto;

/// Actions that can be proposed and voted on by the council.
#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
#[borsh(crate = "near_sdk::borsh")]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ProposalAction {
    // Oracle management
    AddOracle {
        #[schemars(with = "String")]
        account_id: AccountId,
    },
    RemoveOracle {
        #[schemars(with = "String")]
        account_id: AccountId,
    },

    // Asset management
    AddAsset {
        asset_id: AssetId,
        /// Name of the TEE-generated secret (PROTECTED_ env var) used to sign
        /// price push transactions (e.g., "PROTECTED_KEY_RHEA").
        /// None = warm only (prices fetched but not pushed to contract).
        push_signer_key: Option<String>,
    },
    RemoveAsset {
        asset_id: AssetId,
    },
    AddAssetEma {
        asset_id: AssetId,
        period_sec: DurationSec,
    },
    RemoveAssetEma {
        asset_id: AssetId,
        period_sec: DurationSec,
    },
    SetPushSignerAccounts {
        asset_id: AssetId,
        #[schemars(with = "Option<Vec<String>>")]
        push_signer_accounts: Option<Vec<AccountId>>,
    },

    // Config
    SetRecencyDurationSec {
        recency_duration_sec: DurationSec,
    },
    ConfigureOutlayer {
        #[schemars(with = "String")]
        outlayer_contract_id: AccountId,
        code_source: String,
        secrets_profile: Option<String>,
        #[schemars(with = "Option<String>")]
        secrets_account_id: Option<AccountId>,
    },
    SetSubsidizeOutlayerCalls {
        enabled: bool,
    },
    UpdateNearClaimAmount {
        #[schemars(with = "String")]
        near_claim_amount: U128,
    },

    // Pyth config
    AddPriceMapping {
        price_id_hex: String,
        asset_id: String,
    },
    RemovePriceMapping {
        price_id_hex: String,
    },
    SetPythStaleThreshold {
        threshold_sec: u64,
    },

    // Council governance
    AddCouncilMember {
        #[schemars(with = "String")]
        account_id: AccountId,
    },
    RemoveCouncilMember {
        #[schemars(with = "String")]
        account_id: AccountId,
    },

    /// Batch set/remove push signer keys for assets.
    /// Each entry: (asset_id, Some("PROTECTED_...") to push, None to make warm-only).
    /// The push_signer_key is the name of a TEE-generated secret (PROTECTED_ env var)
    /// whose private key signs price push transactions.
    SetPushSignerKeys {
        keys: Vec<(AssetId, Option<String>)>,
    },

    /// Register a TEE push signer: adds implicit account as oracle + sets
    /// push_signer_key for the specified assets.
    /// Created by `propose_register_push_signer` after resolving the
    /// PROTECTED_ key via OutLayer WASI (TEE).
    RegisterPushSigner {
        /// TEE secret name (e.g., "PROTECTED_KEY_RHEA")
        push_signer_key: String,
        /// Implicit account derived from the TEE key (hex-encoded ed25519 pubkey)
        #[schemars(with = "String")]
        signer_account_id: AccountId,
        /// Assets this signer will push prices for
        asset_ids: Vec<AssetId>,
    },

    /// Set exchange config for one asset. Config is opaque JSON — parsed by WASI, not contract.
    SetAssetExchangeConfig {
        asset_id: AssetId,
        config: String,
    },
    /// Batch set exchange configs (initial setup or bulk updates)
    SetAssetExchangeConfigs {
        configs: Vec<(AssetId, String)>,
    },
    /// Remove exchange config for an asset
    RemoveAssetExchangeConfig {
        asset_id: AssetId,
    },

    // Owner
    UpdateOwner {
        #[schemars(with = "String")]
        owner_id: AccountId,
    },

    /// Upgrade contract code. Upload code first via `upload_upgrade_code`,
    /// then create a proposal with this action.
    /// On approval: deploys stored code, optionally calls migration method,
    /// then clears code from state.
    UpgradeContract {
        /// SHA-256 hex hash of the uploaded code (verified before deploy)
        code_hash: String,
        /// Optional migration method to call after deploy (e.g., "migrate_state2")
        migrate_method: Option<String>,
    },

    // Pause
    Pause,
    Unpause,
}

/// Status of a proposal.
#[derive(
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, PartialEq, schemars::JsonSchema,
)]
#[serde(crate = "near_sdk::serde")]
#[borsh(crate = "near_sdk::borsh")]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    /// Actively accepting votes.
    Active,
    /// Reached threshold and action was executed.
    Approved,
    /// Removed by proposer or expired.
    Removed,
}

/// A council proposal with votes.
#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
#[borsh(crate = "near_sdk::borsh")]
pub struct Proposal {
    pub id: u64,
    #[schemars(with = "String")]
    pub proposer: AccountId,
    pub action: ProposalAction,
    pub status: ProposalStatus,
    #[schemars(with = "Vec<String>")]
    pub votes: Vec<AccountId>,
    #[serde(with = "u64_dec_format")]
    #[schemars(with = "String")]
    pub created_at: Timestamp,
}

/// Versioned proposal for storage evolution.
#[derive(BorshSerialize, BorshDeserialize)]
#[borsh(crate = "near_sdk::borsh")]
pub enum VProposal {
    Current(Proposal),
}

impl From<VProposal> for Proposal {
    fn from(v: VProposal) -> Self {
        match v {
            VProposal::Current(p) => p,
        }
    }
}

impl From<Proposal> for VProposal {
    fn from(p: Proposal) -> Self {
        VProposal::Current(p)
    }
}

#[near_bindgen]
impl Contract {
    // =========================================================================
    // Council view methods
    // =========================================================================

    /// Get all council members.
    pub fn get_council_members(&self) -> Vec<AccountId> {
        self.council_members.clone()
    }

    /// Get the voting threshold (>50% of members).
    pub fn get_council_threshold(&self) -> u32 {
        self.required_votes()
    }

    /// Whether the contract is paused.
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Get a proposal by ID.
    pub fn get_proposal(&self, id: u64) -> Option<Proposal> {
        self.proposals.get(&id).map(|v| v.into())
    }

    /// List proposals (paginated, most recent first).
    pub fn get_proposals(&self, from_index: Option<u64>, limit: Option<u64>) -> Vec<Proposal> {
        let limit = limit.unwrap_or(20).min(100) as usize;
        let total = self.next_proposal_id;
        if total == 0 {
            return vec![];
        }
        // from_index counts backwards from the latest proposal
        let skip = from_index.unwrap_or(0) as usize;
        let mut results = Vec::new();
        let mut remaining = limit;
        // Iterate from newest to oldest
        let mut id = total.saturating_sub(1).saturating_sub(skip as u64);
        loop {
            if remaining == 0 {
                break;
            }
            if let Some(vp) = self.proposals.get(&id) {
                results.push(Proposal::from(vp));
                remaining -= 1;
            }
            if id == 0 {
                break;
            }
            id -= 1;
        }
        results
    }

    // =========================================================================
    // Council mutate methods
    // =========================================================================

    /// Create a proposal. Only council members can propose.
    /// Caller's vote is automatically counted.
    /// If threshold == 1, the proposal is auto-executed immediately.
    /// Requires at least 1 yoctoNEAR. For config proposals, attach extra for storage deposit.
    #[payable]
    pub fn create_proposal(&mut self, action: ProposalAction) -> u64 {
        assert!(
            env::attached_deposit().as_yoctonear() >= 1,
            "Requires at least 1 yoctoNEAR"
        );
        let caller = env::predecessor_account_id();
        self.assert_council_member(&caller);

        let storage_before = env::storage_usage();
        let id = self.internal_create_proposal(caller.clone(), action);
        self.refund_storage_excess(caller, storage_before);
        id
    }

    /// Create multiple proposals in one transaction.
    /// Each action becomes a separate proposal. All auto-execute if threshold == 1.
    #[payable]
    pub fn create_proposals(&mut self, actions: Vec<ProposalAction>) -> Vec<u64> {
        assert!(
            env::attached_deposit().as_yoctonear() >= 1,
            "Requires at least 1 yoctoNEAR"
        );
        let caller = env::predecessor_account_id();
        self.assert_council_member(&caller);

        let storage_before = env::storage_usage();
        let ids: Vec<u64> = actions
            .into_iter()
            .map(|action| self.internal_create_proposal(caller.clone(), action))
            .collect();
        self.refund_storage_excess(caller, storage_before);
        ids
    }

    /// Vote to approve a proposal. Only council members can vote.
    /// Each member can vote once. Auto-executes when threshold is reached.
    #[payable]
    pub fn approve_proposal(&mut self, id: u64) {
        assert_one_yocto();
        let caller = env::predecessor_account_id();
        self.assert_council_member(&caller);

        let vp = self.proposals.get(&id).expect("Proposal not found");
        let mut proposal: Proposal = vp.into();

        assert!(
            proposal.status == ProposalStatus::Active,
            "Proposal is not active"
        );
        assert!(
            !proposal.votes.contains(&caller),
            "Already voted on this proposal"
        );

        proposal.votes.push(caller);

        if proposal.votes.len() as u32 >= self.required_votes() {
            self.execute_proposal(&mut proposal);
        }

        self.proposals.insert(&id, &VProposal::from(proposal));
    }

    /// Remove an active proposal. Only the proposer or owner can remove.
    #[payable]
    pub fn remove_proposal(&mut self, id: u64) {
        assert_one_yocto();
        let caller = env::predecessor_account_id();

        let vp = self.proposals.get(&id).expect("Proposal not found");
        let mut proposal: Proposal = vp.into();

        assert!(
            proposal.status == ProposalStatus::Active,
            "Proposal is not active"
        );
        assert!(
            caller == proposal.proposer || caller == self.owner_id,
            "Only proposer or owner can remove"
        );

        // Clear pending upgrade code and refund deposit if removing an upgrade proposal
        if let ProposalAction::UpgradeContract { ref code_hash, .. } = proposal.action {
            if let Some(entry) = self.pending_upgrade_codes.remove(code_hash) {
                if entry.deposit > 0 {
                    Promise::new(entry.uploader.clone())
                        .transfer(NearToken::from_yoctonear(entry.deposit));
                }
                log!(
                    "Pending upgrade code removed: {} (refunded {} to {})",
                    code_hash,
                    entry.deposit,
                    entry.uploader
                );
            }
        }

        proposal.status = ProposalStatus::Removed;
        self.proposals.insert(&id, &VProposal::from(proposal));
        log!("Proposal #{} removed", id);
    }

    // =========================================================================
    // Council admin methods (owner only — bootstrap council)
    // =========================================================================

    /// Initialize or replace the council. Owner only.
    /// Threshold is always >50% of members (computed automatically).
    #[payable]
    pub fn set_council(&mut self, members: Vec<AccountId>) {
        assert_one_yocto();
        self.assert_owner();
        assert!(!members.is_empty(), "Council must have at least one member");
        self.council_members = members;
        log!(
            "Council set: {} members, threshold {}",
            self.council_members.len(),
            self.required_votes()
        );
    }
}

// =============================================================================
// Internal helpers
// =============================================================================

impl Contract {
    /// Charge for storage growth; refund excess deposit.
    fn refund_storage_excess(&self, caller: AccountId, storage_before: u64) {
        let storage_after = env::storage_usage();
        let storage_cost = u128::from(storage_after.saturating_sub(storage_before))
            * env::storage_byte_cost().as_yoctonear();
        let deposit = env::attached_deposit().as_yoctonear();
        assert!(
            deposit >= storage_cost,
            "Insufficient deposit for storage. Need {} yoctoNEAR, got {}",
            storage_cost,
            deposit
        );
        let refund = deposit - storage_cost;
        if refund > 1 {
            Promise::new(caller).transfer(NearToken::from_yoctonear(refund));
        }
    }

    pub fn assert_council_member(&self, account_id: &AccountId) {
        assert!(
            self.council_members.contains(account_id),
            "Not a council member"
        );
    }

    pub fn assert_not_paused(&self) {
        assert!(!self.paused, "Contract is paused");
    }

    /// >50% of council members required to approve
    pub fn required_votes(&self) -> u32 {
        (self.council_members.len() as u32) / 2 + 1
    }

    pub(crate) fn internal_create_proposal(&mut self, caller: AccountId, action: ProposalAction) -> u64 {
        let id = self.next_proposal_id;
        self.next_proposal_id += 1;

        let mut proposal = Proposal {
            id,
            proposer: caller.clone(),
            action,
            status: ProposalStatus::Active,
            votes: vec![caller],
            created_at: env::block_timestamp(),
        };

        if proposal.votes.len() as u32 >= self.required_votes() {
            self.execute_proposal(&mut proposal);
        }

        self.proposals.insert(&id, &VProposal::from(proposal));
        log!("Proposal #{} created", id);
        id
    }

    /// Execute the action inside a proposal. Called when threshold is met.
    fn execute_proposal(&mut self, proposal: &mut Proposal) {
        match &proposal.action {
            ProposalAction::AddOracle { account_id } => {
                assert!(self.internal_get_oracle(account_id).is_none(), "Oracle already exists");
                self.internal_set_oracle(account_id, Oracle::new());
                log!("Oracle added: {}", account_id);
            }
            ProposalAction::RemoveOracle { account_id } => {
                assert!(self.oracles.remove(account_id).is_some(), "Oracle not found");
                log!("Oracle removed: {}", account_id);
            }
            ProposalAction::AddAsset { asset_id, push_signer_key } => {
                assert!(self.internal_get_asset(asset_id).is_none(), "Asset already exists");
                self.internal_set_asset(asset_id, Asset::new());
                if let Some(key) = push_signer_key {
                    assert!(key.starts_with("PROTECTED_"), "push_signer_key must start with PROTECTED_");
                    self.asset_oracle_keys.insert(asset_id, key);
                    log!("Asset added: {} (push_signer_key: {})", asset_id, key);
                } else {
                    log!("Asset added: {} (warm only)", asset_id);
                }
            }
            ProposalAction::RemoveAsset { asset_id } => {
                assert!(self.assets.remove(asset_id).is_some(), "Asset not found");
                log!("Asset removed: {}", asset_id);
            }
            ProposalAction::AddAssetEma {
                asset_id,
                period_sec,
            } => {
                let mut asset = self.internal_get_asset(asset_id).expect("Asset not found");
                assert!(
                    !asset.emas.iter().any(|e| e.period_sec == *period_sec),
                    "EMA period already exists"
                );
                asset.emas.push(AssetEma::new(*period_sec));
                self.internal_set_asset(asset_id, asset);
                log!("EMA added: {} period {}s", asset_id, period_sec);
            }
            ProposalAction::RemoveAssetEma {
                asset_id,
                period_sec,
            } => {
                let mut asset = self.internal_get_asset(asset_id).expect("Asset not found");
                let before = asset.emas.len();
                asset.emas.retain(|e| e.period_sec != *period_sec);
                assert!(asset.emas.len() < before, "EMA period not found");
                self.internal_set_asset(asset_id, asset);
                log!("EMA removed: {} period {}s", asset_id, period_sec);
            }
            ProposalAction::SetPushSignerAccounts {
                asset_id,
                push_signer_accounts,
            } => {
                let mut asset = self.internal_get_asset(asset_id).expect("Asset not found");
                asset.push_signer_accounts = push_signer_accounts.clone();
                self.internal_set_asset(asset_id, asset);
                log!("Push signer accounts updated for asset {}", asset_id);
            }
            ProposalAction::SetRecencyDurationSec {
                recency_duration_sec,
            } => {
                self.recency_duration_sec = *recency_duration_sec;
                log!("Recency duration set to {}s", recency_duration_sec);
            }
            ProposalAction::ConfigureOutlayer {
                outlayer_contract_id,
                code_source,
                secrets_profile,
                secrets_account_id,
            } => {
                self.outlayer_contract_id = Some(outlayer_contract_id.clone());
                self.outlayer_code_source = Some(code_source.clone());
                self.secrets_profile = secrets_profile.clone();
                self.secrets_account_id = secrets_account_id.clone();
                log!("OutLayer configured via proposal");
            }
            ProposalAction::SetSubsidizeOutlayerCalls { enabled } => {
                self.subsidize_outlayer_calls = *enabled;
                log!("Subsidize OutLayer calls: {}", enabled);
            }
            ProposalAction::UpdateNearClaimAmount { near_claim_amount } => {
                self.near_claim_amount = near_claim_amount.0;
                log!("NEAR claim amount updated");
            }
            ProposalAction::AddPriceMapping {
                price_id_hex,
                asset_id,
            } => {
                assert!(
                    price_id_hex.len() == 64 && price_id_hex.chars().all(|c| c.is_ascii_hexdigit()),
                    "price_id_hex must be 64 hex characters"
                );
                self.pyth_price_id_to_asset
                    .insert(price_id_hex, asset_id);
                self.pyth_asset_to_price_id
                    .insert(asset_id, price_id_hex);
                log!("Pyth mapping added: {} -> {}", price_id_hex, asset_id);
            }
            ProposalAction::RemovePriceMapping { price_id_hex } => {
                if let Some(asset_id) = self.pyth_price_id_to_asset.remove(price_id_hex) {
                    self.pyth_asset_to_price_id.remove(&asset_id);
                    log!("Pyth mapping removed: {}", price_id_hex);
                }
            }
            ProposalAction::SetPythStaleThreshold { threshold_sec } => {
                self.pyth_stale_threshold = *threshold_sec;
                log!("Pyth stale threshold set to {}s", threshold_sec);
            }
            ProposalAction::SetPushSignerKeys { keys } => {
                for (asset_id, key) in keys {
                    assert!(self.internal_get_asset(asset_id).is_some(), "Asset not found: {}", asset_id);
                    match key {
                        Some(k) => {
                            assert!(k.starts_with("PROTECTED_"), "push_signer_key must start with PROTECTED_");
                            self.asset_oracle_keys.insert(asset_id, k);
                            log!("Push signer key set: {} -> {}", asset_id, k);
                        }
                        None => {
                            self.asset_oracle_keys.remove(asset_id);
                            log!("Push signer key removed: {} (warm only)", asset_id);
                        }
                    }
                }
            }
            ProposalAction::RegisterPushSigner {
                push_signer_key,
                signer_account_id,
                asset_ids,
            } => {
                // Register as oracle if not already registered
                if self.internal_get_oracle(signer_account_id).is_none() {
                    self.internal_set_oracle(signer_account_id, Oracle::new());
                    log!("Oracle added: {}", signer_account_id);
                }
                // Set push_signer_key and push_signer_accounts for all specified assets
                for asset_id in asset_ids {
                    let mut asset = self.internal_get_asset(asset_id).expect("Asset not found");
                    // Only this TEE-derived account can push prices for this asset
                    asset.push_signer_accounts = Some(vec![signer_account_id.clone()]);
                    self.internal_set_asset(asset_id, asset);
                    self.asset_oracle_keys.insert(asset_id, push_signer_key);
                    log!("Push signer set: {} -> {} (account: {})", asset_id, push_signer_key, signer_account_id);
                }
                log!(
                    "Push signer registered: {} -> {}",
                    push_signer_key,
                    signer_account_id
                );
            }
            ProposalAction::AddCouncilMember { account_id } => {
                assert!(
                    !self.council_members.contains(account_id),
                    "Already a council member"
                );
                self.council_members.push(account_id.clone());
                log!("Council member added: {}", account_id);
            }
            ProposalAction::RemoveCouncilMember { account_id } => {
                let before = self.council_members.len();
                self.council_members.retain(|m| m != account_id);
                assert!(self.council_members.len() < before, "Not a council member");
                assert!(
                    !self.council_members.is_empty(),
                    "Cannot remove last council member"
                );
                log!("Council member removed: {} (threshold now {})", account_id, self.required_votes());
            }
            ProposalAction::SetAssetExchangeConfig { asset_id, config } => {
                assert!(
                    self.internal_get_asset(asset_id).is_some(),
                    "Asset not found: {}",
                    asset_id
                );
                self.asset_exchange_configs.insert(asset_id, config);
                log!("Exchange config set for {}", asset_id);
            }
            ProposalAction::SetAssetExchangeConfigs { configs } => {
                for (asset_id, config) in configs {
                    assert!(
                        self.internal_get_asset(asset_id).is_some(),
                        "Asset not found: {}",
                        asset_id
                    );
                    self.asset_exchange_configs.insert(asset_id, config);
                    log!("Exchange config set for {}", asset_id);
                }
            }
            ProposalAction::RemoveAssetExchangeConfig { asset_id } => {
                assert!(
                    self.asset_exchange_configs.remove(asset_id).is_some(),
                    "Exchange config not found: {}",
                    asset_id
                );
                log!("Exchange config removed for {}", asset_id);
            }
            ProposalAction::UpdateOwner { owner_id } => {
                self.owner_id = owner_id.clone();
                log!("Owner updated to {}", owner_id);
            }
            ProposalAction::UpgradeContract {
                code_hash,
                migrate_method,
            } => {
                let entry = self
                    .pending_upgrade_codes
                    .remove(code_hash)
                    .expect("No pending upgrade code with this hash");

                let mut promise =
                    Promise::new(env::current_account_id()).deploy_contract(entry.code);

                if let Some(method) = migrate_method {
                    promise = promise.function_call(
                        method.clone(),
                        b"{}".to_vec(),
                        NO_DEPOSIT,
                        Gas::from_tgas(50),
                    );
                }

                // Verify the contract still works after deploy
                let _ = promise.function_call(
                    "get_version".to_string(),
                    b"{}".to_vec(),
                    NO_DEPOSIT,
                    Gas::from_tgas(5),
                );

                // Refund storage deposit to uploader
                if entry.deposit > 0 {
                    Promise::new(entry.uploader.clone())
                        .transfer(NearToken::from_yoctonear(entry.deposit));
                }

                log!(
                    "Upgrade deployed (hash: {}, refunded {} to {})",
                    code_hash,
                    entry.deposit,
                    entry.uploader
                );
            }
            ProposalAction::Pause => {
                self.paused = true;
                log!("Contract paused");
            }
            ProposalAction::Unpause => {
                self.paused = false;
                log!("Contract unpaused");
            }
        }

        proposal.status = ProposalStatus::Approved;
        log!("Proposal #{} approved and executed", proposal.id);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_required_votes_formula() {
        // Formula: n/2 + 1 (integer division) — always >50%
        let required = |n: u32| n / 2 + 1;

        // 1 member: 1 vote — auto-execute by proposer
        assert_eq!(required(1), 1);
        // 2 members: 2 votes — both must agree
        assert_eq!(required(2), 2);
        // 3 members: 2 of 3
        assert_eq!(required(3), 2);
        // 4 members: 3 of 4
        assert_eq!(required(4), 3);
        // 5 members: 3 of 5
        assert_eq!(required(5), 3);
        // 6 members: 4 of 6
        assert_eq!(required(6), 4);
        // 7 members: 4 of 7
        assert_eq!(required(7), 4);
        // 10 members: 6 of 10
        assert_eq!(required(10), 6);
    }

    #[test]
    fn test_required_votes_always_majority() {
        let required = |n: u32| n / 2 + 1;
        for n in 1..=100 {
            let r = required(n);
            // Must be strictly more than half
            assert!(r * 2 > n, "required({}) = {} is not >50%", n, r);
            // Must be achievable (not more than total members)
            assert!(r <= n, "required({}) = {} exceeds member count", n, r);
        }
    }
}
