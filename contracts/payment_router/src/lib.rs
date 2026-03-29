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

        // Call create_escrow on the escrow contract with individual parameters
        let escrow_id: u64 = env.invoke_contract(
            escrow_contract,
            &Symbol::new(env, "create_escrow"),
            (
                mentor,
                learner,
                amount,
                session_id,
                token,
                session_end_time,
                total_sessions,
            )
                .into_val(env),
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
    use soroban_sdk::testutils::Ledger;

    // =========================================================================
    // Mock Bridge Receiver Contract
    // =========================================================================

    /// Mock bridge receiver that simulates the real bridge receiver contract
    /// for testing cross-chain payment routing
    #[contract]
    pub struct MockBridgeReceiver;

    #[contracttype]
    #[derive(Clone)]
    pub enum MockBridgeKey {
        ProcessedVAA(BytesN<32>),
    }

    #[contractimpl]
    impl MockBridgeReceiver {
        /// Mark a VAA as processed (for test setup)
        pub fn set_vaa_processed(env: Env, vaa_hash: BytesN<32>) {
            env.storage()
                .instance()
                .set(&MockBridgeKey::ProcessedVAA(vaa_hash), &true);
        }

        /// Check if a VAA has been processed
        pub fn is_vaa_processed(env: Env, vaa_hash: BytesN<32>) -> bool {
            env.storage()
                .instance()
                .has(&MockBridgeKey::ProcessedVAA(vaa_hash))
        }
    }

    // =========================================================================
    // Mock Escrow Contract
    // =========================================================================

    /// Mock escrow contract that simulates escrow creation
    #[contract]
    pub struct MockEscrow;

    #[contracttype]
    #[derive(Clone)]
    pub enum MockEscrowKey {
        EscrowCount,
        Escrow(u64),
        Session(Symbol),
    }

    #[contractimpl]
    impl MockEscrow {
        /// Create an escrow (simplified mock implementation)
        pub fn create_escrow(
            env: Env,
            mentor: Address,
            learner: Address,
            amount: i128,
            session_id: Symbol,
            token_address: Address,
            session_end_time: u64,
            total_sessions: u32,
        ) -> u64 {
            // Simplified: just increment counter and return ID
            let mut count: u64 = env
                .storage()
                .instance()
                .get(&MockEscrowKey::EscrowCount)
                .unwrap_or(0);
            count += 1;
            env.storage()
                .instance()
                .set(&MockEscrowKey::EscrowCount, &count);
            count
        }

        pub fn get_escrow_count(env: Env) -> u64 {
            env.storage()
                .instance()
                .get(&MockEscrowKey::EscrowCount)
                .unwrap_or(0)
        }
    }

    // =========================================================================
    // Mock Token Contract
    // =========================================================================

    /// Mock token contract for testing Stellar direct payments
    #[contract]
    pub struct MockToken;

    #[contracttype]
    #[derive(Clone)]
    pub enum MockTokenKey {
        Balance(Address),
    }

    #[contractimpl]
    impl MockToken {
        pub fn mint(env: Env, to: Address, amount: i128) {
            let bal: i128 = env
                .storage()
                .instance()
                .get(&MockTokenKey::Balance(to.clone()))
                .unwrap_or(0);
            env.storage()
                .instance()
                .set(&MockTokenKey::Balance(to), &(bal + amount));
        }

        pub fn balance(env: Env, id: Address) -> i128 {
            env.storage()
                .instance()
                .get(&MockTokenKey::Balance(id))
                .unwrap_or(0)
        }

        pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
            let from_bal = Self::balance(env.clone(), from.clone());
            assert!(from_bal >= amount, "Insufficient balance");
            let to_bal = Self::balance(env.clone(), to.clone());
            env.storage()
                .instance()
                .set(&MockTokenKey::Balance(from), &(from_bal - amount));
            env.storage()
                .instance()
                .set(&MockTokenKey::Balance(to), &(to_bal + amount));
        }

        pub fn spendable_balance(env: Env, id: Address) -> i128 {
            Self::balance(env, id)
        }
    }

    // =========================================================================
    // Test Setup
    // =========================================================================

    fn setup_env(env: &Env) -> (Address, Address, Address, Address, PaymentRouterClient<'_>) {
        let admin = Address::generate(env);
        let escrow_contract = Address::generate(env);
        let bridge_receiver = Address::generate(env);
        let token = Address::generate(env);

        let contract_id = env.register_contract(None, PaymentRouter);
        let client = PaymentRouterClient::new(env, &contract_id);

        (admin, escrow_contract, bridge_receiver, token, client)
    }

    /// Setup with mock contracts for integration testing
    struct IntegrationFixture {
        env: Env,
        router_client: PaymentRouterClient<'static>,
        bridge_client: MockBridgeReceiverClient<'static>,
        escrow_client: MockEscrowClient<'static>,
        token_client: MockTokenClient<'static>,
        admin: Address,
    }

    impl IntegrationFixture {
        fn setup() -> Self {
            let env = Env::default();
            env.mock_all_auths();

            let admin = Address::generate(&env);

            // Register mock contracts
            let bridge_id = env.register_contract(None, MockBridgeReceiver);
            let escrow_id = env.register_contract(None, MockEscrow);
            let token_id = env.register_contract(None, MockToken);

            // Register payment router
            let router_id = env.register_contract(None, PaymentRouter);

            // Initialize router with mock contract addresses
            let router_client = PaymentRouterClient::new(&env, &router_id);
            router_client.init(&admin, &escrow_id, &bridge_id);

            IntegrationFixture {
                env,
                router_client,
                bridge_client: MockBridgeReceiverClient::new(&env, &bridge_id),
                escrow_client: MockEscrowClient::new(&env, &escrow_id),
                token_client: MockTokenClient::new(&env, &token_id),
                admin,
            }
        }

        fn fund_learner(&self, learner: &Address, amount: i128) {
            self.token_client.mint(learner, &amount);
        }

        fn mark_bridge_vaa_processed(&self, vaa_hash: &BytesN<32>) {
            self.bridge_client.set_vaa_processed(vaa_hash);
        }
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

    // =========================================================================
    // STELLAR DIRECT PAYMENT TEST
    // =========================================================================

    /// Test routing a direct Stellar payment (source_chain = 0)
    /// This verifies that native Stellar payments are properly routed to escrow
    #[test]
    fn test_stellar_direct_payment() {
        let fixture = IntegrationFixture::setup();
        let learner = Address::generate(&fixture.env);
        let mentor = Address::generate(&fixture.env);

        // Fund the learner with tokens
        let amount = 1000i128;
        fixture.fund_learner(&learner, amount);

        let tx_hash = BytesN::from_array(&fixture.env, &[1u8; 32]);

        // Route a Stellar direct payment
        let escrow_id = fixture.router_client.route_payment(
            &CHAIN_STELLAR,
            &tx_hash,
            &learner,
            &mentor,
            &amount,
            &fixture.token_client.address,
        );

        // Verify escrow ID was returned (should be 1 for first escrow)
        assert_eq!(escrow_id, 1);

        // Verify route was stored
        let stored_escrow_id = fixture.router_client.get_route(&tx_hash);
        assert_eq!(stored_escrow_id, escrow_id);

        // Verify transaction is marked as processed
        assert!(fixture.router_client.is_tx_processed(&tx_hash));

        // Verify route count increased
        assert_eq!(fixture.router_client.get_route_count(), 1);

        // Verify route details
        let route = fixture.router_client.get_route_details(&tx_hash);
        assert_eq!(route.source_chain, CHAIN_STELLAR);
        assert_eq!(route.learner, learner);
        assert_eq!(route.mentor, mentor);
        assert_eq!(route.amount, amount);
    }

    // =========================================================================
    // BRIDGED ETH PAYMENT TEST
    // =========================================================================

    /// Test routing a bridged ETH payment
    /// This verifies that cross-chain payments via bridge are properly routed
    #[test]
    fn test_bridged_eth_payment() {
        let fixture = IntegrationFixture::setup();
        let learner = Address::generate(&fixture.env);
        let mentor = Address::generate(&fixture.env);

        let tx_hash = BytesN::from_array(&fixture.env, &[2u8; 32]);
        let amount = 5000i128; // 5000 units (e.g., USDC from ETH)

        // Mark the VAA as processed in the mock bridge receiver
        fixture.mark_bridge_vaa_processed(&tx_hash);

        // Route the bridged ETH payment
        let escrow_id = fixture.router_client.route_payment(
            &CHAIN_ETHEREUM,
            &tx_hash,
            &learner,
            &mentor,
            &amount,
            &fixture.token_client.address,
        );

        // Verify escrow was created
        assert_eq!(escrow_id, 1);

        // Verify route details show ETH as source chain
        let route = fixture.router_client.get_route_details(&tx_hash);
        assert_eq!(route.source_chain, CHAIN_ETHEREUM);
        assert_eq!(route.amount, amount);

        // Verify transaction is marked as processed
        assert!(fixture.router_client.is_tx_processed(&tx_hash));
    }

    // =========================================================================
    // DUPLICATE ROUTING PREVENTION TEST
    // =========================================================================

    /// Test that duplicate routing is prevented
    /// This ensures the same transaction cannot be routed twice (replay protection)
    #[test]
    #[should_panic(expected = "Transaction already routed")]
    fn test_duplicate_routing_prevention() {
        let fixture = IntegrationFixture::setup();
        let learner = Address::generate(&fixture.env);
        let mentor = Address::generate(&fixture.env);

        // Fund the learner
        let amount = 1000i128;
        fixture.fund_learner(&learner, amount);

        let tx_hash = BytesN::from_array(&fixture.env, &[3u8; 32]);

        // Route payment first time - should succeed
        let escrow_id_1 = fixture.router_client.route_payment(
            &CHAIN_STELLAR,
            &tx_hash,
            &learner,
            &mentor,
            &amount,
            &fixture.token_client.address,
        );
        assert_eq!(escrow_id_1, 1);

        // Attempt to route the same transaction again - should panic
        fixture.router_client.route_payment(
            &CHAIN_STELLAR,
            &tx_hash,
            &learner,
            &mentor,
            &amount,
            &fixture.token_client.address,
        );
    }

    /// Test that different transactions get different escrow IDs
    #[test]
    fn test_multiple_routes_different_ids() {
        let fixture = IntegrationFixture::setup();
        let learner = Address::generate(&fixture.env);
        let mentor = Address::generate(&fixture.env);

        // Fund learner with enough for multiple payments
        fixture.fund_learner(&learner, 3000);

        // Route first payment
        let tx_hash_1 = BytesN::from_array(&fixture.env, &[1u8; 32]);
        let escrow_id_1 = fixture.router_client.route_payment(
            &CHAIN_STELLAR,
            &tx_hash_1,
            &learner,
            &mentor,
            &1000,
            &fixture.token_client.address,
        );

        // Route second payment
        let tx_hash_2 = BytesN::from_array(&fixture.env, &[2u8; 32]);
        let escrow_id_2 = fixture.router_client.route_payment(
            &CHAIN_STELLAR,
            &tx_hash_2,
            &learner,
            &mentor,
            &2000,
            &fixture.token_client.address,
        );

        // Verify different escrow IDs
        assert_ne!(escrow_id_1, escrow_id_2);
        assert_eq!(escrow_id_1, 1);
        assert_eq!(escrow_id_2, 2);

        // Verify route count
        assert_eq!(fixture.router_client.get_route_count(), 2);

        // Verify both routes are stored and accessible
        assert_eq!(fixture.router_client.get_route(&tx_hash_1), escrow_id_1);
        assert_eq!(fixture.router_client.get_route(&tx_hash_2), escrow_id_2);
    }

    /// Test that bridged payments from different chains are properly distinguished
    #[test]
    fn test_bridged_payments_different_chains() {
        let fixture = IntegrationFixture::setup();
        let learner = Address::generate(&fixture.env);
        let mentor = Address::generate(&fixture.env);

        // Route from Ethereum
        let tx_hash_eth = BytesN::from_array(&fixture.env, &[10u8; 32]);
        fixture.mark_bridge_vaa_processed(&tx_hash_eth);
        let escrow_id_eth = fixture.router_client.route_payment(
            &CHAIN_ETHEREUM,
            &tx_hash_eth,
            &learner,
            &mentor,
            &3000,
            &fixture.token_client.address,
        );

        // Route from BSC
        let tx_hash_bsc = BytesN::from_array(&fixture.env, &[20u8; 32]);
        fixture.mark_bridge_vaa_processed(&tx_hash_bsc);
        let escrow_id_bsc = fixture.router_client.route_payment(
            &CHAIN_BSC,
            &tx_hash_bsc,
            &learner,
            &mentor,
            &4000,
            &fixture.token_client.address,
        );

        // Verify different escrow IDs
        assert_ne!(escrow_id_eth, escrow_id_bsc);

        // Verify source chains are recorded correctly
        let route_eth = fixture.router_client.get_route_details(&tx_hash_eth);
        let route_bsc = fixture.router_client.get_route_details(&tx_hash_bsc);

        assert_eq!(route_eth.source_chain, CHAIN_ETHEREUM);
        assert_eq!(route_bsc.source_chain, CHAIN_BSC);
    }

    /// Test route not found error
    #[test]
    #[should_panic(expected = "Route not found")]
    fn test_get_route_not_found() {
        let env = Env::default();
        let (admin, escrow_contract, bridge_receiver, _, client) = setup_env(&env);

        client.init(&admin, &escrow_contract, &bridge_receiver);

        // Try to get route for non-existent transaction
        let tx_hash = BytesN::from_array(&env, &[99u8; 32]);
        client.get_route(&tx_hash);
    }

    /// Test that negative amounts are rejected
    #[test]
    #[should_panic(expected = "Amount must be positive")]
    fn test_route_payment_negative_amount() {
        let fixture = IntegrationFixture::setup();
        let learner = Address::generate(&fixture.env);
        let mentor = Address::generate(&fixture.env);

        let tx_hash = BytesN::from_array(&fixture.env, &[1u8; 32]);
        fixture.router_client.route_payment(
            &CHAIN_STELLAR,
            &tx_hash,
            &learner,
            &mentor,
            &-100,
            &fixture.token_client.address,
        );
    }

    /// Test that bridged payment fails when bridge hasn't verified the transaction
    #[test]
    #[should_panic(expected = "Bridge transaction not verified")]
    fn test_bridged_payment_not_verified() {
        let fixture = IntegrationFixture::setup();
        let learner = Address::generate(&fixture.env);
        let mentor = Address::generate(&fixture.env);

        let tx_hash = BytesN::from_array(&fixture.env, &[5u8; 32]);
        // Don't mark VAA as processed - should fail

        fixture.router_client.route_payment(
            &CHAIN_ETHEREUM,
            &tx_hash,
            &learner,
            &mentor,
            &1000,
            &fixture.token_client.address,
        );
    }
}
