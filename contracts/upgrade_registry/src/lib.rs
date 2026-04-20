#![no_std]

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
}

// ---------------------------------------------------------------------------
// Data Types
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

// ---------------------------------------------------------------------------
// Storage Keys
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    UpgradeHistory(Symbol),
    LatestVersion(Symbol),
    Subscribers(Symbol),
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct UpgradeRegistryContract;

#[contractimpl]
impl UpgradeRegistryContract {
    /// Initialize the upgrade registry contract.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    /// Register a contract upgrade.
    /// Admin only.
    pub fn register_upgrade(
        env: Env,
        contract_name: Symbol,
        old_version: u32,
        new_version: u32,
        changelog_hash: BytesN<32>,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;

        admin.require_auth();

        let record = UpgradeRecord {
            old_version,
            new_version,
            changelog_hash: changelog_hash.clone(),
            timestamp: env.ledger().timestamp(),
            admin: admin.clone(),
        };

        // Get or create history vector
        let mut history: Vec<UpgradeRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::UpgradeHistory(contract_name.clone()))
            .unwrap_or(Vec::new(&env));

        history.push_back(record);

        // Store updated history
        env.storage()
            .persistent()
            .set(&DataKey::UpgradeHistory(contract_name.clone()), &history);

        // Update latest version
        env.storage()
            .persistent()
            .set(&DataKey::LatestVersion(contract_name.clone()), &new_version);

        // Emit event for backend indexer
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

    /// Subscribe to upgrade notifications for a specific contract.
    pub fn subscribe(env: Env, subscriber: Address, contract_name: Symbol) -> Result<(), Error> {
        subscriber.require_auth();

        let mut subscribers: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::Subscribers(contract_name.clone()))
            .unwrap_or(Vec::new(&env));

        // Check if already subscribed
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

    /// Unsubscribe from upgrade notifications.
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

    /// Get upgrade history for a contract.
    pub fn get_upgrade_history(env: Env, contract_name: Symbol) -> Vec<UpgradeRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::UpgradeHistory(contract_name))
            .unwrap_or(Vec::new(&env))
    }

    /// Get the latest version for a contract.
    pub fn get_latest_version(env: Env, contract_name: Symbol) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::LatestVersion(contract_name))
            .unwrap_or(0)
    }

    /// Get all subscribers for a contract.
    pub fn get_subscribers(env: Env, contract_name: Symbol) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::Subscribers(contract_name))
            .unwrap_or(Vec::new(&env))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

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

        client.initialize(&admin);

        (env, admin, contract_id, client)
    }

    #[test]
    fn test_initialize() {
        let (env, _admin, _contract_id, client) = setup();

        // Verify initialization worked by trying to register an upgrade
        let contract_name = symbol_short!("escrow");
        let hash = BytesN::from_array(&env, &[0u8; 32]);
        client.register_upgrade(&contract_name, &1, &2, &hash);
    }

    #[test]
    fn test_register_upgrade() {
        let (env, _admin, _contract_id, client) = setup();

        let contract_name = symbol_short!("escrow");
        let hash = BytesN::from_array(&env, &[1u8; 32]);

        client.register_upgrade(&contract_name, &1, &2, &hash);

        let history = client.get_upgrade_history(&contract_name);
        assert_eq!(history.len(), 1);

        let record = history.get(0).unwrap();
        assert_eq!(record.old_version, 1);
        assert_eq!(record.new_version, 2);

        assert_eq!(client.get_latest_version(&contract_name), 2);
    }

    #[test]
    fn test_subscribe() {
        let (env, _admin, _contract_id, client) = setup();

        let contract_name = symbol_short!("escrow");
        let subscriber = Address::generate(&env);

        client.subscribe(&subscriber, &contract_name);

        let subscribers = client.get_subscribers(&contract_name);
        assert_eq!(subscribers.len(), 1);
        assert_eq!(subscribers.get(0).unwrap(), subscriber);
    }

    #[test]
    fn test_unsubscribe() {
        let (env, _admin, _contract_id, client) = setup();

        let contract_name = symbol_short!("escrow");
        let subscriber = Address::generate(&env);

        client.subscribe(&subscriber, &contract_name);
        client.unsubscribe(&subscriber, &contract_name);

        let subscribers = client.get_subscribers(&contract_name);
        assert_eq!(subscribers.len(), 0);
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
}
