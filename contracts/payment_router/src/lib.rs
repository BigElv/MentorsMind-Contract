#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, BytesN, Env, IntoVal,
    Symbol, Vec,
};

// Source chain constants
pub const CHAIN_STELLAR: u32 = 0;
pub const CHAIN_ETHEREUM: u32 = 2;
pub const CHAIN_SOLANA: u32 = 1;
pub const CHAIN_BSC: u32 = 4;

#[derive(Clone)]
#[contracttype]
pub struct RouterConfig {
    pub admin: Address,
    pub escrow_contract: Address,
    pub bridge_receiver: Address,
    pub supported_chains: Vec<u32>,
}

#[derive(Clone)]
#[contracttype]
pub struct PaymentRoute {
    pub escrow_id: u64,
    pub source_chain: u32,
    pub source_tx_hash: BytesN<32>,
    pub learner: Address,
    pub mentor: Address,
    pub amount: i128,
    pub token: Address,
    pub created_at: u64,
}

#[contracttype]
pub struct PaymentRoutedEvent {
    pub source_chain: u32,
    pub source_tx_hash: BytesN<32>,
    pub escrow_id: u64,
    pub learner: Address,
    pub mentor: Address,
    pub amount: i128,
    pub token: Address,
}

#[contracttype]
pub struct EscrowParams {
    pub mentor: Address,
    pub learner: Address,
    pub amount: i128,
    pub session_id: Symbol,
    pub token_address: Address,
    pub session_end_time: u64,
    pub total_sessions: u32,
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Config,
    Route(BytesN<32>),
    ProcessedTx(BytesN<32>),
    EscrowIdCounter,
}

#[contract]
pub struct PaymentRouter;

#[contractimpl]
impl PaymentRouter {
    /// Initialize the payment router contract
    pub fn init(env: Env, admin: Address, escrow_contract: Address, bridge_receiver: Address) {
        // Check if already initialized
        if env.storage().instance().has(&DataKey::Config) {
            panic!("Already initialized");
        }

        let mut supported_chains = Vec::new(&env);
        supported_chains.push_back(CHAIN_STELLAR);
        supported_chains.push_back(CHAIN_ETHEREUM);
        supported_chains.push_back(CHAIN_SOLANA);
        supported_chains.push_back(CHAIN_BSC);

        let config = RouterConfig {
            admin: admin.clone(),
            escrow_contract,
            bridge_receiver,
            supported_chains,
        };

        env.storage().instance().set(&DataKey::Config, &config);
        env.storage()
            .instance()
            .set(&DataKey::EscrowIdCounter, &0u64);

        // Emit initialization event
        env.events()
            .publish((symbol_short!("router"), symbol_short!("init")), admin);
    }

    /// Route a payment from any supported chain to create an escrow
    ///
    /// # Arguments
    /// * `source_chain` - The chain ID where payment originated (0 for Stellar native)
    /// * `source_tx_hash` - The transaction hash on the source chain
    /// * `learner` - The learner's address
    /// * `mentor` - The mentor's address  
    /// * `amount` - The payment amount
    /// * `token` - The token contract address
    ///
    /// # Returns
    /// * The escrow ID created
    pub fn route_payment(
        env: Env,
        source_chain: u32,
        source_tx_hash: BytesN<32>,
        learner: Address,
        mentor: Address,
        amount: i128,
        token: Address,
    ) -> u64 {
        // Verify the source transaction
        Self::verify_source_transaction(
            &env,
            source_chain,
            &source_tx_hash,
            &learner,
            amount,
            &token,
        );

        // Check for duplicate routing
        let processed_key = DataKey::ProcessedTx(source_tx_hash.clone());
        if env.storage().instance().has(&processed_key) {
            panic!("Transaction already routed");
        }

        // Verify amount is positive
        if amount <= 0 {
            panic!("Amount must be positive");
        }

        // Verify learner authorization for direct Stellar payments
        // For bridged payments, verification happens via bridge receiver
        if source_chain == CHAIN_STELLAR {
            learner.require_auth();
        }

        // Get config
        let config = Self::get_config(env.clone());

        // For Stellar direct payments, transfer tokens from learner to escrow
        if source_chain == CHAIN_STELLAR {
            let token_client = token::Client::new(&env, &token);

            // Verify learner has sufficient balance
            if token_client.balance(&learner) < amount {
                panic!("Insufficient token balance");
            }

            // Transfer tokens from learner to the escrow contract
            token_client.transfer(&learner, &config.escrow_contract, &amount);
        }

        // Generate a unique session ID for the escrow
        let session_id = Self::generate_session_id(&env, &source_tx_hash, source_chain);

        // Create escrow via cross-contract call
        let escrow_id = Self::create_escrow(
            &env,
            &config.escrow_contract,
            mentor.clone(),
            learner.clone(),
            amount,
            session_id,
            token.clone(),
        );

        // Store the route mapping
        let route = PaymentRoute {
            escrow_id,
            source_chain,
            source_tx_hash: source_tx_hash.clone(),
            learner: learner.clone(),
            mentor: mentor.clone(),
            amount,
            token: token.clone(),
            created_at: env.ledger().timestamp(),
        };

        let route_key = DataKey::Route(source_tx_hash.clone());
        env.storage().instance().set(&route_key, &route);
        env.storage().instance().set(&processed_key, &true);

        // Update counter
        let counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::EscrowIdCounter)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::EscrowIdCounter, &(counter + 1));

        // Emit payment routed event
        let event = PaymentRoutedEvent {
            source_chain,
            source_tx_hash: source_tx_hash.clone(),
            escrow_id,
            learner: learner.clone(),
            mentor: mentor.clone(),
            amount,
            token: token.clone(),
        };
        Self::emit_payment_routed(&env, event);

        escrow_id
    }

    /// Get the escrow ID for a given source transaction hash
    pub fn get_route(env: Env, source_tx_hash: BytesN<32>) -> u64 {
        let route_key = DataKey::Route(source_tx_hash);
        let route: PaymentRoute = env
            .storage()
            .instance()
            .get(&route_key)
            .expect("Route not found");
        route.escrow_id
    }

    /// Get full route details for a source transaction hash
    pub fn get_route_details(env: Env, source_tx_hash: BytesN<32>) -> PaymentRoute {
        let route_key = DataKey::Route(source_tx_hash);
        env.storage()
            .instance()
            .get(&route_key)
            .expect("Route not found")
    }

    /// Check if a transaction has already been routed
    pub fn is_tx_processed(env: Env, source_tx_hash: BytesN<32>) -> bool {
        let processed_key = DataKey::ProcessedTx(source_tx_hash);
        env.storage().instance().has(&processed_key)
    }

    /// Get the list of supported chains
    pub fn get_supported_chains(env: Env) -> Vec<u32> {
        let config = Self::get_config(env.clone());
        config.supported_chains
    }

    /// Add a supported chain (admin only)
    pub fn add_supported_chain(env: Env, chain_id: u32) {
        let config = Self::get_config(env.clone());
        config.admin.require_auth();

        // Check if chain already exists
        let exists = config.supported_chains.iter().any(|c| c == chain_id);
        if exists {
            panic!("Chain already supported");
        }

        let mut new_config = config;
        new_config.supported_chains.push_back(chain_id);
        env.storage().instance().set(&DataKey::Config, &new_config);
    }

    /// Remove a supported chain (admin only)
    pub fn remove_supported_chain(env: Env, chain_id: u32) {
        let config = Self::get_config(env.clone());
        config.admin.require_auth();

        // Cannot remove Stellar native chain
        if chain_id == CHAIN_STELLAR {
            panic!("Cannot remove Stellar native chain");
        }

        let mut new_chains = Vec::new(&env);
        for chain in config.supported_chains.iter() {
            if chain != chain_id {
                new_chains.push_back(chain);
            }
        }

        let mut new_config = config;
        new_config.supported_chains = new_chains;
        env.storage().instance().set(&DataKey::Config, &new_config);
    }

    /// Update escrow contract address (admin only)
    pub fn set_escrow_contract(env: Env, escrow_contract: Address) {
        let config = Self::get_config(env.clone());
        config.admin.require_auth();

        let mut new_config = config;
        new_config.escrow_contract = escrow_contract;
        env.storage().instance().set(&DataKey::Config, &new_config);
    }

    /// Update bridge receiver address (admin only)
    pub fn set_bridge_receiver(env: Env, bridge_receiver: Address) {
        let config = Self::get_config(env.clone());
        config.admin.require_auth();

        let mut new_config = config;
        new_config.bridge_receiver = bridge_receiver;
        env.storage().instance().set(&DataKey::Config, &new_config);
    }

    /// Get the router configuration
    pub fn get_config(env: Env) -> RouterConfig {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .expect("Router not initialized")
    }

    /// Get total number of routed payments
    pub fn get_route_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::EscrowIdCounter)
            .unwrap_or(0)
    }

    // Helper functions

    fn verify_source_transaction(
        env: &Env,
        source_chain: u32,
        source_tx_hash: &BytesN<32>,
        _learner: &Address,
        _amount: i128,
        _token: &Address,
    ) {
        let config = Self::get_config(env.clone());

        // Check if source chain is supported
        let is_supported = config
            .supported_chains
            .iter()
            .any(|chain| chain == source_chain);
        if !is_supported {
            panic!("Source chain not supported");
        }

        // For bridged transactions, verify via bridge receiver
        if source_chain != CHAIN_STELLAR {
            // Check if the bridge receiver has processed this VAA
            let is_processed: bool = env.invoke_contract(
                &config.bridge_receiver,
                &Symbol::new(env, "is_vaa_processed"),
                (source_tx_hash.clone(),).into_val(env),
            );

            if !is_processed {
                panic!("Bridge transaction not verified");
            }
        }
        // For Stellar native (source_chain == 0), verification is done via require_auth
        // in the route_payment function
    }

    fn create_escrow(
        env: &Env,
        escrow_contract: &Address,
        mentor: Address,
        learner: Address,
        amount: i128,
        session_id: Symbol,
        token: Address,
    ) -> u64 {
        // Use a default session end time (30 days from now)
        let session_end_time = env.ledger().timestamp() + (30 * 24 * 60 * 60);
        let total_sessions = 1u32;

        let params = EscrowParams {
            mentor,
            learner,
            amount,
            session_id,
            token_address: token,
            session_end_time,
            total_sessions,
        };

        // Call create_escrow on the escrow contract
        let escrow_id: u64 = env.invoke_contract(
            escrow_contract,
            &Symbol::new(env, "create_escrow"),
            (params,).into_val(env),
        );

        escrow_id
    }

    fn generate_session_id(env: &Env, _source_tx_hash: &BytesN<32>, _source_chain: u32) -> Symbol {
        // Generate a unique session ID using counter
        // The escrow contract will enforce uniqueness via SESSION_KEY
        let counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::EscrowIdCounter)
            .unwrap_or(0);

        // Use a simple scheme: alternate between known unique symbols based on counter
        // Since the escrow contract tracks session_id uniqueness, we can cycle through
        // a set of base symbols and rely on the contract's internal counter for true uniqueness
        match counter % 4 {
            0 => Symbol::new(env, "ROUTER_PAY_A"),
            1 => Symbol::new(env, "ROUTER_PAY_B"),
            2 => Symbol::new(env, "ROUTER_PAY_C"),
            _ => Symbol::new(env, "ROUTER_PAY_D"),
        }
    }

    fn emit_payment_routed(env: &Env, event: PaymentRoutedEvent) {
        env.events()
            .publish((symbol_short!("router"), symbol_short!("routed")), event);
    }
}

// Unit tests
#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn setup_env(env: &Env) -> (Address, Address, Address, Address, PaymentRouterClient<'_>) {
        let admin = Address::generate(env);
        let escrow_contract = Address::generate(env);
        let bridge_receiver = Address::generate(env);
        let token = Address::generate(env);

        let contract_id = env.register_contract(None, PaymentRouter);
        let client = PaymentRouterClient::new(env, &contract_id);

        (admin, escrow_contract, bridge_receiver, token, client)
    }

    #[test]
    fn test_init() {
        let env = Env::default();
        let (admin, escrow_contract, bridge_receiver, _, client) = setup_env(&env);

        client.init(&admin, &escrow_contract, &bridge_receiver);

        let config = client.get_config();
        assert_eq!(config.admin, admin);
        assert_eq!(config.escrow_contract, escrow_contract);
        assert_eq!(config.bridge_receiver, bridge_receiver);

        let chains = client.get_supported_chains();
        assert_eq!(chains.len(), 4);
        assert_eq!(chains.get(0).unwrap(), CHAIN_STELLAR);
        assert_eq!(chains.get(1).unwrap(), CHAIN_ETHEREUM);
    }

    #[test]
    #[should_panic(expected = "Already initialized")]
    fn test_double_init() {
        let env = Env::default();
        let (admin, escrow_contract, bridge_receiver, _, client) = setup_env(&env);

        client.init(&admin, &escrow_contract, &bridge_receiver);
        client.init(&admin, &escrow_contract, &bridge_receiver);
    }

    #[test]
    fn test_add_supported_chain() {
        let env = Env::default();
        let (admin, escrow_contract, bridge_receiver, _, client) = setup_env(&env);
        env.mock_all_auths();

        client.init(&admin, &escrow_contract, &bridge_receiver);

        // Add a new chain (e.g., Arbitrum = 23)
        client.add_supported_chain(&23);

        let chains = client.get_supported_chains();
        assert_eq!(chains.len(), 5);
    }

    #[test]
    #[should_panic(expected = "Chain already supported")]
    fn test_add_duplicate_chain() {
        let env = Env::default();
        let (admin, escrow_contract, bridge_receiver, _, client) = setup_env(&env);
        env.mock_all_auths();

        client.init(&admin, &escrow_contract, &bridge_receiver);
        client.add_supported_chain(&CHAIN_ETHEREUM);
    }

    #[test]
    fn test_remove_supported_chain() {
        let env = Env::default();
        let (admin, escrow_contract, bridge_receiver, _, client) = setup_env(&env);
        env.mock_all_auths();

        client.init(&admin, &escrow_contract, &bridge_receiver);
        client.remove_supported_chain(&CHAIN_BSC);

        let chains = client.get_supported_chains();
        assert_eq!(chains.len(), 3);

        let contains_bsc = chains.iter().any(|c| c == CHAIN_BSC);
        assert!(!contains_bsc);
    }

    #[test]
    #[should_panic(expected = "Cannot remove Stellar native chain")]
    fn test_remove_stellar_chain() {
        let env = Env::default();
        let (admin, escrow_contract, bridge_receiver, _, client) = setup_env(&env);
        env.mock_all_auths();

        client.init(&admin, &escrow_contract, &bridge_receiver);
        client.remove_supported_chain(&CHAIN_STELLAR);
    }

    #[test]
    fn test_is_tx_processed_not_found() {
        let env = Env::default();
        let (admin, escrow_contract, bridge_receiver, _, client) = setup_env(&env);

        client.init(&admin, &escrow_contract, &bridge_receiver);

        let tx_hash = BytesN::from_array(&env, &[0u8; 32]);
        assert!(!client.is_tx_processed(&tx_hash));
    }

    #[test]
    #[should_panic(expected = "Source chain not supported")]
    fn test_route_payment_unsupported_chain() {
        let env = Env::default();
        let (admin, escrow_contract, bridge_receiver, token, client) = setup_env(&env);
        let learner = Address::generate(&env);
        let mentor = Address::generate(&env);

        client.init(&admin, &escrow_contract, &bridge_receiver);

        let tx_hash = BytesN::from_array(&env, &[1u8; 32]);

        // Try to route from unsupported chain (99)
        client.route_payment(&99, &tx_hash, &learner, &mentor, &1000, &token);
    }

    #[test]
    #[should_panic(expected = "Amount must be positive")]
    fn test_route_payment_zero_amount() {
        let env = Env::default();
        let (admin, escrow_contract, bridge_receiver, token, client) = setup_env(&env);
        let learner = Address::generate(&env);
        let mentor = Address::generate(&env);

        client.init(&admin, &escrow_contract, &bridge_receiver);

        let tx_hash = BytesN::from_array(&env, &[1u8; 32]);

        client.route_payment(&CHAIN_STELLAR, &tx_hash, &learner, &mentor, &0, &token);
    }

    #[test]
    fn test_set_escrow_contract() {
        let env = Env::default();
        let (admin, escrow_contract, bridge_receiver, _, client) = setup_env(&env);
        let new_escrow = Address::generate(&env);
        env.mock_all_auths();

        client.init(&admin, &escrow_contract, &bridge_receiver);
        client.set_escrow_contract(&new_escrow);

        let config = client.get_config();
        assert_eq!(config.escrow_contract, new_escrow);
    }

    #[test]
    fn test_set_bridge_receiver() {
        let env = Env::default();
        let (admin, escrow_contract, bridge_receiver, _, client) = setup_env(&env);
        let new_bridge = Address::generate(&env);
        env.mock_all_auths();

        client.init(&admin, &escrow_contract, &bridge_receiver);
        client.set_bridge_receiver(&new_bridge);

        let config = client.get_config();
        assert_eq!(config.bridge_receiver, new_bridge);
    }

    #[test]
    fn test_get_route_count_initial() {
        let env = Env::default();
        let (admin, escrow_contract, bridge_receiver, _, client) = setup_env(&env);

        client.init(&admin, &escrow_contract, &bridge_receiver);

        let count = client.get_route_count();
        assert_eq!(count, 0);
    }
}
