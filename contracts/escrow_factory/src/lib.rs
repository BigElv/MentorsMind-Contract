#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, Env, IntoVal, Symbol, Vec,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowInfo {
    pub address: Address,
    pub session_id: Symbol,
    pub mentor: Address,
    pub learner: Address,
    pub created_at: u64,
}

// Storage keys
const ADMIN: Symbol = symbol_short!("ADMIN");
const IMPLEMENTATION: Symbol = symbol_short!("IMPL");
const PAUSE_GUARDIAN: Symbol = symbol_short!("PAUSE_GD");
const ESCROW_MAPPING: Symbol = symbol_short!("ESC_MAP");
const ESCROW_LIST: Symbol = symbol_short!("ESC_LIST");
const ESCROW_COUNT: Symbol = symbol_short!("ESC_CNT");
const FACTORY_TTL_THRESHOLD: u32 = 500_000;
const FACTORY_TTL_BUMP: u32 = 1_000_000;

#[contract]
pub struct EscrowFactory;

#[contractimpl]
impl EscrowFactory {
    /// Initialize the factory with admin, implementation contract, and optional pause guardian.
    pub fn initialize(env: Env, admin: Address, implementation_address: Address) {
        if env.storage().persistent().has(&ADMIN) {
            panic!("Already initialized");
        }

        env.storage().persistent().set(&ADMIN, &admin);
        env.storage()
            .persistent()
            .extend_ttl(&ADMIN, FACTORY_TTL_THRESHOLD, FACTORY_TTL_BUMP);

        env.storage()
            .persistent()
            .set(&IMPLEMENTATION, &implementation_address);
        env.storage().persistent().extend_ttl(
            &IMPLEMENTATION,
            FACTORY_TTL_THRESHOLD,
            FACTORY_TTL_BUMP,
        );

        env.storage().persistent().set(&ESCROW_COUNT, &0u64);
        env.storage().persistent().extend_ttl(
            &ESCROW_COUNT,
            FACTORY_TTL_THRESHOLD,
            FACTORY_TTL_BUMP,
        );
    }

    /// Set the pause guardian contract address. Admin only.
    pub fn set_pause_guardian(env: Env, guardian: Address) {
        let admin = Self::admin(&env);
        admin.require_auth();
        env.storage().persistent().set(&PAUSE_GUARDIAN, &guardian);
        env.storage()
            .persistent()
            .extend_ttl(&PAUSE_GUARDIAN, FACTORY_TTL_THRESHOLD, FACTORY_TTL_BUMP);
    }

    /// Deploy a new escrow contract instance using minimal proxy pattern
    pub fn deploy_escrow(
        env: Env,
        mentor: Address,
        learner: Address,
        amount: i128,
        token: Address,
        session_id: Symbol,
    ) -> Address {
        // Check pause guardian
        if let Some(guardian) = env.storage().persistent().get::<_, Address>(&PAUSE_GUARDIAN) {
            let is_paused: bool = env.invoke_contract(
                &guardian,
                &Symbol::new(&env, "is_paused"),
                soroban_sdk::Vec::new(&env),
            );
            if is_paused {
                panic!("Contract is paused");
            }
        }
        // Check if session ID already exists
        let session_key = (ESCROW_MAPPING, session_id.clone());
        if env.storage().persistent().has(&session_key) {
            panic!("Session ID already exists");
        }

        // Get implementation address
        let implementation: Address = env
            .storage()
            .persistent()
            .get(&IMPLEMENTATION)
            .expect("Implementation not set");

        // Deploy new escrow instance as minimal proxy
        let escrow_address = Self::deploy_minimal_proxy(&env, &implementation);

        // Initialize the new escrow contract
        let initialize_sym = Symbol::new(&env, "initialize");
        env.invoke_contract(
            &escrow_address,
            &initialize_sym,
            (
                env.current_contract_address(), // Set factory as admin
                Address::generate(&env),        // Treasury (placeholder)
                0u32,                           // Fee bps (placeholder)
                Vec::new(&env),                 // Approved tokens (empty for now)
                72 * 60 * 60,                   // Auto release delay (72 hours)
            )
                .into_val(&env),
        );

        // Create escrow in the deployed contract
        let create_escrow_sym = Symbol::new(&env, "create_escrow");
        env.invoke_contract(
            &escrow_address,
            &create_escrow_sym,
            (
                mentor,
                learner,
                amount,
                session_id.clone(),
                token,
                env.ledger().timestamp() + (24 * 60 * 60), // Session end time (24 hours from now)
            )
                .into_val(&env),
        );

        // Store mapping
        env.storage()
            .persistent()
            .set(&session_key, &escrow_address);
        env.storage().persistent().extend_ttl(
            &session_key,
            FACTORY_TTL_THRESHOLD,
            FACTORY_TTL_BUMP,
        );

        // Add to list
        let mut count: u64 = env.storage().persistent().get(&ESCROW_COUNT).unwrap_or(0);
        count += 1;
        env.storage().persistent().set(&ESCROW_COUNT, &count);
        env.storage().persistent().extend_ttl(
            &ESCROW_COUNT,
            FACTORY_TTL_THRESHOLD,
            FACTORY_TTL_BUMP,
        );

        let list_key = (ESCROW_LIST, count);
        let escrow_info = EscrowInfo {
            address: escrow_address.clone(),
            session_id: session_id.clone(),
            mentor,
            learner,
            created_at: env.ledger().timestamp(),
        };
        env.storage().persistent().set(&list_key, &escrow_info);
        env.storage()
            .persistent()
            .extend_ttl(&list_key, FACTORY_TTL_THRESHOLD, FACTORY_TTL_BUMP);

        // Emit event
        env.events().publish(
            (symbol_short!("escrow_deployed"), session_id.clone()),
            (escrow_address.clone(), session_id),
        );

        escrow_address
    }

    /// Get escrow address by session ID
    pub fn get_escrow_address(env: Env, session_id: Symbol) -> Option<Address> {
        let session_key = (ESCROW_MAPPING, session_id);
        env.storage().persistent().get(&session_key)
    }

    /// Get all escrows with pagination
    pub fn get_all_escrows(env: Env, page: u32, page_size: u32) -> Vec<EscrowInfo> {
        if page == 0 || page_size == 0 {
            panic!("Invalid pagination parameters");
        }

        let count: u64 = env.storage().persistent().get(&ESCROW_COUNT).unwrap_or(0);
        let start_idx = ((page - 1) * page_size) as u64 + 1;
        let end_idx = (start_idx + page_size as u64 - 1).min(count);

        let mut result = Vec::new(&env);

        for i in start_idx..=end_idx {
            let list_key = (ESCROW_LIST, i);
            if let Some(escrow_info) = env.storage().persistent().get::<_, EscrowInfo>(&list_key) {
                result.push_back(escrow_info);
            }
            env.storage().persistent().extend_ttl(
                &list_key,
                FACTORY_TTL_THRESHOLD,
                FACTORY_TTL_BUMP,
            );
        }

        result
    }

    /// Update implementation contract for future deployments
    pub fn upgrade_implementation(env: Env, new_implementation: Address) {
        let admin = Self::admin(&env);
        admin.require_auth();

        env.storage()
            .persistent()
            .set(&IMPLEMENTATION, &new_implementation);
        env.storage().persistent().extend_ttl(
            &IMPLEMENTATION,
            FACTORY_TTL_THRESHOLD,
            FACTORY_TTL_BUMP,
        );

        env.events().publish(
            (symbol_short!("implementation_upgraded")),
            (new_implementation, env.ledger().timestamp()),
        );
    }

    /// Get current implementation address
    pub fn get_implementation(env: Env) -> Address {
        env.storage()
            .persistent()
            .get(&IMPLEMENTATION)
            .expect("Implementation not set")
    }

    /// Get admin address
    pub fn get_admin(env: Env) -> Address {
        Self::admin(&env)
    }

    /// Get total escrow count
    pub fn get_escrow_count(env: Env) -> u64 {
        env.storage().persistent().get(&ESCROW_COUNT).unwrap_or(0)
    }

    /// Deploy minimal proxy (clone) of implementation contract
    fn deploy_minimal_proxy(env: &Env, implementation: &Address) -> Address {
        // In Soroban, we deploy a new contract instance that will delegate calls
        // to the implementation. For now, we create a new contract address.
        // In a real implementation, this would create a minimal proxy contract.
        let salt = env.prng().gen::<u64>();
        let deployer = env.deployer();
        let deployed_address = deployer
            .with_current_contract(salt)
            .deploy_address(implementation);
        deployed_address
    }

    /// Get admin address (internal helper)
    fn admin(env: &Env) -> Address {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&ADMIN)
            .expect("Not initialized");
        env.storage()
            .persistent()
            .extend_ttl(&ADMIN, FACTORY_TTL_THRESHOLD, FACTORY_TTL_BUMP);
        admin
    }
}

#[cfg(test)]
mod testutils;
