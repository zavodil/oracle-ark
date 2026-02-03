use crate::*;

#[near_bindgen]
impl Contract {
    #[private]
    #[init(ignore_state)]
    pub fn migrate_state() -> Self {
        env::state_read().unwrap()
    }

    /// Returns semver of this contract.
    pub fn get_version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    /// Self upgrade and call migrate
    /// Takes as input serialized bytes of the new contract code.
    #[payable]
    pub fn upgrade(&mut self) {
        self.assert_owner();

        let code = env::input().expect("No code provided");

        // Deploy new code and call migrate_state
        Promise::new(env::current_account_id())
            .deploy_contract(code)
            .function_call(
                "migrate_state".to_string(),
                b"{}".to_vec(),
                NO_DEPOSIT,
                Gas::from_tgas(50),
            )
            .function_call(
                "get_owner_id".to_string(),
                b"{}".to_vec(),
                NO_DEPOSIT,
                Gas::from_tgas(5),
            );
    }
}
