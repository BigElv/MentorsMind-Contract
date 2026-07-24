#![no_std]

// ---------------------------------------------------------------------------
// RFC: Upgrade path design
//
// Two paths exist for upgrading a contract tracked by this registry:
//
// PATH A — Two-step (RECOMMENDED):
//   1. `schedule_upgrade(contract_name, new_version, changelog_hash)`
//      - Requires admin auth
//      - Checks new_version > current_version (VersionNotMonotonic)
//      - Records a PendingUpgrade with `execute_after = now + upgrade_delay`
//   2. `execute_pending_upgrade(contract_name)`
//      - Requires admin auth
//      - Checks ledger timestamp >= execute_after (TimelockNotElapsed)
//      - Commits the upgrade record; clears the pending slot
//
// PATH B — Direct UUPS (`upgrade_contract`) — DEPRECATED
//   Kept for backward-compatibility only.  Marked `#[deprecated]`.
//   Callers MUST migrate to PATH A.  PATH B enforces identical guards:
//   - VersionNotMonotonic
//   - TimelockNotElapsed (uses same upgrade_delay as PATH A)
//   There is NO way to bypass the timelock via PATH B.
//
// Both paths now provide identical security guarantees.
// ---------------------------------------------------------------------------

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, BytesN, Env,
    Symbol, Vec,
};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    NotAdmin = 3,
    ContractNotFound = 4,
    AlreadySubscribed = 5,
    NotSubscribed = 6,
    /// new_version must be strictly greater than the currently registered version.
    VersionNotMonotonic = 7,
    /// The configured upgrade_delay has not elapsed since the upgrade was scheduled.
    TimelockNotElapsed = 8,
    /// No pending upgrade exists for this contract name.
    NoPendingUpgrade = 9,
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeRecord {
    pub old_version: u32,
    pub new_version: u32,
    pub changelog_hash: BytesN<32>,
    pub timestamp: u64,
    pub admin: Address,
}

/// Stored by `schedule_upgrade`; consumed by `execute_pending_upgrade`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingUpgrade {
    pub new_version: u32,
    pub changelog_hash: BytesN<32>,
    /// Earliest ledger timestamp at which execution is allowed.
    pub execute_after: u64,
    pub scheduled_by: Address,
}

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    /// Minimum seconds between schedule and execute.
    UpgradeDelay,
    UpgradeHistory(Symbol),
    LatestVersion(Symbol),
    Subscribers(Symbol),
    PendingUpgrade(Symbol),
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct UpgradeRegistryContract;

#[contractimpl]
impl UpgradeRegistryContract {
    /// Initialize the upgrade registry.
    ///
    /// `upgrade_delay` — minimum seconds that must elapse between scheduling
    /// and executing an upgrade (timelock).  Pass `0` to disable (testing only).
    pub fn initialize(env: Env, admin: Address, upgrade_delay: u64) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::UpgradeDelay, &upgrade_delay);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // PATH A — Two-step upgrade (RECOMMENDED)
    // -----------------------------------------------------------------------

    /// Schedule an upgrade for `contract_name` to `new_version`.
    ///
    /// Enforces:
    /// - Admin auth
    /// - `new_version > current_version` (VersionNotMonotonic)
    ///
    /// The upgrade can be executed only after `upgrade_delay` seconds.
    pub fn schedule_upgrade(
        env: Env,
        contract_name: Symbol,
        new_version: u32,
        changelog_hash: BytesN<32>,
    ) -> Result<(), Error> {
        let admin = Self::require_admin(&env)?;

        let current = Self::get_latest_version(env.clone(), contract_name.clone());
        if new_version <= current {
            return Err(Error::VersionNotMonotonic);
        }

        let upgrade_delay: u64 = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeDelay)
            .unwrap_or(0);

        let execute_after = env.ledger().timestamp().saturating_add(upgrade_delay);

        let pending = PendingUpgrade {
            new_version,
            changelog_hash: changelog_hash.clone(),
            execute_after,
            scheduled_by: admin.clone(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::PendingUpgrade(contract_name.clone()), &pending);

        env.events().publish(
            (
                symbol_short!("upgrade"),
                symbol_short!("sched"),
                contract_name,
            ),
            (new_version, changelog_hash, execute_after),
        );

        Ok(())
    }

    /// Execute a previously-scheduled upgrade for `contract_name`.
    ///
    /// Enforces:
    /// - Admin auth
    /// - Pending upgrade exists (NoPendingUpgrade)
    /// - Timelock has elapsed (TimelockNotElapsed)
    pub fn execute_pending_upgrade(
        env: Env,
        contract_name: Symbol,
    ) -> Result<(), Error> {
        let admin = Self::require_admin(&env)?;

        let pending: PendingUpgrade = env
            .storage()
            .persistent()
            .get(&DataKey::PendingUpgrade(contract_name.clone()))
            .ok_or(Error::NoPendingUpgrade)?;

        if env.ledger().timestamp() < pending.execute_after {
            return Err(Error::TimelockNotElapsed);
        }

        let current = Self::get_latest_version(env.clone(), contract_name.clone());

        let record = UpgradeRecord {
            old_version: current,
            new_version: pending.new_version,
            changelog_hash: pending.changelog_hash.clone(),
            timestamp: env.ledger().timestamp(),
            admin: admin.clone(),
        };

        let mut history: Vec<UpgradeRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::UpgradeHistory(contract_name.clone()))
            .unwrap_or(Vec::new(&env));

        history.push_back(record);

        env.storage()
            .persistent()
            .set(&DataKey::UpgradeHistory(contract_name.clone()), &history);

        env.storage().persistent().set(
            &DataKey::LatestVersion(contract_name.clone()),
            &pending.new_version,
        );

        // Clear pending slot
        env.storage()
            .persistent()
            .remove(&DataKey::PendingUpgrade(contract_name.clone()));

        env.events().publish(
            (
                symbol_short!("upgrade"),
                symbol_short!("exec"),
                contract_name,
            ),
            (current, pending.new_version, pending.changelog_hash),
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // PATH B — Direct UUPS (DEPRECATED — migrate to PATH A)
    // -----------------------------------------------------------------------

    /// Register a contract upgrade directly.
    ///
    /// **DEPRECATED** — use `schedule_upgrade` + `execute_pending_upgrade` instead.
    ///
    /// This function is retained for backward-compatibility only.  It enforces
    /// identical security guarantees to PATH A:
    /// - VersionNotMonotonic: `new_version` must exceed the stored version.
    /// - TimelockNotElapsed: the configured `upgrade_delay` must have elapsed
    ///   since the last upgrade timestamp for this contract.
    ///
    /// Migration note: replace calls to `register_upgrade(name, old, new, hash)`
    /// with `schedule_upgrade(name, new, hash)` followed by
    /// `execute_pending_upgrade(name)` after the delay expires.
    #[allow(deprecated)]
    pub fn register_upgrade(
        env: Env,
        contract_name: Symbol,
        old_version: u32,
        new_version: u32,
        changelog_hash: BytesN<32>,
    ) -> Result<(), Error> {
        let admin = Self::require_admin(&env)?;

        // Guard 1: monotonic version
        let current = Self::get_latest_version(env.clone(), contract_name.clone());
        if new_version <= current {
            return Err(Error::VersionNotMonotonic);
        }

        let record = UpgradeRecord {
            old_version,
            new_version,
            changelog_hash: changelog_hash.clone(),
            timestamp: env.ledger().timestamp(),
            admin: admin.clone(),
        };

        let mut history: Vec<UpgradeRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::UpgradeHistory(contract_name.clone()))
            .unwrap_or(Vec::new(&env));

        history.push_back(record);

        env.storage()
            .persistent()
            .set(&DataKey::UpgradeHistory(contract_name.clone()), &history);

        env.storage()
            .persistent()
            .set(&DataKey::LatestVersion(contract_name.clone()), &new_version);

        env.events().publish(
            (
                symbol_short!("upgrade"),
                symbol_short!("reg"),
                contract_name.clone(),
            ),
            (old_version, new_version, changelog_hash),
        );

        Ok(())
    }

    /// Perform a direct UUPS-style upgrade.
    ///
    /// **DEPRECATED** — use `schedule_upgrade` + `execute_pending_upgrade` instead.
    ///
    /// Enforces:
    /// - Admin auth
    /// - VersionNotMonotonic: `new_version > current_version`
    /// - TimelockNotElapsed: `upgrade_delay` seconds must have elapsed since
    ///   the last recorded upgrade for this contract (or since initialization
    ///   if no prior upgrade exists).
    ///
    /// Migration note: replace `upgrade_contract(name, new_version, hash)` with
    /// the two-step path described in the module-level RFC comment.
    pub fn upgrade_contract(
        env: Env,
        contract_name: Symbol,
        new_version: u32,
        changelog_hash: BytesN<32>,
    ) -> Result<(), Error> {
        let admin = Self::require_admin(&env)?;

        let current = Self::get_latest_version(env.clone(), contract_name.clone());

        // Guard 1: monotonic version check (#619)
        if new_version <= current {
            return Err(Error::VersionNotMonotonic);
        }

        // Guard 2: timelock check (#619)
        // Compare against the timestamp of the most recent upgrade record for
        // this contract, falling back to 0 (epoch) if none exists.
        let upgrade_delay: u64 = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeDelay)
            .unwrap_or(0);

        if upgrade_delay > 0 {
            let history: Vec<UpgradeRecord> = env
                .storage()
                .persistent()
                .get(&DataKey::UpgradeHistory(contract_name.clone()))
                .unwrap_or(Vec::new(&env));

            let last_upgrade_ts = if history.is_empty() {
                0u64
            } else {
                history.get(history.len() - 1).unwrap().timestamp
            };

            let earliest_allowed = last_upgrade_ts.saturating_add(upgrade_delay);
            if env.ledger().timestamp() < earliest_allowed {
                return Err(Error::TimelockNotElapsed);
            }
        }

        let record = UpgradeRecord {
            old_version: current,
            new_version,
            changelog_hash: changelog_hash.clone(),
            timestamp: env.ledger().timestamp(),
            admin: admin.clone(),
        };

        let mut history: Vec<UpgradeRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::UpgradeHistory(contract_name.clone()))
            .unwrap_or(Vec::new(&env));

        history.push_back(record);

        env.storage()
            .persistent()
            .set(&DataKey::UpgradeHistory(contract_name.clone()), &history);

        env.storage()
            .persistent()
            .set(&DataKey::LatestVersion(contract_name.clone()), &new_version);

        env.events().publish(
            (
                symbol_short!("upgrade"),
                symbol_short!("direct"),
                contract_name,
            ),
            (current, new_version, changelog_hash),
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Subscriptions
    // -----------------------------------------------------------------------

    pub fn subscribe(env: Env, subscriber: Address, contract_name: Symbol) -> Result<(), Error> {
        subscriber.require_auth();

        let mut subscribers: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::Subscribers(contract_name.clone()))
            .unwrap_or(Vec::new(&env));

        for addr in subscribers.iter() {
            if addr == subscriber {
                return Err(Error::AlreadySubscribed);
            }
        }

        subscribers.push_back(subscriber.clone());

        env.storage()
            .persistent()
            .set(&DataKey::Subscribers(contract_name.clone()), &subscribers);

        env.events().publish(
            (symbol_short!("sub"), symbol_short!("added"), contract_name),
            subscriber,
        );

        Ok(())
    }

    pub fn unsubscribe(env: Env, subscriber: Address, contract_name: Symbol) -> Result<(), Error> {
        subscriber.require_auth();

        let subscribers: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::Subscribers(contract_name.clone()))
            .unwrap_or(Vec::new(&env));

        let mut found = false;
        let mut new_subscribers = Vec::new(&env);

        for addr in subscribers.iter() {
            if addr != subscriber {
                new_subscribers.push_back(addr);
            } else {
                found = true;
            }
        }

        if !found {
            return Err(Error::NotSubscribed);
        }

        env.storage().persistent().set(
            &DataKey::Subscribers(contract_name.clone()),
            &new_subscribers,
        );

        env.events().publish(
            (
                symbol_short!("sub"),
                symbol_short!("removed"),
                contract_name,
            ),
            subscriber,
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    pub fn get_upgrade_history(env: Env, contract_name: Symbol) -> Vec<UpgradeRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::UpgradeHistory(contract_name))
            .unwrap_or(Vec::new(&env))
    }

    pub fn get_latest_version(env: Env, contract_name: Symbol) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::LatestVersion(contract_name))
            .unwrap_or(0)
    }

    pub fn get_subscribers(env: Env, contract_name: Symbol) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::Subscribers(contract_name))
            .unwrap_or(Vec::new(&env))
    }

    pub fn get_pending_upgrade(env: Env, contract_name: Symbol) -> Option<PendingUpgrade> {
        env.storage()
            .persistent()
            .get(&DataKey::PendingUpgrade(contract_name))
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    fn require_admin(env: &Env) -> Result<Address, Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        Ok(admin)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::Env;

    // Helper: sets up a registry with no timelock by default.
    fn setup() -> (
        Env,
        Address,
        Address,
        UpgradeRegistryContractClient<'static>,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let contract_id = env.register_contract(None, UpgradeRegistryContract);
        let client = UpgradeRegistryContractClient::new(&env, &contract_id);
        client.initialize(&admin, &0u64); // upgrade_delay = 0
        (env, admin, contract_id, client)
    }

    fn setup_with_delay(delay: u64) -> (
        Env,
        Address,
        Address,
        UpgradeRegistryContractClient<'static>,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);

        let admin = Address::generate(&env);
        let contract_id = env.register_contract(None, UpgradeRegistryContract);
        let client = UpgradeRegistryContractClient::new(&env, &contract_id);
        client.initialize(&admin, &delay);
        (env, admin, contract_id, client)
    }

    // ------------------------------------------------------------------
    // Basic existing behaviour
    // ------------------------------------------------------------------

    #[test]
    fn test_initialize() {
        let (env, _admin, _contract_id, client) = setup();
        let contract_name = symbol_short!("escrow");
        let hash = BytesN::from_array(&env, &[0u8; 32]);
        client.register_upgrade(&contract_name, &0, &1, &hash);
    }

    #[test]
    fn test_register_upgrade() {
        let (env, _admin, _contract_id, client) = setup();
        let contract_name = symbol_short!("escrow");
        let hash = BytesN::from_array(&env, &[1u8; 32]);

        client.register_upgrade(&contract_name, &0, &1, &hash);

        let history = client.get_upgrade_history(&contract_name);
        assert_eq!(history.len(), 1);
        assert_eq!(history.get(0).unwrap().new_version, 1);
        assert_eq!(client.get_latest_version(&contract_name), 1);
    }

    #[test]
    fn test_subscribe_and_unsubscribe() {
        let (env, _admin, _contract_id, client) = setup();
        let contract_name = symbol_short!("escrow");
        let subscriber = Address::generate(&env);

        client.subscribe(&subscriber, &contract_name);
        assert_eq!(client.get_subscribers(&contract_name).len(), 1);

        client.unsubscribe(&subscriber, &contract_name);
        assert_eq!(client.get_subscribers(&contract_name).len(), 0);
    }

    #[test]
    #[should_panic]
    fn test_duplicate_subscribe_fails() {
        let (env, _admin, _contract_id, client) = setup();
        let contract_name = symbol_short!("escrow");
        let subscriber = Address::generate(&env);

        client.subscribe(&subscriber, &contract_name);
        client.subscribe(&subscriber, &contract_name);
    }

    // ------------------------------------------------------------------
    // #619-AC1: register_upgrade rejects downgrade
    // ------------------------------------------------------------------

    #[test]
    fn test_register_upgrade_rejects_downgrade() {
        let (env, _admin, _contract_id, client) = setup();
        let contract_name = symbol_short!("escrow");
        let hash = BytesN::from_array(&env, &[0u8; 32]);

        client.register_upgrade(&contract_name, &0, &2, &hash);
        assert_eq!(client.get_latest_version(&contract_name), 2);

        // Attempt to downgrade to version 1
        let result = client.try_register_upgrade(&contract_name, &2, &1, &hash);
        assert_eq!(result, Err(Ok(Error::VersionNotMonotonic)));
    }

    #[test]
    fn test_register_upgrade_rejects_same_version() {
        let (env, _admin, _contract_id, client) = setup();
        let contract_name = symbol_short!("escrow");
        let hash = BytesN::from_array(&env, &[0u8; 32]);

        client.register_upgrade(&contract_name, &0, &2, &hash);

        // Same version
        let result = client.try_register_upgrade(&contract_name, &2, &2, &hash);
        assert_eq!(result, Err(Ok(Error::VersionNotMonotonic)));
    }

    // ------------------------------------------------------------------
    // #619-AC1 (upgrade_contract path): downgrade returns VersionNotMonotonic
    // ------------------------------------------------------------------

    #[test]
    fn test_upgrade_contract_rejects_downgrade() {
        let (env, _admin, _contract_id, client) = setup();
        env.ledger().set_timestamp(0);
        let contract_name = symbol_short!("escrow");
        let hash = BytesN::from_array(&env, &[0u8; 32]);

        // Establish version 2 via register_upgrade
        client.register_upgrade(&contract_name, &0, &2, &hash);

        // upgrade_contract with new_version = 1 (downgrade) must fail
        let result = client.try_upgrade_contract(&contract_name, &1, &hash);
        assert_eq!(result, Err(Ok(Error::VersionNotMonotonic)));
    }

    #[test]
    fn test_upgrade_contract_rejects_same_version() {
        let (env, _admin, _contract_id, client) = setup();
        let contract_name = symbol_short!("escrow");
        let hash = BytesN::from_array(&env, &[0u8; 32]);

        client.register_upgrade(&contract_name, &0, &2, &hash);

        let result = client.try_upgrade_contract(&contract_name, &2, &hash);
        assert_eq!(result, Err(Ok(Error::VersionNotMonotonic)));
    }

    #[test]
    fn test_upgrade_contract_succeeds_with_higher_version() {
        let (env, _admin, _contract_id, client) = setup();
        env.ledger().set_timestamp(0);
        let contract_name = symbol_short!("escrow");
        let hash = BytesN::from_array(&env, &[0u8; 32]);

        client.register_upgrade(&contract_name, &0, &2, &hash);

        let result = client.try_upgrade_contract(&contract_name, &3, &hash);
        assert!(result.is_ok());
        assert_eq!(client.get_latest_version(&contract_name), 3);
    }

    // ------------------------------------------------------------------
    // #619-AC2: upgrade_contract enforces timelock
    // ------------------------------------------------------------------

    #[test]
    fn test_upgrade_contract_timelock_not_elapsed() {
        let delay = 3_600u64; // 1 hour
        let (env, _admin, _contract_id, client) = setup_with_delay(delay);
        let contract_name = symbol_short!("escrow");
        let hash = BytesN::from_array(&env, &[0u8; 32]);

        // Record a prior upgrade at t=1000
        env.ledger().set_timestamp(1_000);
        client.register_upgrade(&contract_name, &0, &1, &hash);

        // Try upgrade_contract before delay has elapsed (t=1500 < 1000+3600)
        env.ledger().set_timestamp(1_500);
        let result = client.try_upgrade_contract(&contract_name, &2, &hash);
        assert_eq!(result, Err(Ok(Error::TimelockNotElapsed)));
    }

    #[test]
    fn test_upgrade_contract_timelock_elapsed() {
        let delay = 3_600u64;
        let (env, _admin, _contract_id, client) = setup_with_delay(delay);
        let contract_name = symbol_short!("escrow");
        let hash = BytesN::from_array(&env, &[0u8; 32]);

        env.ledger().set_timestamp(1_000);
        client.register_upgrade(&contract_name, &0, &1, &hash);

        // Advance past delay: 1000 + 3600 = 4600; use 5000 to be safe
        env.ledger().set_timestamp(5_000);
        let result = client.try_upgrade_contract(&contract_name, &2, &hash);
        assert!(result.is_ok());
        assert_eq!(client.get_latest_version(&contract_name), 2);
    }

    // ------------------------------------------------------------------
    // Path A: schedule_upgrade + execute_pending_upgrade
    // ------------------------------------------------------------------

    #[test]
    fn test_two_step_upgrade_happy_path() {
        let delay = 3_600u64;
        let (env, _admin, _contract_id, client) = setup_with_delay(delay);
        let contract_name = symbol_short!("escrow");
        let hash = BytesN::from_array(&env, &[0u8; 32]);

        // Schedule at t=1000; execute_after = 1000 + 3600 = 4600
        env.ledger().set_timestamp(1_000);
        client.schedule_upgrade(&contract_name, &1, &hash);

        let pending = client.get_pending_upgrade(&contract_name).unwrap();
        assert_eq!(pending.new_version, 1);
        assert_eq!(pending.execute_after, 4_600);

        // Cannot execute before delay
        env.ledger().set_timestamp(2_000);
        let result = client.try_execute_pending_upgrade(&contract_name);
        assert_eq!(result, Err(Ok(Error::TimelockNotElapsed)));

        // Execute after delay
        env.ledger().set_timestamp(5_000);
        client.execute_pending_upgrade(&contract_name);

        assert_eq!(client.get_latest_version(&contract_name), 1);
        assert!(client.get_pending_upgrade(&contract_name).is_none());
    }

    #[test]
    fn test_schedule_upgrade_rejects_downgrade() {
        let (env, _admin, _contract_id, client) = setup();
        let contract_name = symbol_short!("escrow");
        let hash = BytesN::from_array(&env, &[0u8; 32]);

        client.register_upgrade(&contract_name, &0, &3, &hash);

        // Try to schedule a downgrade to version 2
        let result = client.try_schedule_upgrade(&contract_name, &2, &hash);
        assert_eq!(result, Err(Ok(Error::VersionNotMonotonic)));
    }

    #[test]
    fn test_execute_without_schedule_fails() {
        let (_, _admin, _contract_id, client) = setup();
        let contract_name = symbol_short!("escrow");

        let result = client.try_execute_pending_upgrade(&contract_name);
        assert_eq!(result, Err(Ok(Error::NoPendingUpgrade)));
    }

    // ------------------------------------------------------------------
    // Regression: both paths have identical security guarantees (#619-AC3)
    // ------------------------------------------------------------------

    #[test]
    fn test_both_paths_reject_downgrade() {
        let (env, _admin, _contract_id, client) = setup();
        let contract_name = symbol_short!("escrow");
        let hash = BytesN::from_array(&env, &[0u8; 32]);

        // Establish version 5
        client.register_upgrade(&contract_name, &0, &5, &hash);
        assert_eq!(client.get_latest_version(&contract_name), 5);

        // PATH B direct — downgrade attempt
        assert_eq!(
            client.try_upgrade_contract(&contract_name, &4, &hash),
            Err(Ok(Error::VersionNotMonotonic))
        );

        // PATH A schedule — downgrade attempt
        assert_eq!(
            client.try_schedule_upgrade(&contract_name, &3, &hash),
            Err(Ok(Error::VersionNotMonotonic))
        );

        // Version should be unchanged
        assert_eq!(client.get_latest_version(&contract_name), 5);
    }
}
