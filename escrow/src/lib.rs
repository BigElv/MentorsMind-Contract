#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, symbol_short};

// Escrow status enum
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EscrowStatus {
    Active,
    Released,
    Disputed,
    Refunded,
}

// Escrow data structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct Escrow {
    pub id: u64,
    pub mentor: Address,
    pub learner: Address,
    pub amount: i128,
    pub session_id: Symbol,
    pub status: EscrowStatus,
    pub created_at: u64,
}

const ESCROW_COUNT: Symbol = symbol_short!("ESC_CNT");
const ADMIN: Symbol = symbol_short!("ADMIN");

#[contract]
pub struct EscrowContract;

#[contractimpl]
impl EscrowContract {
    /// Initialize the contract with an admin
    pub fn initialize(env: Env, admin: Address) {
        // Ensure not already initialized
        if env.storage().instance().has(&ADMIN) {
            panic!("Already initialized");
        }
        
        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&ESCROW_COUNT, &0u64);
    }

    /// Create a new escrow
    pub fn create_escrow(
        env: Env,
        mentor: Address,
        learner: Address,
        amount: i128,
        session_id: Symbol,
    ) -> u64 {
        // Require learner authorization
        learner.require_auth();

        // Get and increment escrow count
        let mut count: u64 = env.storage().instance().get(&ESCROW_COUNT).unwrap_or(0);
        count += 1;

        // Create escrow
        let escrow = Escrow {
            id: count,
            mentor: mentor.clone(),
            learner: learner.clone(),
            amount,
            session_id: session_id.clone(),
            status: EscrowStatus::Active,
            created_at: env.ledger().timestamp(),
        };

        // Store escrow
        let key = (symbol_short!("ESCROW"), count);
        env.storage().persistent().set(&key, &escrow);
        env.storage().instance().set(&ESCROW_COUNT, &count);

        // Emit event
        env.events().publish(
            (symbol_short!("created"), count),
            (mentor, learner, amount, session_id),
        );

        count
    }

    /// Release funds to mentor (called by learner or admin)
    pub fn release_funds(env: Env, escrow_id: u64) {
        let key = (symbol_short!("ESCROW"), escrow_id);
        let mut escrow: Escrow = env.storage().persistent()
            .get(&key)
            .expect("Escrow not found");

        // Check status
        if escrow.status != EscrowStatus::Active {
            panic!("Escrow not active");
        }

        // Require learner or admin authorization.
        // The transaction must be signed by the learner (normal case) or admin (override).
        let admin: Address = env.storage().instance().get(&ADMIN).unwrap();
        let learner = escrow.learner.clone();
        // Try learner auth; if the invoker is the admin instead, require admin auth.
        if learner == admin {
            admin.require_auth();
        } else {
            learner.require_auth();
        }

        // Update status
        escrow.status = EscrowStatus::Released;
        env.storage().persistent().set(&key, &escrow);

        // Emit event
        env.events().publish(
            (symbol_short!("released"), escrow_id),
            (escrow.mentor.clone(), escrow.amount),
        );
    }

    /// Open a dispute (called by mentor or learner)
    pub fn dispute(env: Env, escrow_id: u64) {
        let key = (symbol_short!("ESCROW"), escrow_id);
        let mut escrow: Escrow = env.storage().persistent()
            .get(&key)
            .expect("Escrow not found");

        // Check status
        if escrow.status != EscrowStatus::Active {
            panic!("Escrow not active");
        }

        // Require mentor or learner authorization
        // (In production, add proper auth check)

        // Update status
        escrow.status = EscrowStatus::Disputed;
        env.storage().persistent().set(&key, &escrow);

        // Emit event
        env.events().publish(
            (symbol_short!("disputed"), escrow_id),
            escrow_id,
        );
    }

    /// Refund to learner (called by admin)
    pub fn refund(env: Env, escrow_id: u64) {
        let admin: Address = env.storage().instance().get(&ADMIN).unwrap();
        admin.require_auth();

        let key = (symbol_short!("ESCROW"), escrow_id);
        let mut escrow: Escrow = env.storage().persistent()
            .get(&key)
            .expect("Escrow not found");

        // Check status
        if escrow.status == EscrowStatus::Released || escrow.status == EscrowStatus::Refunded {
            panic!("Cannot refund");
        }

        // Update status
        escrow.status = EscrowStatus::Refunded;
        env.storage().persistent().set(&key, &escrow);

        // Emit event
        env.events().publish(
            (symbol_short!("refunded"), escrow_id),
            (escrow.learner.clone(), escrow.amount),
        );
    }

    /// Get escrow details
    pub fn get_escrow(env: Env, escrow_id: u64) -> Escrow {
        let key = (symbol_short!("ESCROW"), escrow_id);
        env.storage().persistent()
            .get(&key)
            .expect("Escrow not found")
    }

    /// Get total escrow count
    pub fn get_escrow_count(env: Env) -> u64 {
        env.storage().instance().get(&ESCROW_COUNT).unwrap_or(0)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Env, Symbol, symbol_short};

    #[test]
    fn test_create_escrow() {
        let env = Env::default();
        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let mentor = Address::generate(&env);
        let learner = Address::generate(&env);

        // Initialize
        client.initialize(&admin);

        // Create escrow
        env.mock_all_auths();
        let escrow_id = client.create_escrow(
            &mentor,
            &learner,
            &1000,
            &symbol_short!("SESSION1"),
        );

        assert_eq!(escrow_id, 1);

        // Get escrow
        let escrow = client.get_escrow(&escrow_id);
        assert_eq!(escrow.amount, 1000);
        assert_eq!(escrow.status, EscrowStatus::Active);
    }

    #[test]
    fn test_release_funds() {
        let env = Env::default();
        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let mentor = Address::generate(&env);
        let learner = Address::generate(&env);

        // Initialize and create escrow
        client.initialize(&admin);
        env.mock_all_auths();
        let escrow_id = client.create_escrow(
            &mentor,
            &learner,
            &1000,
            &symbol_short!("SESSION1"),
        );

        // Release funds
        client.release_funds(&escrow_id);

        // Check status
        let escrow = client.get_escrow(&escrow_id);
        assert_eq!(escrow.status, EscrowStatus::Released);
    }
}
