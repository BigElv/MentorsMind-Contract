#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, token, Address, Env, Symbol, Vec, IntoVal, BytesN};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EscrowStatus {
    Active,
    Released,
    Disputed,
    Refunded,
    Resolved,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MilestoneStatus {
    Pending,
    Completed,
    Disputed,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct MilestoneSpec {
    pub description_hash: BytesN<32>,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct MilestoneEscrow {
    pub id: u64,
    pub mentor: Address,
    pub learner: Address,
    pub total_amount: i128,
    pub milestones: Vec<MilestoneSpec>,
    pub milestone_statuses: Vec<MilestoneStatus>,
    pub status: EscrowStatus,
    pub created_at: u64,
    pub token_address: Address,
    pub platform_fee: i128,
    pub net_amount: i128,
}

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
    pub token_address: Address,
    pub platform_fee: i128,
    pub net_amount: i128,
    pub session_end_time: u64,
    pub auto_release_delay: u64,
    pub dispute_reason: Symbol,
    pub resolved_at: u64,
    pub usd_amount: i128,
    pub quoted_token_amount: i128,
    pub send_asset: Address,
    pub dest_asset: Address,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct EscrowLegacy {
    pub id: u64,
    pub mentor: Address,
    pub learner: Address,
    pub amount: i128,
    pub session_id: Symbol,
    pub status: EscrowStatus,
    pub created_at: u64,
    pub token_address: Address,
    pub platform_fee: i128,
    pub net_amount: i128,
    pub session_end_time: u64,
    pub auto_release_delay: u64,
    pub dispute_reason: Symbol,
    pub resolved_at: u64,
}

const ESCROW_COUNT: Symbol = symbol_short!("ESC_CNT");
const MILESTONE_ESCROW_COUNT: Symbol = symbol_short!("MESC_CNT");
const ADMIN: Symbol = symbol_short!("ADMIN");
const TREASURY: Symbol = symbol_short!("TREASURY");
const FEE_BPS: Symbol = symbol_short!("FEE_BPS");
const AUTO_REL_DLY: Symbol = symbol_short!("AR_DELAY");
const SESSION_KEY: Symbol = symbol_short!("SESSION");
const ORACLE_ID: Symbol = symbol_short!("ORACLE");
const ORACLE_MAX_AGE: Symbol = symbol_short!("OR_AGE");
const MAX_FEE_BPS: u32 = 1_000;
const DEFAULT_AUTO_RELEASE_DELAY: u64 = 72 * 60 * 60;
const APPROVED_TOKEN_KEY: Symbol = symbol_short!("APRV_TOK");
const ESCROW_TTL_THRESHOLD: u32 = 500_000;
const ESCROW_TTL_BUMP: u32 = 1_000_000;

#[contract]
pub struct EscrowContract;

#[contractimpl]
impl EscrowContract {
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
        env.storage()
            .persistent()
            .extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        env.storage().persistent().set(&TREASURY, &treasury);
        env.storage()
            .persistent()
            .extend_ttl(&TREASURY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        env.storage().persistent().set(&FEE_BPS, &fee_bps);
        env.storage()
            .persistent()
            .extend_ttl(&FEE_BPS, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        env.storage().persistent().set(&ESCROW_COUNT, &0u64);
        env.storage()
            .persistent()
            .extend_ttl(&ESCROW_COUNT, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        env.storage().persistent().set(&MILESTONE_ESCROW_COUNT, &0u64);
        env.storage()
            .persistent()
            .extend_ttl(&MILESTONE_ESCROW_COUNT, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        // Store configurable auto-release delay; fall back to 72 hours if 0.
        let delay = if auto_release_delay_secs == 0 {
            DEFAULT_AUTO_RELEASE_DELAY
        } else {
            auto_release_delay_secs
        };
        env.storage().persistent().set(&AUTO_REL_DLY, &delay);
        env.storage()
            .persistent()
            .extend_ttl(&AUTO_REL_DLY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        for token_addr in approved_tokens.iter() {
            Self::_set_token_approved(&env, &token_addr, true);
        }
    }

    pub fn update_fee(env: Env, new_fee_bps: u32) {
        let admin = Self::admin(&env);
        admin.require_auth();

        if new_fee_bps > MAX_FEE_BPS {
            panic!("Fee exceeds maximum (1000 bps)");
        }

        env.storage().persistent().set(&FEE_BPS, &new_fee_bps);
        env.storage()
            .persistent()
            .extend_ttl(&FEE_BPS, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
    }

    pub fn update_treasury(env: Env, new_treasury: Address) {
        let admin = Self::admin(&env);
        admin.require_auth();

        env.storage().persistent().set(&TREASURY, &new_treasury);
        env.storage()
            .persistent()
            .extend_ttl(&TREASURY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
    }

    pub fn set_approved_token(env: Env, token_address: Address, approved: bool) {
        let admin = Self::admin(&env);
        admin.require_auth();
        Self::_set_token_approved(&env, &token_address, approved);
    }

    pub fn set_oracle(env: Env, oracle: Address, max_age_secs: u64) {
        let admin = Self::admin(&env);
        admin.require_auth();
        env.storage().persistent().set(&ORACLE_ID, &oracle);
        env.storage().persistent().set(&ORACLE_MAX_AGE, &max_age_secs);
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
        Self::_create_escrow_internal(env, mentor, learner, amount, session_id, token_address.clone(), session_end_time, 0, amount, token_address.clone(), token_address)
    }

    pub fn release_funds(env: Env, caller: Address, escrow_id: u64) {
        let key = (symbol_short!("ESCROW"), escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let mut escrow = Self::load_escrow(&env, &key);

        if escrow.status != EscrowStatus::Active {
            panic!("Escrow not active");
        }

        let admin: Address = env
            .storage()
            .persistent()
            .get(&ADMIN)
            .expect("Admin not found");
        env.storage()
            .persistent()
            .extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        // Auth check: caller must be learner OR admin
        caller.require_auth();
        if caller != escrow.learner && caller != admin {
            panic!("Caller not authorized");
        }

        Self::_do_release(&env, &mut escrow, &key);
    }
    pub fn release_partial(env: Env, caller: Address, escrow_id: u64, amount_to_release: i128) {
        let key = (symbol_short!("ESCROW"), escrow_id);
        env.storage().persistent().extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let mut escrow = Self::load_escrow(&env, &key);

        if escrow.status != EscrowStatus::Active {
            panic!("Escrow not active");
        }

        if amount_to_release <= 0 || amount_to_release > escrow.amount {
            panic!("Invalid release amount");
        }

        let admin: Address = env.storage().persistent().get(&ADMIN).expect("Admin not found");
        env.storage().persistent().extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        caller.require_auth();
        if caller != escrow.learner && caller != admin {
            panic!("Caller not authorized");
        }

        let fee_bps: u32 = env.storage().persistent().get(&FEE_BPS).unwrap_or(0u32);
        env.storage().persistent().extend_ttl(&FEE_BPS, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let platform_fee: i128 = amount_to_release.checked_mul(fee_bps as i128).expect("Overflow").checked_div(10_000).expect("Division error");
        let net_amount: i128 = amount_to_release.checked_sub(platform_fee).expect("Underflow");

        let treasury: Address = env.storage().persistent().get(&TREASURY).expect("Treasury not found");
        env.storage().persistent().extend_ttl(&TREASURY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let token_client = soroban_sdk::token::Client::new(&env, &escrow.token_address);

        if platform_fee > 0 {
            token_client.transfer(&env.current_contract_address(), &treasury, &platform_fee);
        }

        token_client.transfer(&env.current_contract_address(), &escrow.mentor, &net_amount);

        escrow.amount = escrow.amount.checked_sub(amount_to_release).expect("Underflow");
        escrow.platform_fee = escrow.platform_fee.checked_add(platform_fee).expect("Overflow");
        escrow.net_amount = escrow.net_amount.checked_add(net_amount).expect("Overflow");

        if escrow.amount == 0 {
            escrow.status = EscrowStatus::Released;
        }

        env.storage().persistent().set(&key, &escrow);

        env.events().publish((symbol_short!("Escrow"), symbol_short!("rel_part"), escrow.id), (amount_to_release, net_amount));
    }

    pub fn admin_release(env: Env, escrow_id: u64) {
        let key = (symbol_short!("ESCROW"), escrow_id);
        env.storage().persistent().extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let mut escrow = Self::load_escrow(&env, &key);

        if escrow.status != EscrowStatus::Active {
            panic!("Escrow not active");
        }

        let admin: Address = env.storage().persistent().get(&ADMIN).expect("Admin not found");
        env.storage().persistent().extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        admin.require_auth();

        env.events().publish((symbol_short!("Escrow"), symbol_short!("adm_rel"), escrow_id), (escrow_id, env.ledger().timestamp()));

        Self::_do_release(&env, &mut escrow, &key);
    }


    /// Permissionless auto-release.
    ///
    /// Anyone may call this once `env.ledger().timestamp() >=
    /// escrow.session_end_time + escrow.auto_release_delay` and the escrow is
    /// still `Active`. Funds are released to the mentor using the same fee
    /// logic as `release_funds`.
    ///
    /// Panics if:
    /// - Escrow does not exist.
    /// - Escrow status is not `Active`.
    /// - The auto-release window has not yet elapsed.
    pub fn try_auto_release(env: Env, escrow_id: u64) {
        let key = (symbol_short!("ESCROW"), escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let mut escrow = Self::load_escrow(&env, &key);

        if escrow.status != EscrowStatus::Active {
            panic!("Escrow not active");
        }

        let now = env.ledger().timestamp();
        let release_after = escrow
            .session_end_time
            .checked_add(escrow.auto_release_delay)
            .expect("Timestamp overflow");

        if now < release_after {
            panic!("Auto-release window has not elapsed");
        }

        env.events().publish((symbol_short!("Escrow"), symbol_short!("auto_rel"), escrow_id), now);
        Self::_do_release(&env, &mut escrow, &key);
    }


    pub fn dispute(env: Env, caller: Address, escrow_id: u64, reason: Symbol) {
        let key = (symbol_short!("ESCROW"), escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let mut escrow: Escrow = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Escrow not found");

        if escrow.status != EscrowStatus::Active {
            panic!("Escrow not active");
        }

        // Auth check: caller must be mentor OR learner
        caller.require_auth();
        if caller != escrow.mentor && caller != escrow.learner {
            panic!("Caller not authorized to dispute");
        }

        escrow.status = EscrowStatus::Disputed;
        escrow.dispute_reason = reason.clone();
        env.storage().persistent().set(&key, &escrow);

        env.events().publish((symbol_short!("Escrow"), symbol_short!("disp_opn"), escrow_id), reason);
    }

    pub fn resolve_dispute(env: Env, escrow_id: u64, release_to_mentor: bool) {
        let admin = Self::admin(&env);
        admin.require_auth();
        let key = (symbol_short!("ESCROW"), escrow_id);
        env.storage().persistent().extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        let mut escrow = Self::load_escrow(&env, &key);
        if escrow.status != EscrowStatus::Disputed {
            panic!("Escrow is not in Disputed status");
        }
        let token_client = token::Client::new(&env, &escrow.token_address);
        let treasury: Address = env.storage().persistent().get(&TREASURY).expect("Treasury not found");
        let fee_amount = if release_to_mentor { escrow.amount * 5 / 100 } else { 0 };
        let mentor_amount = if release_to_mentor { escrow.amount - fee_amount } else { 0 };
        let learner_amount = if release_to_mentor { 0 } else { escrow.amount };
        if fee_amount > 0 {
            token_client.transfer(&env.current_contract_address(), &treasury, &fee_amount);
        }
        if mentor_amount > 0 {
            token_client.transfer(&env.current_contract_address(), &escrow.mentor, &mentor_amount);
        }
        if learner_amount > 0 {
            token_client.transfer(&env.current_contract_address(), &escrow.learner, &learner_amount);
        }
        escrow.status = EscrowStatus::Resolved;
        escrow.net_amount = mentor_amount;
        escrow.platform_fee = if release_to_mentor { fee_amount } else { learner_amount };
        escrow.amount = 0;
        escrow.resolved_at = env.ledger().timestamp();
        env.storage().persistent().set(&key, &escrow);
        let session_key = (SESSION_KEY, escrow.session_id.clone());
        env.storage().persistent().remove(&session_key);
    }

    pub fn refund(env: Env, escrow_id: u64) {
        let admin = Self::admin(&env);
        admin.require_auth();

        let key = (symbol_short!("ESCROW"), escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let mut escrow = Self::load_escrow(&env, &key);

        if escrow.status == EscrowStatus::Released
            || escrow.status == EscrowStatus::Refunded
            || escrow.status == EscrowStatus::Resolved
        {
            panic!("Cannot refund");
        }

        let token_client = token::Client::new(&env, &escrow.token_address);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.learner,
            &escrow.amount,
        );

        escrow.status = EscrowStatus::Refunded;
        env.storage().persistent().set(&key, &escrow);
        let session_key = (SESSION_KEY, escrow.session_id.clone());
        env.storage().persistent().remove(&session_key);

        env.events().publish((symbol_short!("Escrow"), symbol_short!("refund"), escrow_id), escrow.learner);
    }

    pub fn get_escrow(env: Env, escrow_id: u64) -> Escrow {
        let key = (symbol_short!("ESCROW"), escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        Self::load_escrow(&env, &key)
    }

    pub fn get_escrow_count(env: Env) -> u64 {
        env.storage()
            .persistent()
            .extend_ttl(&ESCROW_COUNT, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        env.storage().persistent().get(&ESCROW_COUNT).unwrap_or(0)
    }

    pub fn get_fee_bps(env: Env) -> u32 {
        env.storage()
            .persistent()
            .extend_ttl(&FEE_BPS, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        env.storage().persistent().get(&FEE_BPS).unwrap_or(0)
    }

    pub fn get_treasury(env: Env) -> Address {
        env.storage()
            .persistent()
            .extend_ttl(&TREASURY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        env.storage()
            .persistent()
            .get(&TREASURY)
            .expect("Treasury not set")
    }

    pub fn get_auto_release_delay(env: Env) -> u64 {
        env.storage()
            .persistent()
            .extend_ttl(&AUTO_REL_DLY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        env.storage()
            .persistent()
            .get(&AUTO_REL_DLY)
            .unwrap_or(DEFAULT_AUTO_RELEASE_DELAY)
    }

    pub fn is_token_approved(env: Env, token_address: Address) -> bool {
        Self::_is_token_approved(&env, &token_address)
    }

    pub fn get_escrows_by_mentor(env: Env, mentor: Address) -> Vec<Escrow> {
        let count = env.storage().persistent().get(&ESCROW_COUNT).unwrap_or(0u64);
        let mut result = Vec::new(&env);

        for i in 1..=count {
            let key = (symbol_short!("ESCROW"), i);
            if let Some(escrow) = env.storage().persistent().get::<_, Escrow>(&key) {
                if escrow.mentor == mentor {
                    result.push_back(escrow);
                }
            }
        }

        result
    }

    pub fn submit_review(env: Env, caller: Address, escrow_id: u64, reason: Symbol) {
        let key = (symbol_short!("ESCROW"), escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let escrow = Self::load_escrow(&env, &key);

        caller.require_auth();
        if caller != escrow.learner {
            panic!("Only learner can submit review");
        }

        if escrow.status != EscrowStatus::Released {
            panic!("Can only review released escrows");
        }
        let review_key = (symbol_short!("REVIEW"), escrow_id);
        env.storage().persistent().set(&review_key, &reason);
        env.storage()
            .persistent()
            .extend_ttl(&review_key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        env.events().publish((symbol_short!("Escrow"), symbol_short!("review"), escrow_id), reason);
    }

    pub fn create_escrow_usd(env: Env, mentor: Address, learner: Address, usd_amount: i128, token_address: Address) -> u64 {
        let oracle: Address = env.storage().persistent().get(&ORACLE_ID).expect("oracle not set");
        let max_age: u64 = env.storage().persistent().get(&ORACLE_MAX_AGE).unwrap_or(300);
        let oracle_sym = Symbol::new(&env, "get_price");
        let (price, updated_at): (i128, u64) = env.invoke_contract(&oracle, &oracle_sym, (Symbol::new(&env, "USD"),).into_val(&env));
        let now = env.ledger().timestamp();
        if now.saturating_sub(updated_at) > max_age || price <= 0 {
            panic!("stale oracle");
        }
        let token_amount = usd_amount.checked_mul(10_000_000).expect("overflow").checked_div(price).expect("div");
        env.events().publish((symbol_short!("Escrow"), symbol_short!("usd_rate"), learner.clone()), (usd_amount, price, token_amount));
        Self::_create_escrow_internal(env, mentor, learner, token_amount, symbol_short!("USD_SES"), token_address.clone(), now, usd_amount, token_amount, token_address.clone(), token_address)
    }

    pub fn create_escrow_with_path_payment(
        env: Env,
        learner: Address,
        mentor: Address,
        send_asset: Address,
        send_max: i128,
        dest_asset: Address,
        dest_amount: i128,
        _path: Vec<Address>,
    ) -> u64 {
        if send_max < dest_amount {
            panic!("path slippage exceeded");
        }
        let rate_scaled = if dest_amount == 0 { 0 } else { send_max * 10_000_000 / dest_amount };
        env.events().publish((symbol_short!("Escrow"), symbol_short!("path_pay"), learner.clone()), rate_scaled);
        Self::_create_escrow_internal(
            env,
            mentor,
            learner,
            dest_amount,
            symbol_short!("PATHPAY"),
            dest_asset.clone(),
            0,
            0,
            dest_amount,
            send_asset,
            dest_asset,
        )
    }

    fn _do_release(env: &Env, escrow: &mut Escrow, key: &(Symbol, u64)) {
        let release_amount = escrow.amount;
        let fee_bps: u32 = env.storage().persistent().get(&FEE_BPS).unwrap_or(0u32);
        env.storage()
            .persistent()
            .extend_ttl(&FEE_BPS, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let platform_fee: i128 = release_amount
            .checked_mul(fee_bps as i128)
            .expect("Overflow")
            .checked_div(10_000)
            .expect("Division error");
        let net_amount: i128 = release_amount
            .checked_sub(platform_fee)
            .expect("Underflow");

        let treasury: Address = env
            .storage()
            .persistent()
            .get(&TREASURY)
            .expect("Treasury not found");
        env.storage()
            .persistent()
            .extend_ttl(&TREASURY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let token_client = token::Client::new(env, &escrow.token_address);

        if platform_fee > 0 {
            token_client.transfer(&env.current_contract_address(), &treasury, &platform_fee);
        }

        token_client.transfer(&env.current_contract_address(), &escrow.mentor, &net_amount);

        escrow.status = EscrowStatus::Released;
        escrow.platform_fee = escrow.platform_fee.checked_add(platform_fee).expect("Overflow");
        escrow.net_amount = escrow.net_amount.checked_add(net_amount).expect("Overflow");
        escrow.amount = 0; // all remaining amount is released
        env.storage().persistent().set(key, escrow);

        let session_key = (SESSION_KEY, escrow.session_id.clone());
        env.storage().persistent().remove(&session_key);
        env.events().publish((symbol_short!("released"), escrow.id), (release_amount, net_amount, platform_fee));
    }

    fn _set_token_approved(env: &Env, token_address: &Address, approved: bool) {
        let key = (APPROVED_TOKEN_KEY, token_address.clone());
        env.storage().persistent().set(&key, &approved);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
    }

    fn _is_token_approved(env: &Env, token_address: &Address) -> bool {
        let key = (APPROVED_TOKEN_KEY, token_address.clone());
        env.storage()
            .persistent()
            .get::<_, bool>(&key)
            .unwrap_or(false)
    }

    fn load_escrow(env: &Env, key: &(Symbol, u64)) -> Escrow {
        if let Some(current) = env.storage().persistent().get::<_, Escrow>(key) {
            return current;
        }
        if let Some(old) = env.storage().persistent().get::<_, EscrowLegacy>(key) {
            return Escrow {
                id: old.id,
                mentor: old.mentor,
                learner: old.learner,
                amount: old.amount,
                session_id: old.session_id,
                status: old.status,
                created_at: old.created_at,
                token_address: old.token_address.clone(),
                platform_fee: old.platform_fee,
                net_amount: old.net_amount,
                session_end_time: old.session_end_time,
                auto_release_delay: old.auto_release_delay,
                dispute_reason: old.dispute_reason,
                resolved_at: old.resolved_at,
                usd_amount: 0,
                quoted_token_amount: old.amount,
                send_asset: old.token_address.clone(),
                dest_asset: old.token_address,
            };
        }
        panic!("Escrow not found");
    }
    fn _create_escrow_internal(
        env: Env,
        mentor: Address,
        learner: Address,
        amount: i128,
        session_id: Symbol,
        token_address: Address,
        session_end_time: u64,
        usd_amount: i128,
        quoted_token_amount: i128,
        send_asset: Address,
        dest_asset: Address,
    ) -> u64 {
        if amount <= 0 {
            panic!("Amount must be greater than zero");
        }
        if !Self::_is_token_approved(&env, &token_address) {
            panic!("Token not approved");
        }
        learner.require_auth();
        let token_client = token::Client::new(&env, &token_address);
        if token_client.balance(&learner) < amount {
            panic!("Insufficient token balance");
        }
        let auto_release_delay: u64 = env.storage().persistent().get(&AUTO_REL_DLY).unwrap_or(DEFAULT_AUTO_RELEASE_DELAY);
        let session_key = (SESSION_KEY, session_id.clone());
        if env.storage().persistent().has(&session_key) {
            panic!("Session ID already exists");
        }
        env.storage().persistent().set(&session_key, &true);
        let mut count: u64 = env.storage().persistent().get(&ESCROW_COUNT).unwrap_or(0);
        count += 1;
        env.storage().persistent().set(&ESCROW_COUNT, &count);
        token_client.transfer(&learner, &env.current_contract_address(), &amount);
        let escrow = Escrow {
            id: count,
            mentor: mentor.clone(),
            learner: learner.clone(),
            amount,
            session_id: session_id.clone(),
            status: EscrowStatus::Active,
            created_at: env.ledger().timestamp(),
            token_address: token_address.clone(),
            platform_fee: 0,
            net_amount: 0,
            session_end_time,
            auto_release_delay,
            dispute_reason: symbol_short!(""),
            resolved_at: 0,
            usd_amount,
            quoted_token_amount,
            send_asset,
            dest_asset,
        };
        let key = (symbol_short!("ESCROW"), count);
        env.storage().persistent().set(&key, &escrow);
        env.events().publish((symbol_short!("created"), count), (mentor, learner, amount, token_address));
        count
    }

    fn admin(env: &Env) -> Address {
        let admin: Address = env.storage().persistent().get(&ADMIN).expect("Not initialized");
        env.storage().persistent().extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        admin
    }

    pub fn create_milestone_escrow(
        env: Env,
        mentor: Address,
        learner: Address,
        milestones: Vec<MilestoneSpec>,
        token_address: Address,
    ) -> u64 {
        if milestones.is_empty() {
            panic!("At least one milestone required");
        }
        
        if !Self::_is_token_approved(&env, &token_address) {
            panic!("Token not approved");
        }
        
        let total_amount = milestones.iter().fold(0i128, |acc, m| acc.checked_add(m.amount).expect("Amount overflow"));
        
        if total_amount <= 0 {
            panic!("Total amount must be greater than zero");
        }
        
        learner.require_auth();
        
        let token_client = token::Client::new(&env, &token_address);
        if token_client.balance(&learner) < total_amount {
            panic!("Insufficient token balance");
        }
        
        let mut count: u64 = env.storage().persistent().get(&MILESTONE_ESCROW_COUNT).unwrap_or(0);
        count += 1;
        env.storage().persistent().set(&MILESTONE_ESCROW_COUNT, &count);
        env.storage()
            .persistent()
            .extend_ttl(&MILESTONE_ESCROW_COUNT, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        
        token_client.transfer(&learner, &env.current_contract_address(), &total_amount);
        
        let milestone_statuses: Vec<MilestoneStatus> = (0..milestones.len())
            .map(|_| MilestoneStatus::Pending)
            .collect();
        
        let milestone_escrow = MilestoneEscrow {
            id: count,
            mentor: mentor.clone(),
            learner: learner.clone(),
            total_amount,
            milestones: milestones.clone(),
            milestone_statuses,
            status: EscrowStatus::Active,
            created_at: env.ledger().timestamp(),
            token_address: token_address.clone(),
            platform_fee: 0,
            net_amount: 0,
        };
        
        let key = (symbol_short!("MESCROW"), count);
        env.storage().persistent().set(&key, &milestone_escrow);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        
        env.events().publish(
            (symbol_short!("milestone_created"), count),
            (mentor, learner, total_amount, milestones.len())
        );
        
        count
    }
    
    pub fn complete_milestone(env: Env, escrow_id: u64, milestone_index: u32) {
        let key = (symbol_short!("MESCROW"), escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        
        let mut milestone_escrow: MilestoneEscrow = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Milestone escrow not found");
        
        if milestone_escrow.status != EscrowStatus::Active {
            panic!("Milestone escrow not active");
        }
        
        if milestone_index as usize >= milestone_escrow.milestones.len() {
            panic!("Invalid milestone index");
        }
        
        let current_status = milestone_escrow.milestone_statuses.get(milestone_index as usize).unwrap();
        if *current_status != MilestoneStatus::Pending {
            panic!("Milestone not pending");
        }
        
        milestone_escrow.learner.require_auth();
        
        let milestone = &milestone_escrow.milestones[milestone_index as usize];
        let fee_bps: u32 = env.storage().persistent().get(&FEE_BPS).unwrap_or(0u32);
        env.storage()
            .persistent()
            .extend_ttl(&FEE_BPS, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        
        let platform_fee: i128 = milestone
            .amount
            .checked_mul(fee_bps as i128)
            .expect("Overflow")
            .checked_div(10_000)
            .expect("Division error");
        let net_amount: i128 = milestone.amount.checked_sub(platform_fee).expect("Underflow");
        
        let treasury: Address = env.storage().persistent().get(&TREASURY).expect("Treasury not found");
        env.storage()
            .persistent()
            .extend_ttl(&TREASURY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        
        let token_client = token::Client::new(&env, &milestone_escrow.token_address);
        
        if platform_fee > 0 {
            token_client.transfer(&env.current_contract_address(), &treasury, &platform_fee);
        }
        
        token_client.transfer(&env.current_contract_address(), &milestone_escrow.mentor, &net_amount);
        
        milestone_escrow.milestone_statuses[milestone_index as usize] = MilestoneStatus::Completed;
        milestone_escrow.platform_fee = milestone_escrow.platform_fee.checked_add(platform_fee).expect("Overflow");
        milestone_escrow.net_amount = milestone_escrow.net_amount.checked_add(net_amount).expect("Overflow");
        
        let all_completed = milestone_escrow.milestone_statuses.iter().all(|s| *s == MilestoneStatus::Completed);
        if all_completed {
            milestone_escrow.status = EscrowStatus::Released;
        }
        
        env.storage().persistent().set(&key, &milestone_escrow);
        
        env.events().publish(
            (symbol_short!("milestone_completed"), escrow_id),
            (milestone_index, milestone.amount, net_amount)
        );
    }
    
    pub fn dispute_milestone(env: Env, escrow_id: u64, milestone_index: u32, reason: Symbol) {
        let key = (symbol_short!("MESCROW"), escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        
        let mut milestone_escrow: MilestoneEscrow = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Milestone escrow not found");
        
        if milestone_escrow.status != EscrowStatus::Active {
            panic!("Milestone escrow not active");
        }
        
        if milestone_index as usize >= milestone_escrow.milestones.len() {
            panic!("Invalid milestone index");
        }
        
        let current_status = milestone_escrow.milestone_statuses.get(milestone_index as usize).unwrap();
        if *current_status != MilestoneStatus::Pending {
            panic!("Milestone not pending");
        }
        
        milestone_escrow.mentor.require_auth();
        milestone_escrow.learner.require_auth();
        
        milestone_escrow.milestone_statuses[milestone_index as usize] = MilestoneStatus::Disputed;
        milestone_escrow.status = EscrowStatus::Disputed;
        
        env.storage().persistent().set(&key, &milestone_escrow);
        
        env.events().publish(
            (symbol_short!("milestone_disputed"), escrow_id),
            (milestone_index, reason)
        );
    }
    
    pub fn get_milestone_escrow(env: Env, escrow_id: u64) -> MilestoneEscrow {
        let key = (symbol_short!("MESCROW"), escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        env.storage()
            .persistent()
            .get(&key)
            .expect("Milestone escrow not found")
    }
    
    pub fn get_milestone_escrow_count(env: Env) -> u64 {
        env.storage()
            .persistent()
            .extend_ttl(&MILESTONE_ESCROW_COUNT, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        env.storage().persistent().get(&MILESTONE_ESCROW_COUNT).unwrap_or(0)
    }
}
