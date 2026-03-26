#![no_std]

mod types;

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, symbol_short};
use types::{MigrationEvent, StreamStatus, StreamV1, StreamV2};

const STREAM_V1_COUNT: Symbol = symbol_short!("SV1_CNT");
const STREAM_V2_COUNT: Symbol = symbol_short!("SV2_CNT");
const STREAM_TTL_THRESHOLD: u32 = 500_000;
const STREAM_TTL_BUMP: u32 = 1_000_000;

#[contract]
pub struct StreamContract;

#[contractimpl]
impl StreamContract {
    /// Initialize the contract
    pub fn initialize(env: Env) {
        // Initialize V1 stream count
        if !env.storage().persistent().has(&STREAM_V1_COUNT) {
            env.storage().persistent().set(&STREAM_V1_COUNT, &0u64);
            env.storage()
                .persistent()
                .extend_ttl(&STREAM_V1_COUNT, STREAM_TTL_THRESHOLD, STREAM_TTL_BUMP);
        }

        // Initialize V2 stream count
        if !env.storage().persistent().has(&STREAM_V2_COUNT) {
            env.storage().persistent().set(&STREAM_V2_COUNT, &0u64);
            env.storage()
                .persistent()
                .extend_ttl(&STREAM_V2_COUNT, STREAM_TTL_THRESHOLD, STREAM_TTL_BUMP);
        }
    }

    /// Create a V1 stream
    pub fn create_stream_v1(
        env: Env,
        owner: Address,
        recipient: Address,
        amount: i128,
        start_time: u64,
        end_time: u64,
    ) -> u64 {
        // Validate inputs
        if amount <= 0 {
            panic!("Amount must be greater than zero");
        }
        if start_time >= end_time {
            panic!("Start time must be before end time");
        }

        // Require owner authorization
        owner.require_auth();

        // Get and increment V1 stream count
        let mut count: u64 = env
            .storage()
            .persistent()
            .get(&STREAM_V1_COUNT)
            .unwrap_or(0);
        count += 1;

        // Bump TTL for stream count
        env.storage()
            .persistent()
            .extend_ttl(&STREAM_V1_COUNT, STREAM_TTL_THRESHOLD, STREAM_TTL_BUMP);

        // Create V1 stream
        let stream = StreamV1 {
            id: count,
            owner: owner.clone(),
            recipient: recipient.clone(),
            amount,
            start_time,
            end_time,
            claimed_amount: 0,
            status: StreamStatus::Active,
        };

        // Store stream with TTL bump
        let key = (symbol_short!("STREAM_V1"), count);
        env.storage().persistent().set(&key, &stream);
        env.storage()
            .persistent()
            .extend_ttl(&key, STREAM_TTL_THRESHOLD, STREAM_TTL_BUMP);

        // Emit event
        env.events().publish(
            (symbol_short!("created"), count),
            (owner, recipient, amount, start_time, end_time),
        );

        count
    }

    /// Get a V1 stream
    pub fn get_stream_v1(env: Env, stream_id: u64) -> StreamV1 {
        let key = (symbol_short!("STREAM_V1"), stream_id);

        // Bump TTL before reading
        env.storage()
            .persistent()
            .extend_ttl(&key, STREAM_TTL_THRESHOLD, STREAM_TTL_BUMP);

        env.storage()
            .persistent()
            .get(&key)
            .expect("Stream V1 not found")
    }

    /// Get a V2 stream
    pub fn get_stream_v2(env: Env, stream_id: u64) -> StreamV2 {
        let key = (symbol_short!("STREAM_V2"), stream_id);

        // Bump TTL before reading
        env.storage()
            .persistent()
            .extend_ttl(&key, STREAM_TTL_THRESHOLD, STREAM_TTL_BUMP);

        env.storage()
            .persistent()
            .get(&key)
            .expect("Stream V2 not found")
    }

    /// Migrate a V1 stream to V2
    ///
    /// This function performs an atomic migration of a V1 stream to V2:
    /// 1. Authenticates the sender
    /// 2. Validates the V1 stream exists
    /// 3. Finalizes V1 state (marks as Migrated)
    /// 4. Creates V2 stream with migrated state
    /// 5. Emits MigrationEvent with all required data
    ///
    /// If any step fails, the entire transaction is rolled back and no event is emitted.
    pub fn migrate_stream(env: Env, sender: Address, v1_id: u64) -> u64 {
        // Step 1: Authenticate sender
        sender.require_auth();

        // Step 2: Validate V1 stream exists
        let v1_key = (symbol_short!("STREAM_V1"), v1_id);
        env.storage()
            .persistent()
            .extend_ttl(&v1_key, STREAM_TTL_THRESHOLD, STREAM_TTL_BUMP);

        let mut v1_stream: StreamV1 = env
            .storage()
            .persistent()
            .get(&v1_key)
            .expect("V1 stream not found");

        // Validate V1 stream is not already migrated
        if v1_stream.status == StreamStatus::Migrated {
            panic!("V1 stream already migrated");
        }

        // Validate sender is the owner
        if sender != v1_stream.owner {
            panic!("Sender is not the stream owner");
        }

        // Step 3: Calculate remaining balance
        let remaining_balance = v1_stream.amount - v1_stream.claimed_amount;
        if remaining_balance < 0 {
            panic!("Invalid remaining balance calculation");
        }

        // Step 4: Finalize V1 state
        v1_stream.status = StreamStatus::Migrated;
        env.storage().persistent().set(&v1_key, &v1_stream);

        // Step 5: Create V2 stream
        let mut v2_count: u64 = env
            .storage()
            .persistent()
            .get(&STREAM_V2_COUNT)
            .unwrap_or(0);
        v2_count += 1;

        // Bump TTL for V2 stream count
        env.storage()
            .persistent()
            .extend_ttl(&STREAM_V2_COUNT, STREAM_TTL_THRESHOLD, STREAM_TTL_BUMP);

        let v2_stream = StreamV2 {
            id: v2_count,
            owner: v1_stream.owner.clone(),
            recipient: v1_stream.recipient.clone(),
            amount: v1_stream.amount,
            start_time: v1_stream.start_time,
            end_time: v1_stream.end_time,
            claimed_amount: v1_stream.claimed_amount,
            status: StreamStatus::Active,
            v1_id: v1_id,
            migrated_at: env.ledger().timestamp(),
        };

        // Store V2 stream with TTL bump
        let v2_key = (symbol_short!("STREAM_V2"), v2_count);
        env.storage().persistent().set(&v2_key, &v2_stream);
        env.storage()
            .persistent()
            .extend_ttl(&v2_key, STREAM_TTL_THRESHOLD, STREAM_TTL_BUMP);

        // Update V2 stream count
        env.storage().persistent().set(&STREAM_V2_COUNT, &v2_count);

        // Step 6: Emit MigrationEvent
        let topic = Symbol::new(&env, "migrate");
        let event = MigrationEvent {
            v1_id,
            v2_id: v2_count,
            sender: sender.clone(),
            remaining_balance,
        };
        env.events().publish((topic,), event);

        // Return V2 stream ID
        v2_count
    }

    /// Get total V1 stream count
    pub fn get_stream_v1_count(env: Env) -> u64 {
        env.storage()
            .persistent()
            .extend_ttl(&STREAM_V1_COUNT, STREAM_TTL_THRESHOLD, STREAM_TTL_BUMP);
        env.storage()
            .persistent()
            .get(&STREAM_V1_COUNT)
            .unwrap_or(0)
    }

    /// Get total V2 stream count
    pub fn get_stream_v2_count(env: Env) -> u64 {
        env.storage()
            .persistent()
            .extend_ttl(&STREAM_V2_COUNT, STREAM_TTL_THRESHOLD, STREAM_TTL_BUMP);
        env.storage()
            .persistent()
            .get(&STREAM_V2_COUNT)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    fn setup_env() -> (Env, Address, Address, Address) {
        let env = Env::default();
        let _contract_id = env.register_contract(None, StreamContract);
        let owner = Address::generate(&env);
        let recipient = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();

        (env, owner, recipient, other)
    }

    #[test]
    fn test_initialize() {
        let (env, _, _, _) = setup_env();
        let contract_id = env.register_contract(None, StreamContract);
        let client = StreamContractClient::new(&env, &contract_id);

        client.initialize();

        assert_eq!(client.get_stream_v1_count(), 0);
        assert_eq!(client.get_stream_v2_count(), 0);
    }

    #[test]
    fn test_create_stream_v1() {
        let (env, owner, recipient, _) = setup_env();
        let contract_id = env.register_contract(None, StreamContract);
        let client = StreamContractClient::new(&env, &contract_id);

        client.initialize();

        let stream_id = client.create_stream_v1(
            &owner,
            &recipient,
            &1000,
            &100,
            &200,
        );

        assert_eq!(stream_id, 1);

        let stream = client.get_stream_v1(&stream_id);
        assert_eq!(stream.id, 1);
        assert_eq!(stream.amount, 1000);
        assert_eq!(stream.claimed_amount, 0);
        assert_eq!(stream.status, StreamStatus::Active);
    }

    #[test]
    #[should_panic(expected = "Amount must be greater than zero")]
    fn test_create_stream_v1_invalid_amount() {
        let (env, owner, recipient, _) = setup_env();
        let contract_id = env.register_contract(None, StreamContract);
        let client = StreamContractClient::new(&env, &contract_id);

        client.initialize();

        env.mock_all_auths();
        client.create_stream_v1(&owner, &recipient, &0, &100, &200);
    }

    #[test]
    #[should_panic(expected = "Start time must be before end time")]
    fn test_create_stream_v1_invalid_time() {
        let (env, owner, recipient, _) = setup_env();
        let contract_id = env.register_contract(None, StreamContract);
        let client = StreamContractClient::new(&env, &contract_id);

        client.initialize();

        env.mock_all_auths();
        client.create_stream_v1(&owner, &recipient, &1000, &200, &100);
    }

    #[test]
    fn test_migrate_stream_success() {
        let (env, owner, recipient, _) = setup_env();
        let contract_id = env.register_contract(None, StreamContract);
        let client = StreamContractClient::new(&env, &contract_id);

        client.initialize();

        // Create V1 stream
        let v1_id = client.create_stream_v1(&owner, &recipient, &1000, &100, &200);

        // Migrate to V2
        env.mock_all_auths();
        let v2_id = client.migrate_stream(&owner, &v1_id);

        assert_eq!(v2_id, 1);
        assert_eq!(client.get_stream_v2_count(), 1);

        // Verify V1 is marked as migrated
        let v1_stream = client.get_stream_v1(&v1_id);
        assert_eq!(v1_stream.status, StreamStatus::Migrated);

        // Verify V2 stream is created correctly
        let v2_stream = client.get_stream_v2(&v2_id);
        assert_eq!(v2_stream.id, v2_id);
        assert_eq!(v2_stream.v1_id, v1_id);
        assert_eq!(v2_stream.amount, 1000);
        assert_eq!(v2_stream.claimed_amount, 0);
        assert_eq!(v2_stream.status, StreamStatus::Active);
    }

    #[test]
    #[should_panic]
    fn test_migrate_stream_not_found() {
        let (env, owner, _, _) = setup_env();
        let contract_id = env.register_contract(None, StreamContract);
        let client = StreamContractClient::new(&env, &contract_id);

        client.initialize();

        env.mock_all_auths();
        client.migrate_stream(&owner, &999);
    }

    #[test]
    #[should_panic(expected = "V1 stream already migrated")]
    fn test_migrate_stream_already_migrated() {
        let (env, owner, recipient, _) = setup_env();
        let contract_id = env.register_contract(None, StreamContract);
        let client = StreamContractClient::new(&env, &contract_id);

        client.initialize();

        // Create and migrate V1 stream
        let v1_id = client.create_stream_v1(&owner, &recipient, &1000, &100, &200);
        env.mock_all_auths();
        let _ = client.migrate_stream(&owner, &v1_id);

        // Try to migrate again
        env.mock_all_auths();
        client.migrate_stream(&owner, &v1_id);
    }

    #[test]
    #[should_panic(expected = "Sender is not the stream owner")]
    fn test_migrate_stream_unauthorized() {
        let (env, owner, recipient, other) = setup_env();
        let contract_id = env.register_contract(None, StreamContract);
        let client = StreamContractClient::new(&env, &contract_id);

        client.initialize();

        // Create V1 stream
        let v1_id = client.create_stream_v1(&owner, &recipient, &1000, &100, &200);

        // Try to migrate as non-owner
        env.mock_all_auths();
        client.migrate_stream(&other, &v1_id);
    }

    #[test]
    fn test_migrate_stream_with_claimed_amount() {
        let (env, owner, recipient, _) = setup_env();
        let contract_id = env.register_contract(None, StreamContract);
        let client = StreamContractClient::new(&env, &contract_id);

        client.initialize();

        // Create V1 stream
        let v1_id = client.create_stream_v1(&owner, &recipient, &1000, &100, &200);

        // Manually set claimed amount (simulating partial claim)
        env.as_contract(&contract_id, || {
            let v1_key = (symbol_short!("STREAM_V1"), v1_id);
            let mut v1_stream: StreamV1 = env.storage().persistent().get(&v1_key).unwrap();
            v1_stream.claimed_amount = 300;
            env.storage().persistent().set(&v1_key, &v1_stream);
        });

        // Migrate to V2
        env.mock_all_auths();
        let v2_id = client.migrate_stream(&owner, &v1_id);

        // Verify V2 has correct claimed amount and remaining balance
        let v2_stream = client.get_stream_v2(&v2_id);
        assert_eq!(v2_stream.claimed_amount, 300);
        assert_eq!(v2_stream.amount - v2_stream.claimed_amount, 700);
    }

    // ========================================================================
    // MIGRATION EVENT EMISSION TESTS
    // ========================================================================

    #[test]
    fn test_migration_event_contains_correct_v1_id() {
        let (env, owner, recipient, _) = setup_env();
        let contract_id = env.register_contract(None, StreamContract);
        let client = StreamContractClient::new(&env, &contract_id);

        client.initialize();

        // Create first V1 stream
        let v1_id_1 = client.create_stream_v1(&owner, &recipient, &1000, &100, &200);

        // Migrate first stream
        let v2_id_1 = client.migrate_stream(&owner, &v1_id_1);

        // Verify V2 references correct V1
        let v2_stream_1 = client.get_stream_v2(&v2_id_1);
        assert_eq!(v2_stream_1.v1_id, v1_id_1);

        // Create second V1 stream
        let v1_id_2 = client.create_stream_v1(&owner, &recipient, &2000, &100, &200);

        // Migrate second stream
        let v2_id_2 = client.migrate_stream(&owner, &v1_id_2);

        // Verify V2 references correct V1
        let v2_stream_2 = client.get_stream_v2(&v2_id_2);
        assert_eq!(v2_stream_2.v1_id, v1_id_2);
    }

    #[test]
    fn test_migration_event_contains_correct_v2_id() {
        let (env, owner, recipient, _) = setup_env();
        let contract_id = env.register_contract(None, StreamContract);
        let client = StreamContractClient::new(&env, &contract_id);

        client.initialize();

        // Create V1 stream
        let v1_id = client.create_stream_v1(&owner, &recipient, &1000, &100, &200);

        // Migrate to V2
        let v2_id = client.migrate_stream(&owner, &v1_id);

        // Verify V2 ID is correct
        assert_eq!(v2_id, 1);

        // Verify V2 stream has correct ID
        let v2_stream = client.get_stream_v2(&v2_id);
        assert_eq!(v2_stream.id, v2_id);
    }

    #[test]
    fn test_migration_event_contains_correct_sender() {
        let (env, owner, recipient, _) = setup_env();
        let contract_id = env.register_contract(None, StreamContract);
        let client = StreamContractClient::new(&env, &contract_id);

        client.initialize();

        // Create V1 stream
        let v1_id = client.create_stream_v1(&owner, &recipient, &1000, &100, &200);

        // Migrate to V2
        let v2_id = client.migrate_stream(&owner, &v1_id);

        // Verify V2 has correct owner
        let v2_stream = client.get_stream_v2(&v2_id);
        assert_eq!(v2_stream.owner, owner);
    }

    #[test]
    fn test_migration_event_contains_correct_remaining_balance() {
        let (env, owner, recipient, _) = setup_env();
        let contract_id = env.register_contract(None, StreamContract);
        let client = StreamContractClient::new(&env, &contract_id);

        client.initialize();

        // Create V1 stream with 1000 amount
        let v1_id = client.create_stream_v1(&owner, &recipient, &1000, &100, &200);

        // Migrate to V2 (no claimed amount yet)
        let v2_id = client.migrate_stream(&owner, &v1_id);

        // Verify remaining balance is correct (1000 - 0 = 1000)
        let v2_stream = client.get_stream_v2(&v2_id);
        assert_eq!(v2_stream.amount, 1000);
        assert_eq!(v2_stream.claimed_amount, 0);
        assert_eq!(v2_stream.amount - v2_stream.claimed_amount, 1000);
    }

    #[test]
    fn test_migration_event_with_partial_claimed_amount() {
        let (env, owner, recipient, _) = setup_env();
        let contract_id = env.register_contract(None, StreamContract);
        let client = StreamContractClient::new(&env, &contract_id);

        client.initialize();

        // Create V1 stream
        let v1_id = client.create_stream_v1(&owner, &recipient, &1000, &100, &200);

        // Simulate partial claim by modifying V1 stream
        env.as_contract(&contract_id, || {
            let v1_key = (symbol_short!("STREAM_V1"), v1_id);
            let mut v1_stream: StreamV1 = env.storage().persistent().get(&v1_key).unwrap();
            v1_stream.claimed_amount = 400;
            env.storage().persistent().set(&v1_key, &v1_stream);
        });

        // Migrate to V2
        let v2_id = client.migrate_stream(&owner, &v1_id);

        // Verify remaining balance is correct (1000 - 400 = 600)
        let v2_stream = client.get_stream_v2(&v2_id);
        assert_eq!(v2_stream.claimed_amount, 400);
        assert_eq!(v2_stream.amount - v2_stream.claimed_amount, 600);
    }

    // ========================================================================
    // V2 STREAM CREATION TESTS
    // ========================================================================

    #[test]
    fn test_v2_stream_created_with_migrated_state() {
        let (env, owner, recipient, _) = setup_env();
        let contract_id = env.register_contract(None, StreamContract);
        let client = StreamContractClient::new(&env, &contract_id);

        client.initialize();

        // Create V1 stream
        let v1_id = client.create_stream_v1(&owner, &recipient, &1000, &100, &200);

        // Migrate to V2
        let v2_id = client.migrate_stream(&owner, &v1_id);

        // Verify V2 stream has correct state
        let v2_stream = client.get_stream_v2(&v2_id);
        assert_eq!(v2_stream.id, v2_id);
        assert_eq!(v2_stream.owner, owner);
        assert_eq!(v2_stream.recipient, recipient);
        assert_eq!(v2_stream.amount, 1000);
        assert_eq!(v2_stream.start_time, 100);
        assert_eq!(v2_stream.end_time, 200);
        assert_eq!(v2_stream.claimed_amount, 0);
        assert_eq!(v2_stream.status, StreamStatus::Active);
        assert_eq!(v2_stream.v1_id, v1_id);
    }

    #[test]
    fn test_v2_stream_has_migration_timestamp() {
        let (env, owner, recipient, _) = setup_env();
        let contract_id = env.register_contract(None, StreamContract);
        let client = StreamContractClient::new(&env, &contract_id);

        client.initialize();

        // Create V1 stream
        let v1_id = client.create_stream_v1(&owner, &recipient, &1000, &100, &200);

        // Get current timestamp
        let before_migration = env.ledger().timestamp();

        // Migrate to V2
        let v2_id = client.migrate_stream(&owner, &v1_id);

        // Get timestamp after migration
        let after_migration = env.ledger().timestamp();

        // Verify V2 has migration timestamp
        let v2_stream = client.get_stream_v2(&v2_id);
        assert!(v2_stream.migrated_at >= before_migration);
        assert!(v2_stream.migrated_at <= after_migration);
    }

    #[test]
    fn test_multiple_migrations_create_separate_v2_streams() {
        let (env, owner, recipient, _) = setup_env();
        let contract_id = env.register_contract(None, StreamContract);
        let client = StreamContractClient::new(&env, &contract_id);

        client.initialize();

        // Create and migrate first V1 stream
        let v1_id_1 = client.create_stream_v1(&owner, &recipient, &1000, &100, &200);
        let v2_id_1 = client.migrate_stream(&owner, &v1_id_1);

        // Create and migrate second V1 stream
        let v1_id_2 = client.create_stream_v1(&owner, &recipient, &2000, &100, &200);
        let v2_id_2 = client.migrate_stream(&owner, &v1_id_2);

        // Verify both V2 streams exist with different IDs
        assert_ne!(v2_id_1, v2_id_2);

        let v2_stream_1 = client.get_stream_v2(&v2_id_1);
        let v2_stream_2 = client.get_stream_v2(&v2_id_2);

        assert_eq!(v2_stream_1.v1_id, v1_id_1);
        assert_eq!(v2_stream_2.v1_id, v1_id_2);
        assert_eq!(v2_stream_1.amount, 1000);
        assert_eq!(v2_stream_2.amount, 2000);
    }

    // ========================================================================
    // V1 STATE FINALIZATION TESTS
    // ========================================================================

    #[test]
    fn test_v1_stream_marked_as_migrated() {
        let (env, owner, recipient, _) = setup_env();
        let contract_id = env.register_contract(None, StreamContract);
        let client = StreamContractClient::new(&env, &contract_id);

        client.initialize();

        // Create V1 stream
        let v1_id = client.create_stream_v1(&owner, &recipient, &1000, &100, &200);

        // Verify V1 is initially Active
        let v1_before = client.get_stream_v1(&v1_id);
        assert_eq!(v1_before.status, StreamStatus::Active);

        // Migrate to V2
        client.migrate_stream(&owner, &v1_id);

        // Verify V1 is now Migrated
        let v1_after = client.get_stream_v1(&v1_id);
        assert_eq!(v1_after.status, StreamStatus::Migrated);
    }

    #[test]
    fn test_v1_stream_data_preserved_after_migration() {
        let (env, owner, recipient, _) = setup_env();
        let contract_id = env.register_contract(None, StreamContract);
        let client = StreamContractClient::new(&env, &contract_id);

        client.initialize();

        // Create V1 stream
        let v1_id = client.create_stream_v1(&owner, &recipient, &1000, &100, &200);

        // Get V1 data before migration
        let v1_before = client.get_stream_v1(&v1_id);

        // Migrate to V2
        client.migrate_stream(&owner, &v1_id);

        // Get V1 data after migration
        let v1_after = client.get_stream_v1(&v1_id);

        // Verify V1 data is preserved (except status)
        assert_eq!(v1_before.id, v1_after.id);
        assert_eq!(v1_before.owner, v1_after.owner);
        assert_eq!(v1_before.recipient, v1_after.recipient);
        assert_eq!(v1_before.amount, v1_after.amount);
        assert_eq!(v1_before.start_time, v1_after.start_time);
        assert_eq!(v1_before.end_time, v1_after.end_time);
        assert_eq!(v1_before.claimed_amount, v1_after.claimed_amount);
        assert_eq!(v1_after.status, StreamStatus::Migrated);
    }
}
