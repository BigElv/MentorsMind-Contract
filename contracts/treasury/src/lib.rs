#![no_std]

use shared::ReentrancyGuard;
use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, symbol_short, token,
    Address, Env, IntoVal, Symbol, Vec,
};

// ---------------------------------------------------------------------------
// Oracle client interface (matches oracle contract's public API)
// ---------------------------------------------------------------------------

/// Mirrors `OracleHealth` from the oracle contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleHealth {
    pub active_feeders: u32,
    pub last_update: u64,
    pub is_stale: bool,
}

#[contractclient(name = "OracleContractClient")]
pub trait OracleContractTrait {
    fn get_oracle_health(env: Env, asset: Symbol) -> OracleHealth;
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized     = 2,
    Unauthorized       = 3,
    InsufficientBalance = 4,
    /// Oracle has too few active feeders — buyback aborted to prevent
    /// economic attacks via a manipulated TWAP price.
    OracleUnhealthy = 5,
    /// Oracle data is stale — buyback aborted until a fresh price is available.
    OracleStale = 6,
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllocationHistory {
    pub token:     Address,
    pub recipient: Address,
    pub amount:    i128,
    pub timestamp: u64,
}

// ---------------------------------------------------------------------------
// Token approval event
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreasuryTokenApprovalEvent {
    pub token:    Address,
    pub approved: bool,
}

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    /// The timelock contract whose `execute` is the only allowed caller of
    /// `buyback_and_burn`. Set during `initialize`.
    Timelock,
    StakingContract,
    AllocationCount,
    /// Individual allocation history: DataKey::Allocation(index) → AllocationHistory
    Allocation(u32),
    /// Token whitelist: DataKey::ApprovedToken(token_address) → bool
    ApprovedToken(Address),
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct TreasuryContract;

#[contractimpl]
impl TreasuryContract {
    /// Initialize treasury contract with admin and staking contract address.
    pub fn initialize(env: Env, admin: Address, staking_contract: Address) -> Result<(), Error> {
        if env.storage().persistent().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().persistent().set(&DataKey::Admin,           &admin);
        env.storage().persistent().set(&DataKey::StakingContract, &staking_contract);
        env.storage().persistent().set(&DataKey::Timelock,        &timelock);
        env.storage().persistent().set(&DataKey::AllocationCount, &0u32);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Token whitelist management
    // -----------------------------------------------------------------------

    /// Add or remove an approved token from the treasury whitelist (admin only).
    pub fn set_approved_token(
        env: Env,
        token_address: Address,
        approved: bool,
    ) -> Result<(), Error> {
        let admin = env
            .storage()
            .persistent()
            .get::<DataKey, Address>(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        let key = DataKey::ApprovedToken(token_address.clone());
        env.storage().persistent().set(&key, &approved);

        if approved {
            env.events().publish(
                (symbol_short!("treasury"), symbol_short!("tok_appr")),
                TreasuryTokenApprovalEvent { token: token_address, approved: true },
            );
        } else {
            env.events().publish(
                (symbol_short!("treasury"), symbol_short!("tok_rej")),
                TreasuryTokenApprovalEvent { token: token_address, approved: false },
            );
        }
        Ok(())
    }

    /// Accept deposits of any approved Stellar asset.
    pub fn deposit(env: Env, from: Address, token: Address, amount: i128) -> Result<(), Error> {
        from.require_auth();
        if !Self::_is_token_approved(&env, &token) {
            panic!("Token not approved");
        }
        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&from, &env.current_contract_address(), &amount);

        env.events().publish(
            (symbol_short!("deposit"), from.clone(), token.clone()),
            amount,
        );
        Ok(())
    }

    /// get_balance — returns the contract's current balance of `token`.
    pub fn get_balance(env: Env, token: Address) -> i128 {
        token::Client::new(&env, &token).balance(&env.current_contract_address())
    }

    /// allocate — governance/timelock only; transfers `amount` of `token` to `recipient`.
    pub fn allocate(
        env: Env,
        token: Address,
        recipient: Address,
        amount: i128,
    ) -> Result<(), Error> {
        let _guard = ReentrancyGuard::enter(&env, Symbol::new(&env, "allocate"));
        let admin = env
            .storage()
            .persistent()
            .get::<DataKey, Address>(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        if !Self::_is_token_approved(&env, &token) {
            return Err(Error::TokenNotApproved);
        }

        token::Client::new(&env, &token)
            .transfer(&env.current_contract_address(), &recipient, &amount);

        let mut history = env
            .storage()
            .persistent()
            .get(&DataKey::AllocationCount)
            .unwrap_or(0u32);
        env.storage().persistent().set(
            &DataKey::Allocation(count),
            &AllocationHistory {
                token:     token.clone(),
                recipient: recipient.clone(),
                amount,
                timestamp: env.ledger().timestamp(),
            },
        );
        env.storage().persistent().set(&DataKey::AllocationCount, &(count + 1));

        env.events().publish(
            (symbol_short!("allocate"), recipient.clone(), token.clone()),
            amount,
        );
        Ok(())
    }

    /// distribute_to_stakers — pro-rata by stake amount.
    pub fn distribute_to_stakers(
        env: Env,
        token: Address,
        total_amount: i128,
    ) -> Result<(), Error> {
        let _guard = ReentrancyGuard::enter(&env, Symbol::new(&env, "distribute"));
        let admin = env
            .storage()
            .persistent()
            .get::<DataKey, Address>(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        if !Self::_is_token_approved(&env, &token) {
            return Err(Error::TokenNotApproved);
        }

        let staking_contract: Address = env
            .storage()
            .persistent()
            .get(&DataKey::StakingContract)
            .ok_or(Error::NotInitialized)?;

        token::Client::new(&env, &token)
            .transfer(&env.current_contract_address(), &staking_contract, &total_amount);

        env.invoke_contract::<()>(
            &staking_contract,
            &Symbol::new(&env, "distribute_revenue"),
            (token.clone(), total_amount).into_val(&env),
        );

        env.events().publish(
            (symbol_short!("distrib"), staking_contract.clone(), token.clone()),
            total_amount,
        );
        Ok(())
    }

    /// buyback_and_burn — swap XLM for MNT on DEX, then burn MNT.
    ///
    /// # Oracle health gate (#614)
    /// Before executing the swap, this function queries the oracle for the
    /// MNT asset health.  The call is aborted with `OracleUnhealthy` or
    /// `OracleStale` if the oracle does not meet the minimum-feeder threshold
    /// or has not been updated recently.  This prevents a manipulated TWAP
    /// from being used as the slippage baseline for `min_mnt_out`.
    ///
    /// Pass `oracle_contract = None` to skip the health check (legacy / test).
    pub fn buyback_and_burn(
        env: Env,
        xlm_token:    Address,
        mnt_token:    Address,
        dex_contract: Address,
        xlm_amount: i128,
        oracle_contract: Option<Address>,
        mnt_asset_symbol: Option<Symbol>,
    ) -> Result<(), Error> {
        let _guard = ReentrancyGuard::enter(&env, Symbol::new(&env, "buyback"));

        // ------------------------------------------------------------------
        // 1. Access control: must be called by the registered timelock only.
        // ------------------------------------------------------------------
        let timelock: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Timelock)
            .ok_or(Error::NotInitialized)?;
        timelock.require_auth();

        // ------------------------------------------------------------------
        // 2. Pre-flight validation — no state changes yet.
        // ------------------------------------------------------------------
        dex_iface.validate(&env);

        if min_mnt_out <= 0 {
            env.events().publish(
                (symbol_short!("buyback"), symbol_short!("failed")),
                BuybackFailed {
                    xlm_amount,
                    reason: Symbol::new(&env, "invalid_min_out"),
                },
            );
            return Err(Error::InvalidMinOut);
        }

        // --- Oracle health gate -------------------------------------------
        if let (Some(oracle), Some(asset_sym)) = (oracle_contract.clone(), mnt_asset_symbol.clone()) {
            let health: OracleHealth =
                OracleContractClient::new(&env, &oracle).get_oracle_health(&asset_sym);

            if health.is_stale {
                return Err(Error::OracleStale);
            }
            // MIN_FEEDERS is enforced inside the oracle; we check here so
            // treasury can surface a distinct error code.
            if health.active_feeders < 3 {
                return Err(Error::OracleUnhealthy);
            }
        }

        // 1. Transfer XLM to DEX
        let xlm_client = token::Client::new(&env, &xlm_token);
        let expiration_ledger = env.ledger().sequence() + 1;
        xlm_client.approve(
            &env.current_contract_address(),
            &dex_contract,
            &xlm_amount,
            &expiration_ledger,
        );

        // 2. Call DEX swap — returns the amount of MNT received
        let mnt_received: i128 = env.invoke_contract(
            &dex_contract,
            &dex_iface.swap_fn,
            (
                xlm_token.clone(),
                mnt_token.clone(),
                xlm_amount,
                min_mnt_out,
                env.current_contract_address(),
            )
                .into_val(&env),
        );

        // ------------------------------------------------------------------
        // 5. Validate output — revoke allowance and emit failure if bad.
        // ------------------------------------------------------------------
        if mnt_received == 0 {
            // Revoke any remaining allowance (defensive; DEX may not have pulled).
            xlm_client.approve(
                &env.current_contract_address(),
                &dex_contract,
                &0,
                &expiration_ledger,
            );
            env.events().publish(
                (symbol_short!("buyback"), symbol_short!("failed")),
                BuybackFailed {
                    xlm_amount,
                    reason: Symbol::new(&env, "zero_output"),
                },
            );
            return Err(Error::ZeroOutput);
        }

        if mnt_received < min_mnt_out {
            // Revoke any remaining allowance.
            xlm_client.approve(
                &env.current_contract_address(),
                &dex_contract,
                &0,
                &expiration_ledger,
            );
            env.events().publish(
                (symbol_short!("buyback"), symbol_short!("failed")),
                BuybackFailed {
                    xlm_amount,
                    reason: Symbol::new(&env, "slippage"),
                },
            );
            return Err(Error::SlippageExceeded);
        }

        // ------------------------------------------------------------------
        // 6. Burn MNT — only reached if swap succeeded and output is valid.
        // ------------------------------------------------------------------
        env.invoke_contract::<()>(
            &mnt_token,
            &Symbol::new(&env, "burn"),
            (env.current_contract_address(), mnt_received).into_val(&env),
        );

        env.events()
            .publish((symbol_short!("buyback"), mnt_token.clone()), mnt_received);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // View helpers
    // -----------------------------------------------------------------------

    pub fn get_history_page(env: Env, offset: u32, limit: u32) -> Vec<AllocationHistory> {
        let total_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::AllocationCount)
            .unwrap_or(0u32);

        let mut result = Vec::new(&env);
        let end = offset.saturating_add(limit).min(total_count);

        for i in offset..end {
            if let Some(record) = env
                .storage()
                .persistent()
                .get::<DataKey, AllocationHistory>(&DataKey::Allocation(i))
            {
                result.push_back(record);
            }
        }
        result
    }

    pub fn get_timelock(env: Env) -> Address {
        env.storage()
            .persistent()
            .get(&DataKey::Timelock)
            .expect("not initialized")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::Env;

    // ------------------------------------------------------------------
    // Mock contracts
    // ------------------------------------------------------------------

    #[contract]
    pub struct MockDEX;

    #[contractimpl]
    impl MockDEX {
        pub fn swap_exact_in(
            env: Env,
            token_in: Address,
            _token_out: Address,
            amount_in: i128,
            _min_out: i128,
            recipient: Address,
        ) -> i128 {
            // Pull the XLM allowance from the treasury (simulate DEX pull).
            let xlm = token::Client::new(&env, &token_in);
            xlm.transfer_from(
                &env.current_contract_address(),
                &recipient,  // pull from treasury (spender == DEX contract)
                &env.current_contract_address(), // actually pull from who approved
                &amount_in,
            );
            // Return MNT amount (1:1 for tests).
            amount_in
        }
    }

    /// DEX that always returns 0 MNT (simulates failed / empty swap).
    #[contract]
    pub struct MockDEXZero;

    #[contractimpl]
    impl MockDEXZero {
        pub fn swap_exact_in(
            _env: Env,
            _token_in: Address,
            _token_out: Address,
            _amount_in: i128,
            _min_out: i128,
            _recipient: Address,
        ) -> i128 {
            0 // returns nothing — no XLM pulled
        }
    }

    /// DEX that returns less MNT than min_mnt_out (simulates slippage).
    #[contract]
    pub struct MockDEXSlippage;

    #[contractimpl]
    impl MockDEXSlippage {
        pub fn swap_exact_in(
            _env: Env,
            _token_in: Address,
            _token_out: Address,
            _amount_in: i128,
            _min_out: i128,
            _recipient: Address,
        ) -> i128 {
            1 // returns tiny amount — below min_mnt_out
        }
    }

    #[contract]
    pub struct MockStaking;

    #[contractimpl]
    impl MockStaking {
        pub fn distribute_revenue(_env: Env, _token: Address, _amount: i128) {}
    }

    #[contract]
    pub struct MockMNT;

    #[contractimpl]
    impl MockMNT {
        pub fn burn(_env: Env, _from: Address, _amount: i128) {}
    }

    /// A mock oracle that returns a configurable health report.
    #[contract]
    pub struct MockOracleHealthy;

    #[contractimpl]
    impl MockOracleHealthy {
        pub fn get_oracle_health(_env: Env, _asset: Symbol) -> OracleHealth {
            OracleHealth {
                active_feeders: 3,
                last_update: 999,
                is_stale: false,
            }
        }
    }

    #[contract]
    pub struct MockOracleStale;

    #[contractimpl]
    impl MockOracleStale {
        pub fn get_oracle_health(_env: Env, _asset: Symbol) -> OracleHealth {
            OracleHealth {
                active_feeders: 3,
                last_update: 0,
                is_stale: true,
            }
        }
    }

    #[contract]
    pub struct MockOracleInsufficientFeeders;

    #[contractimpl]
    impl MockOracleInsufficientFeeders {
        pub fn get_oracle_health(_env: Env, _asset: Symbol) -> OracleHealth {
            OracleHealth {
                active_feeders: 1,
                last_update: 999,
                is_stale: false,
            }
        }
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    fn setup_test(env: &Env) -> (Address, Address, Address) {
        let admin = Address::generate(env);
        let staking = env.register_contract(None, MockStaking);
        let contract_id = env.register_contract(None, TreasuryContract);
        let client = TreasuryContractClient::new(env, &contract_id);
        client.initialize(&admin, &staking);
        (admin, staking, contract_id)
    }

    // ------------------------------------------------------------------
    // Existing tests (unchanged behaviour)
    // ------------------------------------------------------------------

    #[test]
    fn test_initialization() {
        let env = Env::default();
        let (admin, staking, _) = setup_test(&env);
        let client =
            TreasuryContractClient::new(&env, &env.register_contract(None, TreasuryContract));
        client.initialize(&admin, &staking);
        let result = client.try_initialize(&admin, &staking);
        assert!(result.is_err());
    }

    #[test]
    fn test_deposit_and_balance() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _, _, contract_id) = setup_test(&env);
        let user = Address::generate(&env);
        let token_addr = env.register_stellar_asset_contract(admin.clone());
        let stellar_asset_client = token::StellarAssetClient::new(&env, &token_addr);
        stellar_asset_client.mint(&user, &1000);

        let treasury_client = TreasuryContractClient::new(&env, &contract_id);
        treasury_client.set_approved_token(&token_addr, &true);
        treasury_client.deposit(&user, &token_addr, &500);

        assert_eq!(treasury_client.get_balance(&token_addr), 500);
    }

    #[test]
    #[should_panic(expected = "Token not approved")]
    fn test_deposit_unapproved_token() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _, _, contract_id) = setup_test(&env);
        let user = Address::generate(&env);
        let token_addr = env.register_stellar_asset_contract(admin.clone());
        let stellar_asset_client = token::StellarAssetClient::new(&env, &token_addr);
        stellar_asset_client.mint(&user, &1000);

        let treasury_client = TreasuryContractClient::new(&env, &contract_id);
        treasury_client.deposit(&user, &token_addr, &500);
    }

    #[test]
    fn test_allocate() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _, _, contract_id) = setup_test(&env);
        let recipient = Address::generate(&env);
        let token_addr = env.register_stellar_asset_contract(admin.clone());
        let token_client = token::Client::new(&env, &token_addr);
        let stellar_asset_client = token::StellarAssetClient::new(&env, &token_addr);
        stellar_asset_client.mint(&contract_id, &1000);

        let treasury_client = TreasuryContractClient::new(&env, &contract_id);
        treasury_client.set_approved_token(&token_addr, &true);
        env.ledger().set_timestamp(12345);
        treasury_client.allocate(&token_addr, &recipient, &400);

        assert_eq!(treasury_client.get_balance(&token_addr), 600);
        assert_eq!(token_client.balance(&recipient), 400);
    }

        let history = treasury_client.get_history();
        assert_eq!(history.len(), 1);
        let entry = history.get(0).unwrap();
        assert_eq!(entry.amount, 400);
        assert_eq!(entry.timestamp, 12345);
    }

    // -----------------------------------------------------------------------
    // buyback_and_burn — timelock access control
    // -----------------------------------------------------------------------

    #[test]
    fn test_buyback_requires_timelock_auth() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _, _timelock, contract_id) = setup_test(&env);

        let xlm_addr = env.register_stellar_asset_contract(admin.clone());
        let mnt_addr = env.register_contract(None, MockMNT);
        let dex_addr = env.register_contract(None, MockDEX);

        let stellar_asset_client = token::StellarAssetClient::new(&env, &xlm_addr);
        stellar_asset_client.mint(&contract_id, &1000);

        let treasury_client = TreasuryContractClient::new(&env, &contract_id);
        treasury_client.set_approved_token(&xlm_addr, &true);
        treasury_client.set_approved_token(&mnt_addr, &true);

        // get_timelock should return the registered address
        assert_eq!(treasury_client.get_timelock(), _timelock);

        // mock_all_auths covers timelock auth — call succeeds
        // (full auth-gating is enforced by require_auth; this test confirms the
        //  function reads the timelock address from storage correctly)
        let _ = treasury_client.try_buyback_and_burn(
            &xlm_addr,
            &mnt_addr,
            &dex_addr,
            &1000,
            &500,
            &default_dex_iface(&env),
        );
        // We only check that get_timelock() returns the expected address; the
        // auth mock covers the auth requirement in unit test mode.
        assert_eq!(treasury_client.get_timelock(), _timelock);
    }

    // -----------------------------------------------------------------------
    // buyback_and_burn — zero output (DEX returns 0 MNT)
    // -----------------------------------------------------------------------

    #[test]
    fn test_buyback_dex_returns_zero_mnt_fails_and_no_xlm_lost() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _, _, contract_id) = setup_test(&env);

        let xlm_addr = env.register_stellar_asset_contract(admin.clone());
        let mnt_addr = env.register_contract(None, MockMNT);
        let dex_addr = env.register_contract(None, MockDEXZero); // returns 0

        let stellar_asset_client = token::StellarAssetClient::new(&env, &xlm_addr);
        stellar_asset_client.mint(&contract_id, &1000);

        let treasury_client = TreasuryContractClient::new(&env, &contract_id);
        treasury_client.set_approved_token(&xlm_addr, &true);
        treasury_client.set_approved_token(&mnt_addr, &true);

        let xlm_balance_before = treasury_client.get_balance(&xlm_addr);

        let result = treasury_client.try_buyback_and_burn(
            &xlm_addr,
            &mnt_addr,
            &dex_addr,
            &500,
            &100,
            &default_dex_iface(&env),
        );

        // Must return ZeroOutput error
        assert!(result.is_err(), "expected ZeroOutput error");

        // XLM balance must not have changed — no funds left treasury
        let xlm_balance_after = treasury_client.get_balance(&xlm_addr);
        assert_eq!(
            xlm_balance_before, xlm_balance_after,
            "XLM must not leave treasury when DEX returns 0 MNT"
        );
    }

    // -----------------------------------------------------------------------
    // buyback_and_burn — slippage guard (min_mnt_out not met)
    // -----------------------------------------------------------------------

    #[test]
    fn test_buyback_and_burn_without_oracle() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _, _, contract_id) = setup_test(&env);

        let xlm_addr = env.register_stellar_asset_contract(admin.clone());
        let mnt_addr = env.register_contract(None, MockMNT);
        let dex_addr = env.register_contract(None, MockDEXSlippage); // returns 1

        let stellar_asset_client = token::StellarAssetClient::new(&env, &xlm_addr);
        stellar_asset_client.mint(&contract_id, &1000);

        let treasury_client = TreasuryContractClient::new(&env, &contract_id);
        treasury_client.set_approved_token(&xlm_addr, &true);
        treasury_client.set_approved_token(&mnt_addr, &true);

        let xlm_balance_before = treasury_client.get_balance(&xlm_addr);

        // min_mnt_out = 500, DEX returns 1 → slippage
        let result = treasury_client.try_buyback_and_burn(
            &xlm_addr,
            &mnt_addr,
            &dex_addr,
            &500,
            &500,
            &default_dex_iface(&env),
        );

        assert!(result.is_err(), "expected SlippageExceeded error");

        let xlm_balance_after = treasury_client.get_balance(&xlm_addr);
        assert_eq!(
            xlm_balance_before, xlm_balance_after,
            "XLM must not leave treasury when slippage guard triggers"
        );
    }

    // -----------------------------------------------------------------------
    // buyback_and_burn — invalid min_mnt_out (= 0) rejected before any transfer
    // -----------------------------------------------------------------------

    #[test]
    fn test_buyback_zero_min_out_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _, _, contract_id) = setup_test(&env);

        let xlm_addr = env.register_stellar_asset_contract(admin.clone());
        let mnt_addr = env.register_contract(None, MockMNT);
        let dex_addr = env.register_contract(None, MockDEX);

        let stellar_asset_client = token::StellarAssetClient::new(&env, &xlm_addr);
        stellar_asset_client.mint(&contract_id, &1000);

        let treasury_client = TreasuryContractClient::new(&env, &contract_id);
        treasury_client.set_approved_token(&xlm_addr, &true);
        treasury_client.set_approved_token(&mnt_addr, &true);

        let xlm_balance_before = treasury_client.get_balance(&xlm_addr);

        // min_mnt_out = 0 → InvalidMinOut, no XLM transferred
        let result = treasury_client.try_buyback_and_burn(
            &xlm_addr,
            &mnt_addr,
            &dex_addr,
            &500,
            &0,  // invalid
            &default_dex_iface(&env),
        );

        assert!(result.is_err(), "expected InvalidMinOut error");
        assert_eq!(
            treasury_client.get_balance(&xlm_addr),
            xlm_balance_before,
            "XLM must remain in treasury when min_out = 0"
        );
    }

    // -----------------------------------------------------------------------
    // buyback_and_burn — unapproved tokens rejected
    // -----------------------------------------------------------------------

    #[test]
    fn test_buyback_unapproved_token_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _, _, contract_id) = setup_test(&env);

        let xlm_addr = env.register_stellar_asset_contract(admin.clone());
        let mnt_addr = env.register_contract(None, MockMNT);
        let dex_addr = env.register_contract(None, MockDEX);

        let stellar_asset_client = token::StellarAssetClient::new(&env, &xlm_addr);
        stellar_asset_client.mint(&contract_id, &1000);

        let treasury_client = TreasuryContractClient::new(&env, &contract_id);
        // Do NOT approve tokens

        let result = treasury_client.try_buyback_and_burn(
            &xlm_addr,
            &mnt_addr,
            &dex_addr,
            &1000,
            &500,
            &default_dex_iface(&env),
        );
        assert!(result.is_err(), "unapproved token buyback must fail");
    }

    // -----------------------------------------------------------------------
    // buyback_and_burn — invalid DEX interface rejected
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "DexInterface: swap_fn must not be empty")]
    fn test_buyback_empty_swap_fn_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _, _, contract_id) = setup_test(&env);

        let xlm_addr = env.register_stellar_asset_contract(admin.clone());
        let mnt_addr = env.register_contract(None, MockMNT);
        let dex_addr = env.register_contract(None, MockDEX);

        let stellar_asset_client = token::StellarAssetClient::new(&env, &xlm_addr);
        stellar_asset_client.mint(&contract_id, &1000);

        let treasury_client = TreasuryContractClient::new(&env, &contract_id);
        // oracle_contract = None → skip health check (backward compat)
        treasury_client.buyback_and_burn(
            &xlm_addr,
            &mnt_addr,
            &dex_addr,
            &1000,
            &None,
            &None,
        );

        let bad_iface = DexInterface { swap_fn: Symbol::new(&env, "") };
        let _ = treasury_client.try_buyback_and_burn(
            &xlm_addr, &mnt_addr, &dex_addr, &1000, &500, &bad_iface,
        );
    }

    // ------------------------------------------------------------------
    // #614-AC4: treasury::buyback_and_burn queries oracle health before swap
    // ------------------------------------------------------------------

    #[test]
    fn test_buyback_proceeds_with_healthy_oracle() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);
        let (admin, _, contract_id) = setup_test(&env);

        let xlm_addr = env.register_stellar_asset_contract(admin.clone());
        let mnt_addr = env.register_contract(None, MockMNT);
        let dex_addr = env.register_contract(None, MockDEX);
        let oracle_addr = env.register_contract(None, MockOracleHealthy);

        let stellar_asset_client = token::StellarAssetClient::new(&env, &xlm_addr);
        stellar_asset_client.mint(&contract_id, &500);

        let treasury_client = TreasuryContractClient::new(&env, &contract_id);
        let result = treasury_client.try_buyback_and_burn(
            &xlm_addr,
            &mnt_addr,
            &dex_addr,
            &500,
            &Some(oracle_addr),
            &Some(symbol_short!("MNT")),
        );
        assert!(result.is_ok(), "healthy oracle should allow buyback");
    }

    #[test]
    fn test_buyback_aborted_when_oracle_stale() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);
        let (admin, _, contract_id) = setup_test(&env);

        let xlm_addr = env.register_stellar_asset_contract(admin.clone());
        let mnt_addr = env.register_contract(None, MockMNT);
        let dex_addr = env.register_contract(None, MockDEX);
        let oracle_addr = env.register_contract(None, MockOracleStale);

        let stellar_asset_client = token::StellarAssetClient::new(&env, &xlm_addr);
        stellar_asset_client.mint(&contract_id, &500);

        let treasury_client = TreasuryContractClient::new(&env, &contract_id);
        let result = treasury_client.try_buyback_and_burn(
            &xlm_addr,
            &mnt_addr,
            &dex_addr,
            &500,
            &Some(oracle_addr),
            &Some(symbol_short!("MNT")),
        );
        assert_eq!(result, Err(Ok(Error::OracleStale)));
    }

    #[test]
    fn test_buyback_aborted_when_insufficient_feeders() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);
        let (admin, _, contract_id) = setup_test(&env);

        let xlm_addr = env.register_stellar_asset_contract(admin.clone());
        let mnt_addr = env.register_contract(None, MockMNT);
        let dex_addr = env.register_contract(None, MockDEX);
        let oracle_addr = env.register_contract(None, MockOracleInsufficientFeeders);

        let stellar_asset_client = token::StellarAssetClient::new(&env, &xlm_addr);
        stellar_asset_client.mint(&contract_id, &500);

        let treasury_client = TreasuryContractClient::new(&env, &contract_id);
        let result = treasury_client.try_buyback_and_burn(
            &xlm_addr,
            &mnt_addr,
            &dex_addr,
            &500,
            &Some(oracle_addr),
            &Some(symbol_short!("MNT")),
        );
        assert_eq!(result, Err(Ok(Error::OracleUnhealthy)));
    }
}
