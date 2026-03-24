use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, Env, Symbol, Vec,
    testutils::{Address as _, Ledger},
    token::{Client as TokenClient, StellarAssetClient},
};
use mentorminds_escrow::{
    Escrow as EscrowV2, EscrowContract as EscrowContractV2, EscrowContractClient as EscrowV2Client,
    EscrowStatus as EscrowStatusV2,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
enum EscrowStatusV1 {
    Active,
    Released,
    Disputed,
    Refunded,
    Resolved,
}

#[contracttype]
#[derive(Clone, Debug)]
struct EscrowV1 {
    id: u64,
    mentor: Address,
    learner: Address,
    amount: i128,
    session_id: Symbol,
    status: EscrowStatusV1,
    created_at: u64,
    token_address: Address,
    platform_fee: i128,
    net_amount: i128,
    session_end_time: u64,
    auto_release_delay: u64,
    dispute_reason: Symbol,
    resolved_at: u64,
}

const ESCROW_COUNT: Symbol = symbol_short!("ESC_CNT");
const ADMIN: Symbol = symbol_short!("ADMIN");
const TREASURY: Symbol = symbol_short!("TREASURY");
const FEE_BPS: Symbol = symbol_short!("FEE_BPS");
const AUTO_REL_DLY: Symbol = symbol_short!("AR_DELAY");
const APPROVED_TOKEN_KEY: Symbol = symbol_short!("APRV_TOK");
const MAX_FEE_BPS: u32 = 1_000;
const DEFAULT_AUTO_RELEASE_DELAY: u64 = 72 * 60 * 60;
const ESCROW_TTL_THRESHOLD: u32 = 500_000;
const ESCROW_TTL_BUMP: u32 = 1_000_000;

#[contract]
struct EscrowContractV1;

#[contractimpl]
impl EscrowContractV1 {
    pub fn initialize(
        env: Env,
        admin: Address,
        treasury: Address,
        fee_bps: u32,
        approved_tokens: Vec<Address>,
        auto_release_delay_secs: u64,
    ) {
        if env.storage().persistent().has(&ADMIN) {
            panic!("Already initialized");
        }
        if fee_bps > MAX_FEE_BPS {
            panic!("Fee exceeds maximum (1000 bps)");
        }
        env.storage().persistent().set(&ADMIN, &admin);
        env.storage().persistent().extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        env.storage().persistent().set(&TREASURY, &treasury);
        env.storage().persistent().extend_ttl(&TREASURY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        env.storage().persistent().set(&FEE_BPS, &fee_bps);
        env.storage().persistent().extend_ttl(&FEE_BPS, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        env.storage().persistent().set(&ESCROW_COUNT, &0u64);
        env.storage().persistent().extend_ttl(&ESCROW_COUNT, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        let delay = if auto_release_delay_secs == 0 {
            DEFAULT_AUTO_RELEASE_DELAY
        } else {
            auto_release_delay_secs
        };
        env.storage().persistent().set(&AUTO_REL_DLY, &delay);
        env.storage().persistent().extend_ttl(&AUTO_REL_DLY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        for token_addr in approved_tokens.iter() {
            let key = (APPROVED_TOKEN_KEY, token_addr.clone());
            env.storage().persistent().set(&key, &true);
            env.storage().persistent().extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        }
    }

    pub fn create_escrow(
        env: Env,
        mentor: Address,
        learner: Address,
        amount: i128,
        session_id: Symbol,
        token_address: Address,
        session_end_time: u64,
    ) -> u64 {
        if amount <= 0 {
            panic!("Amount must be greater than zero");
        }
        let key_appr = (APPROVED_TOKEN_KEY, token_address.clone());
        let approved = env.storage().persistent().get::<_, bool>(&key_appr).unwrap_or(false);
        if !approved {
            panic!("Token not approved");
        }
        learner.require_auth();
        let token_client = token::Client::new(&env, &token_address);
        let learner_balance = token_client.balance(&learner);
        if learner_balance < amount {
            panic!("Insufficient token balance");
        }
        let auto_release_delay: u64 = env
            .storage()
            .persistent()
            .get(&AUTO_REL_DLY)
            .unwrap_or(DEFAULT_AUTO_RELEASE_DELAY);
        env.storage().persistent().extend_ttl(&AUTO_REL_DLY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        let mut count: u64 = env.storage().persistent().get(&ESCROW_COUNT).unwrap_or(0);
        count += 1;
        env.storage().persistent().set(&ESCROW_COUNT, &count);
        env.storage().persistent().extend_ttl(&ESCROW_COUNT, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        token_client.transfer(&learner, &env.current_contract_address(), &amount);
        let escrow = EscrowV1 {
            id: count,
            mentor: mentor.clone(),
            learner: learner.clone(),
            amount,
            session_id: session_id.clone(),
            status: EscrowStatusV1::Active,
            created_at: env.ledger().timestamp(),
            token_address: token_address.clone(),
            platform_fee: 0,
            net_amount: 0,
            session_end_time,
            auto_release_delay,
            dispute_reason: symbol_short!(""),
            resolved_at: 0,
        };
        let key = (symbol_short!("ESCROW"), count);
        env.storage().persistent().set(&key, &escrow);
        env.storage().persistent().extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        count
    }

    pub fn release_funds(env: Env, caller: Address, escrow_id: u64) {
        let key = (symbol_short!("ESCROW"), escrow_id);
        env.storage().persistent().extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        let mut escrow: EscrowV1 = env.storage().persistent().get(&key).expect("Escrow not found");
        if escrow.status != EscrowStatusV1::Active {
            panic!("Escrow not active");
        }
        let admin: Address = env.storage().persistent().get(&ADMIN).expect("Admin not found");
        env.storage().persistent().extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        caller.require_auth();
        if caller != escrow.learner && caller != admin {
            panic!("Caller not authorized");
        }
        let fee_bps: u32 = env.storage().persistent().get(&FEE_BPS).unwrap_or(0u32);
        env.storage().persistent().extend_ttl(&FEE_BPS, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        let platform_fee: i128 = escrow.amount * (fee_bps as i128) / 10_000;
        let net_amount: i128 = escrow.amount - platform_fee;
        let treasury: Address = env.storage().persistent().get(&TREASURY).expect("Treasury not found");
        env.storage().persistent().extend_ttl(&TREASURY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        let token_client = token::Client::new(&env, &escrow.token_address);
        if platform_fee > 0 {
            token_client.transfer(&env.current_contract_address(), &treasury, &platform_fee);
        }
        token_client.transfer(&env.current_contract_address(), &escrow.mentor, &net_amount);
        escrow.status = EscrowStatusV1::Released;
        escrow.platform_fee = platform_fee;
        escrow.net_amount = net_amount;
        env.storage().persistent().set(&key, &escrow);
    }

    pub fn refund(env: Env, escrow_id: u64) {
        let admin: Address = env.storage().persistent().get(&ADMIN).expect("Admin not found");
        env.storage().persistent().extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        admin.require_auth();
        let key = (symbol_short!("ESCROW"), escrow_id);
        env.storage().persistent().extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        let mut escrow: EscrowV1 = env.storage().persistent().get(&key).expect("Escrow not found");
        if matches!(escrow.status, EscrowStatusV1::Released | EscrowStatusV1::Refunded | EscrowStatusV1::Resolved) {
            panic!("Cannot refund");
        }
        let token_client = token::Client::new(&env, &escrow.token_address);
        token_client.transfer(&env.current_contract_address(), &escrow.learner, &escrow.amount);
        escrow.status = EscrowStatusV1::Refunded;
        env.storage().persistent().set(&key, &escrow);
    }

    pub fn get_escrow(env: Env, escrow_id: u64) -> EscrowV1 {
        let key = (symbol_short!("ESCROW"), escrow_id);
        env.storage().persistent().extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        env.storage().persistent().get(&key).expect("Escrow not found")
    }

    pub fn get_auto_release_delay(env: Env) -> u64 {
        env.storage().persistent().extend_ttl(&AUTO_REL_DLY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        env.storage().persistent().get(&AUTO_REL_DLY).unwrap_or(DEFAULT_AUTO_RELEASE_DELAY)
    }
}

fn create_token(env: &Env, admin: &Address) -> (Address, StellarAssetClient) {
    let token_address = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let sac = StellarAssetClient::new(env, &token_address);
    (token_address, sac)
}

fn advance_time(env: &Env, secs: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp += secs;
    });
}

#[test]
fn test_upgrade_path_preserves_storage_and_enables_new_features() {
    let env = Env::default();
    env.mock_all_auths();
    let fixed_id = Address::generate(&env);
    env.register_contract(Some(fixed_id.clone()), EscrowContractV1);
    let v1 = EscrowContractV1Client::new(&env, &fixed_id);
    let admin = Address::generate(&env);
    let mentor = Address::generate(&env);
    let learner = Address::generate(&env);
    let treasury = Address::generate(&env);
    let (token_address, sac) = create_token(&env, &admin);
    sac.mint(&learner, &10_000);
    let mut approved = Vec::new(&env);
    approved.push_back(token_address.clone());
    v1.initialize(&admin, &treasury, &500u32, &approved, &0u64);
    let now = env.ledger().timestamp();
    let id1 = v1.create_escrow(&mentor, &learner, &1_000, &symbol_short!("S1"), &token_address, &(now + 200));
    let id2 = v1.create_escrow(&mentor, &learner, &1_000, &symbol_short!("S2"), &token_address, &now);
    let e1_v1 = v1.get_escrow(&id1);
    assert_eq!(e1_v1.status, EscrowStatusV1::Active);
    env.register_contract(Some(fixed_id.clone()), EscrowContractV2);
    let v2 = EscrowV2Client::new(&env, &fixed_id);
    let e1_v2: EscrowV2 = v2.get_escrow(&id1);
    assert_eq!(e1_v2.id, id1);
    assert_eq!(e1_v2.mentor, mentor);
    assert_eq!(e1_v2.learner, learner);
    assert_eq!(e1_v2.amount, 1_000);
    assert_eq!(e1_v2.status, EscrowStatusV2::Active);
    assert_eq!(e1_v2.dispute_reason, symbol_short!(""));
    assert_eq!(e1_v2.resolved_at, 0);
    assert_eq!(v2.get_auto_release_delay(), 72 * 60 * 60);
    let token_client = TokenClient::new(&env, &token_address);
    let mentor_before = token_client.balance(&mentor);
    let learner_before = token_client.balance(&learner);
    v2.dispute(&learner, &id1, &symbol_short!("NO_SHOW"));
    v2.resolve_dispute(&id1, &50u32);
    let e1_post = v2.get_escrow(&id1);
    assert_eq!(e1_post.status, EscrowStatusV2::Resolved);
    assert_eq!(token_client.balance(&mentor), mentor_before + 500);
    assert_eq!(token_client.balance(&learner), learner_before + 500);
    let id2_before_mentor = token_client.balance(&mentor);
    let id2_before_treasury = token_client.balance(&treasury);
    advance_time(&env, 72 * 60 * 60 + 1);
    v2.try_auto_release(&id2);
    let e2_post = v2.get_escrow(&id2);
    assert_eq!(e2_post.status, EscrowStatusV2::Released);
    assert_eq!(token_client.balance(&mentor), id2_before_mentor + 950);
    assert_eq!(token_client.balance(&treasury), id2_before_treasury + 50);
    let reinit = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let other = Address::generate(&env);
        let empty: Vec<Address> = Vec::new(&env);
        v2.initialize(&other, &treasury, &500u32, &empty, &0u64);
    }));
    assert!(reinit.is_err());
}
