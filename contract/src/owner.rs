use crate::*;

/// Owner methods — view only + bootstrap.
/// All state mutations go through DAO (council proposals).
#[near_bindgen]
impl Contract {
    pub fn get_owner_id(&self) -> AccountId {
        self.owner_id.clone()
    }

    pub fn get_near_claim_amount(&self) -> near_sdk::json_types::U128 {
        self.near_claim_amount.into()
    }

    pub fn get_subsidize_outlayer_calls(&self) -> bool {
        self.subsidize_outlayer_calls
    }

    pub fn can_subsidize_outlayer_calls(&self) -> bool {
        self.subsidize_outlayer_calls
            && env::account_balance().as_yoctonear() > crate::MIN_BALANCE_FOR_SUBSIDY
    }

    pub fn get_push_signer_accounts(&self, asset_id: AssetId) -> Option<Vec<AccountId>> {
        self.internal_get_asset(&asset_id)
            .and_then(|a| a.push_signer_accounts)
    }

    /// Get all asset -> push signer key env var mappings.
    pub fn get_asset_oracle_keys(&self) -> Vec<(AssetId, String)> {
        self.asset_oracle_keys.iter().collect()
    }
}

impl Contract {
    pub fn assert_owner(&self) {
        assert_eq!(
            self.owner_id,
            env::predecessor_account_id(),
            "Can only be called by the owner"
        );
    }
}
