#![no_std]

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
    NotInitialized = 2,
    Unauthorized = 3,
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
    pub token: Address,
    pub recipient: Address,
    pub amount: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    StakingContract,
    History,
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
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::StakingContract, &staking_contract);

        let empty_history: Vec<AllocationHistory> = Vec::new(&env);
        env.storage()
            .persistent()
            .set(&DataKey::History, &empty_history);

        Ok(())
    }

    /// Accept deposits of any approved Stellar asset.
    pub fn deposit(env: Env, from: Address, token: Address, amount: i128) -> Result<(), Error> {
        from.require_auth();

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
        let token_client = token::Client::new(&env, &token);
        token_client.balance(&env.current_contract_address())
    }

    /// allocate — governance/timelock only; transfers `amount` of `token` to `recipient`.
    pub fn allocate(
        env: Env,
        token: Address,
        recipient: Address,
        amount: i128,
    ) -> Result<(), Error> {
        let admin = env
            .storage()
            .persistent()
            .get::<DataKey, Address>(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&env.current_contract_address(), &recipient, &amount);

        let mut history = env
            .storage()
            .persistent()
            .get::<DataKey, Vec<AllocationHistory>>(&DataKey::History)
            .unwrap_or_else(|| Vec::new(&env));

        history.push_back(AllocationHistory {
            token: token.clone(),
            recipient: recipient.clone(),
            amount,
            timestamp: env.ledger().timestamp(),
        });
        env.storage().persistent().set(&DataKey::History, &history);

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
        let admin = env
            .storage()
            .persistent()
            .get::<DataKey, Address>(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        let staking_contract = env
            .storage()
            .persistent()
            .get::<DataKey, Address>(&DataKey::StakingContract)
            .ok_or(Error::NotInitialized)?;

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(
            &env.current_contract_address(),
            &staking_contract,
            &total_amount,
        );

        env.invoke_contract::<()>(
            &staking_contract,
            &Symbol::new(&env, "distribute_revenue"),
            (token.clone(), total_amount).into_val(&env),
        );

        env.events().publish(
            (
                symbol_short!("distrib"),
                staking_contract.clone(),
                token.clone(),
            ),
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
        xlm_token: Address,
        mnt_token: Address,
        dex_contract: Address,
        xlm_amount: i128,
        oracle_contract: Option<Address>,
        mnt_asset_symbol: Option<Symbol>,
    ) -> Result<(), Error> {
        let admin = env
            .storage()
            .persistent()
            .get::<DataKey, Address>(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

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
        xlm_client.transfer(&env.current_contract_address(), &dex_contract, &xlm_amount);

        // 2. Call DEX swap — returns the amount of MNT received
        let mnt_received: i128 = env.invoke_contract(
            &dex_contract,
            &Symbol::new(&env, "swap"),
            (xlm_token.clone(), mnt_token.clone(), xlm_amount).into_val(&env),
        );

        // 3. Burn MNT
        env.invoke_contract::<()>(
            &mnt_token,
            &Symbol::new(&env, "burn"),
            (env.current_contract_address(), mnt_received).into_val(&env),
        );

        env.events()
            .publish((symbol_short!("buyback"), mnt_token.clone()), mnt_received);

        Ok(())
    }

    pub fn get_history(env: Env) -> Vec<AllocationHistory> {
        env.storage()
            .persistent()
            .get::<DataKey, Vec<AllocationHistory>>(&DataKey::History)
            .unwrap_or_else(|| Vec::new(&env))
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
        pub fn swap(_env: Env, _token_in: Address, _token_out: Address, amount_in: i128) -> i128 {
            amount_in
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
        let (admin, _, contract_id) = setup_test(&env);
        let user = Address::generate(&env);
        let token_addr = env.register_stellar_asset_contract(admin.clone());
        let token_client = token::Client::new(&env, &token_addr);
        let stellar_asset_client = token::StellarAssetClient::new(&env, &token_addr);

        stellar_asset_client.mint(&user, &1000);

        let treasury_client = TreasuryContractClient::new(&env, &contract_id);
        treasury_client.deposit(&user, &token_addr, &500);

        assert_eq!(treasury_client.get_balance(&token_addr), 500);
        assert_eq!(token_client.balance(&user), 500);
        assert_eq!(token_client.balance(&contract_id), 500);
    }

    #[test]
    fn test_allocate() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _, contract_id) = setup_test(&env);
        let recipient = Address::generate(&env);
        let token_addr = env.register_stellar_asset_contract(admin.clone());
        let token_client = token::Client::new(&env, &token_addr);
        let stellar_asset_client = token::StellarAssetClient::new(&env, &token_addr);

        stellar_asset_client.mint(&contract_id, &1000);

        let treasury_client = TreasuryContractClient::new(&env, &contract_id);

        env.ledger().set_timestamp(12345);
        treasury_client.allocate(&token_addr, &recipient, &400);

        assert_eq!(treasury_client.get_balance(&token_addr), 600);
        assert_eq!(token_client.balance(&recipient), 400);

        let history = treasury_client.get_history();
        assert_eq!(history.len(), 1);
        let entry = history.get(0).unwrap();
        assert_eq!(entry.amount, 400);
        assert_eq!(entry.timestamp, 12345);
    }

    #[test]
    fn test_distribute_to_stakers() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, staking_addr, contract_id) = setup_test(&env);
        let token_addr = env.register_stellar_asset_contract(admin.clone());
        let token_client = token::Client::new(&env, &token_addr);
        let stellar_asset_client = token::StellarAssetClient::new(&env, &token_addr);

        stellar_asset_client.mint(&contract_id, &1000);

        let treasury_client = TreasuryContractClient::new(&env, &contract_id);
        treasury_client.distribute_to_stakers(&token_addr, &300);

        assert_eq!(treasury_client.get_balance(&token_addr), 700);
        assert_eq!(token_client.balance(&staking_addr), 300);
    }

    #[test]
    fn test_buyback_and_burn_without_oracle() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _, contract_id) = setup_test(&env);

        let xlm_addr = env.register_stellar_asset_contract(admin.clone());
        let mnt_addr = env.register_contract(None, MockMNT);
        let dex_addr = env.register_contract(None, MockDEX);

        let xlm_client = token::Client::new(&env, &xlm_addr);
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

        assert_eq!(xlm_client.balance(&contract_id), 0);
        assert_eq!(xlm_client.balance(&dex_addr), 1000);
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
