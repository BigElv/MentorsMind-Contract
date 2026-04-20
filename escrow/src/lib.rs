#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, BytesN, Env, Symbol, Vec,
};

// ---------------------------------------------------------------------------
// Types — escrow
// ---------------------------------------------------------------------------

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
    pub total_sessions: u32,
    pub sessions_completed: u32,
}

// ---------------------------------------------------------------------------
// Group session escrow (issue #91)
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GroupEscrowStatus {
    Open,
    Full,
    Active,
    Released,
    Cancelled,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct GroupEscrow {
    pub id: u64,
    pub mentor: Address,
    pub max_learners: u32,
    pub price_per_learner: i128,
    pub token_address: Address,
    pub session_id: Symbol,
    pub learners: Vec<Address>,
    pub status: GroupEscrowStatus,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LearnerJoinedEventData {
    pub escrow_id: u64,
    pub learner: Address,
    pub learners_count: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupStartedEventData {
    pub escrow_id: u64,
    pub learner_count: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupReleasedEventData {
    pub escrow_id: u64,
    pub gross: i128,
    pub net_amount: i128,
    pub platform_fee: i128,
    pub token_address: Address,
}

// ---------------------------------------------------------------------------
// Events — standard escrow
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowCreatedEventData {
    pub mentor: Address,
    pub learner: Address,
    pub amount: i128,
    pub session_id: Symbol,
    pub token_address: Address,
    pub session_end_time: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowReleasedEventData {
    pub mentor: Address,
    pub amount: i128,
    pub net_amount: i128,
    pub platform_fee: i128,
    pub token_address: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowAutoReleasedEventData {
    pub time: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeOpenedEventData {
    pub caller: Address,
    pub reason: Symbol,
    pub token_address: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeResolvedEventData {
    pub mentor_pct: u32,
    pub mentor_amount: i128,
    pub learner_amount: i128,
    pub token_address: Address,
    pub time: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowRefundedEventData {
    pub learner: Address,
    pub amount: i128,
    pub token_address: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewSubmittedEventData {
    pub caller: Address,
    pub reason: Symbol,
    pub mentor: Address,
}

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

const ESCROW_COUNT: Symbol = symbol_short!("ESC_CNT");
const GROUP_ESCROW_COUNT: Symbol = symbol_short!("G_ESCNT");
const MILESTONE_ESCROW_COUNT: Symbol = symbol_short!("MESC_CNT");
const ADMIN: Symbol = symbol_short!("ADMIN");
const TREASURY: Symbol = symbol_short!("TREASURY");
const FEE_BPS: Symbol = symbol_short!("FEE_BPS");
const AUTO_REL_DLY: Symbol = symbol_short!("AR_DELAY");
/// Contract version for upgrade tracking
const CONTRACT_VERSION: Symbol = symbol_short!("CONTRACT_VER");

/// Maximum configurable fee: 10% = 1 000 basis points.
const SESSION_KEY: Symbol = symbol_short!("SESSION");
const MENTOR_ESCROWS: Symbol = symbol_short!("MNT_ESC");
const LEARNER_ESCROWS: Symbol = symbol_short!("LRN_ESC");
const MAX_FEE_BPS: u32 = 1_000;
const DEFAULT_AUTO_RELEASE_DELAY: u64 = 72 * 60 * 60;

const ESCROW_TTL_THRESHOLD: u32 = 500_000;
const ESCROW_TTL_BUMP: u32 = 1_000_000;

const ESCROW_SYM: Symbol = symbol_short!("ESCROW");
const GROUP_ESCROW_SYM: Symbol = symbol_short!("GR_ESC");
const MESCROW_SYM: Symbol = symbol_short!("MESCROW");

#[contract]
pub struct EscrowContract;

#[contractimpl]
impl EscrowContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        treasury: Address,
        fee_bps: u32,
        approved_tokens: soroban_sdk::Vec<Address>,
        auto_release_delay_secs: u64
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

        env.storage().persistent().set(&GROUP_ESCROW_COUNT, &0u64);
        env.storage().persistent().extend_ttl(
            &GROUP_ESCROW_COUNT,
            ESCROW_TTL_THRESHOLD,
            ESCROW_TTL_BUMP,
        );

        env.storage()
            .persistent()
            .set(&MILESTONE_ESCROW_COUNT, &0u64);
        env.storage().persistent().extend_ttl(
            &MILESTONE_ESCROW_COUNT,
            ESCROW_TTL_THRESHOLD,
            ESCROW_TTL_BUMP,
        );

        let delay = if auto_release_delay_secs == 0 {
            DEFAULT_AUTO_RELEASE_DELAY
        } else {
            auto_release_delay_secs
        };
        env.storage().persistent().set(&AUTO_REL_DLY, &delay);
        env.storage().persistent().extend_ttl(&AUTO_REL_DLY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        env.storage()
            .persistent()
            .extend_ttl(&AUTO_REL_DLY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        for token_addr in approved_tokens.iter() {
            Self::_set_token_approved(&env, &token_addr, true);
        }

        // Initialize contract version (starts at 1)
        env.storage().persistent().set(&CONTRACT_VERSION, &1u32);
        env.storage()
            .persistent()
            .extend_ttl(&CONTRACT_VERSION, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
    }

    /// Update the platform fee — admin only, capped at 1 000 bps (10%).
    pub fn update_fee(env: Env, new_fee_bps: u32) {
        let admin: Address = env.storage().persistent().get(&ADMIN).expect("Not initialized");
        env.storage().persistent().extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        admin.require_auth();
        if new_fee_bps > MAX_FEE_BPS {
            panic!("Fee exceeds maximum (1000 bps)");
        }

        env.storage().persistent().set(&FEE_BPS, &new_fee_bps);
        env.storage().persistent().extend_ttl(&FEE_BPS, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
    }

    /// Update the treasury address — admin only.
    pub fn update_treasury(env: Env, new_treasury: Address) {
        let admin: Address = env.storage().persistent().get(&ADMIN).expect("Not initialized");
        env.storage().persistent().extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        admin.require_auth();

        env.storage().persistent().set(&TREASURY, &new_treasury);
        env.storage().persistent().extend_ttl(&TREASURY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
    }

    /// Add or remove an approved token (admin only).
    pub fn set_approved_token(env: Env, token_address: Address, approved: bool) {
        let admin: Address = env.storage().persistent().get(&ADMIN).expect("Not initialized");
        env.storage().persistent().extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        admin.require_auth();
        Self::_set_token_approved(&env, &token_address, approved);
    }

    // -----------------------------------------------------------------------
    // Single-learner escrow
    // -----------------------------------------------------------------------

    /// Create a new escrow.
    ///
    /// Transfers `amount` tokens from `learner` to the contract.
    ///
    /// - `session_end_time`: unix timestamp (seconds) marking when the session
    ///   ends. After this plus the contract's `auto_release_delay`, anyone may
    ///   call `try_auto_release` to release funds to the mentor.
    ///
    /// Panics if:
    /// - `amount` ≤ 0
    /// - `token_address` is not on the approved list
    /// - learner's on-chain balance is insufficient
    pub fn create_escrow(
        env: Env,
        mentor: Address,
        learner: Address,
        amount: i128,
        session_id: Symbol,
        token_address: Address,
        session_end_time: u64
    ) -> u64 {
        // --- Validate amount ---
        if amount <= 0 {
            panic!("Amount must be greater than zero");
        }

        // --- Validate approved token ---
        if !Self::_is_token_approved(&env, &token_address) {
            panic!("Token not approved");
        }

        // --- Require learner authorization ---
        learner.require_auth();

        // --- Balance check (SEP-41: balance()) ---
        let token_client = token::Client::new(&env, &token_address);
        let learner_balance = token_client.balance(&learner);
        if learner_balance < amount {
            panic!("Insufficient token balance");
        }

        // --- Retrieve global auto-release delay ---
        let auto_release_delay: u64 = env
            .storage()
            .persistent()
            .get(&AUTO_REL_DLY)
            .unwrap_or(DEFAULT_AUTO_RELEASE_DELAY);
        env.storage().persistent().extend_ttl(&AUTO_REL_DLY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        // --- Increment and persist escrow counter ---
        let mut count: u64 = env.storage().persistent().get(&ESCROW_COUNT).unwrap_or(0);
        count = count.checked_add(1).expect("Counter overflow");
        env.storage().persistent().set(&ESCROW_COUNT, &count);
        env.storage().persistent().extend_ttl(&ESCROW_COUNT, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        // --- Transfer tokens from learner → contract ---
        token_client.transfer(&learner, &env.current_contract_address(), &amount);

        // --- Persist escrow ---
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
        };

        let key = (symbol_short!("ESCROW"), count);
        env.storage().persistent().set(&key, &escrow);
        env.storage().persistent().extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        // --- Emit event (includes token_address and session_end_time) ---
        env.events().publish(
            (symbol_short!("created"), count),
            (mentor, learner, amount, session_id, token_address, session_end_time)
        );

        count
    }

    /// Release funds to the mentor (called by learner or admin).
    ///
    /// Calculates the platform fee (`gross * fee_bps / 10_000`), transfers the
    /// fee to the treasury, and transfers the remainder to the mentor.
    /// Both amounts are stored on the escrow record and emitted in the event.
    pub fn release_funds(env: Env, caller: Address, escrow_id: u64) {
        let key = (symbol_short!("ESCROW"), escrow_id);
        env.storage().persistent().extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        learner.require_auth();
        if amount <= 0 {
            panic!("Amount must be greater than zero");
        }
        if !Self::_is_token_approved(&env, &token_address) {
            panic!("Token not approved");
        }
        if total_sessions == 0 {
            panic!("total_sessions must be at least 1");
        }

        let session_dup_key = (SESSION_KEY, session_id.clone());
        if env.storage().persistent().has(&session_dup_key) {
            panic!("Session ID already used");
        }

        let token_client = token::Client::new(&env, &token_address);
        if token_client.balance(&learner) < amount {
            panic!("Insufficient token balance");
        }

        let auto_release_delay: u64 = env
            .storage()
            .persistent()
            .get(&AUTO_REL_DLY)
            .unwrap_or(DEFAULT_AUTO_RELEASE_DELAY);

        let mut count: u64 = env.storage().persistent().get(&ESCROW_COUNT).unwrap_or(0);
        count += 1;
        env.storage().persistent().set(&ESCROW_COUNT, &count);
        env.storage()
            .persistent()
            .extend_ttl(&ESCROW_COUNT, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

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
            usd_amount: 0,
            quoted_token_amount: amount,
            send_asset: token_address.clone(),
            dest_asset: token_address.clone(),
            total_sessions,
            sessions_completed: 0,
        };

        let key = (ESCROW_SYM, count);
        env.storage().persistent().set(&key, &escrow);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        env.storage().persistent().set(&session_dup_key, &true);
        env.storage().persistent().extend_ttl(
            &session_dup_key,
            ESCROW_TTL_THRESHOLD,
            ESCROW_TTL_BUMP,
        );

        let mentor_key = (MENTOR_ESCROWS, mentor.clone());
        let mut mentor_escrows: Vec<u64> = env
            .storage()
            .persistent()
            .get(&mentor_key)
            .unwrap_or(Vec::new(&env));
        mentor_escrows.push_back(count);
        env.storage().persistent().set(&mentor_key, &mentor_escrows);
        env.storage()
            .persistent()
            .extend_ttl(&mentor_key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let learner_key = (LEARNER_ESCROWS, learner.clone());
        let mut learner_escrows: Vec<u64> = env
            .storage()
            .persistent()
            .get(&learner_key)
            .unwrap_or(Vec::new(&env));
        learner_escrows.push_back(count);
        env.storage()
            .persistent()
            .set(&learner_key, &learner_escrows);
        env.storage()
            .persistent()
            .extend_ttl(&learner_key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        env.events().publish(
            (symbol_short!("Escrow"), symbol_short!("Created"), count),
            EscrowCreatedEventData {
                mentor,
                learner,
                amount,
                session_id,
                token_address,
                session_end_time,
            },
        );

        count
    }

    pub fn release_funds(env: Env, caller: Address, escrow_id: u64) {
        let key = (ESCROW_SYM, escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let mut escrow: Escrow = env.storage().persistent().get(&key).expect("Escrow not found");

        if escrow.status != EscrowStatus::Active {
            panic!("Escrow not active");
        }

        let admin: Address = env.storage().persistent().get(&ADMIN).expect("Admin not found");
        env.storage().persistent().extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        caller.require_auth();

        if caller != escrow.learner && caller != admin {
            panic!("Caller not authorized");
        }

        let gross = escrow.amount;
        Self::_do_release(&env, &mut escrow, &key, gross);
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
        let key = (ESCROW_SYM, escrow_id);
        env.storage().persistent().extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let mut escrow: Escrow = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Escrow not found");

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

        env.events().publish((symbol_short!("auto_rel"), escrow_id), (escrow_id, now));

        let gross = escrow.amount;
        Self::_do_release(&env, &mut escrow, &key, gross);
    }

    /// Open a dispute (called by mentor or learner).
    ///
    /// - `reason`: a short symbol describing the dispute (e.g. `symbol_short!("NO_SHOW")`).
    ///   Stored on the escrow for admin review.
    ///
    /// Panics if:
    /// - Escrow does not exist.
    /// - Escrow is not `Active`.
    /// - Caller is neither mentor nor learner.
    pub fn dispute(env: Env, caller: Address, escrow_id: u64, reason: Symbol) {
        let key = (symbol_short!("ESCROW"), escrow_id);
        env.storage().persistent().extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        if escrow.sessions_completed >= escrow.total_sessions {
            panic!("All sessions already released");
        }

        let admin: Address = env
            .storage()
            .persistent()
            .get(&ADMIN)
            .expect("Admin not found");
        env.storage()
            .persistent()
            .extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        caller.require_auth();
        if caller != escrow.learner && caller != admin {
            panic!("Caller not authorized");
        }

        let amount_to_release = if escrow.sessions_completed + 1 == escrow.total_sessions {
            escrow.amount
        } else {
            escrow
                .quoted_token_amount
                .checked_div(escrow.total_sessions as i128)
                .expect("Division error")
        };

        let fee_bps: u32 = env.storage().persistent().get(&FEE_BPS).unwrap_or(0u32);
        env.storage()
            .persistent()
            .extend_ttl(&FEE_BPS, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let platform_fee: i128 = amount_to_release
            .checked_mul(fee_bps as i128)
            .expect("Overflow")
            .checked_div(10_000)
            .expect("Division error");
        let net_amount: i128 = amount_to_release
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

        let token_client = token::Client::new(&env, &escrow.token_address);

        if platform_fee > 0 {
            token_client.transfer(&env.current_contract_address(), &treasury, &platform_fee);
        }

        token_client.transfer(&env.current_contract_address(), &escrow.mentor, &net_amount);

        escrow.sessions_completed += 1;
        escrow.amount = escrow
            .amount
            .checked_sub(amount_to_release)
            .expect("Underflow");
        escrow.platform_fee = escrow
            .platform_fee
            .checked_add(platform_fee)
            .expect("Overflow");
        escrow.net_amount = escrow.net_amount.checked_add(net_amount).expect("Overflow");

        if escrow.sessions_completed == escrow.total_sessions {
            escrow.status = EscrowStatus::Released;
            let session_key = (SESSION_KEY, escrow.session_id.clone());
            env.storage().persistent().remove(&session_key);
        }

        env.storage().persistent().set(&key, &escrow);

        env.events().publish(
            (symbol_short!("partial"), escrow.id),
            (escrow.sessions_completed, amount_to_release),
        );
    }

    pub fn admin_release(env: Env, escrow_id: u64) {
        let key = (ESCROW_SYM, escrow_id);
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

        let admin: Address = env
            .storage()
            .persistent()
            .get(&ADMIN)
            .expect("Admin not found");
        env.storage()
            .persistent()
            .extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        admin.require_auth();

        env.events().publish(
            (symbol_short!("Escrow"), symbol_short!("adm_rel"), escrow_id),
            (escrow_id, env.ledger().timestamp()),
        );

        let gross = escrow.amount;
        Self::_do_release(&env, &mut escrow, &key, gross);
    }

    pub fn try_auto_release(env: Env, escrow_id: u64) {
        let key = (ESCROW_SYM, escrow_id);
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

        let now = env.ledger().timestamp();
        let release_after = escrow
            .session_end_time
            .checked_add(escrow.auto_release_delay)
            .expect("Timestamp overflow");

        if now < release_after {
            panic!("Auto-release window has not elapsed");
        }

        env.events().publish(
            (symbol_short!("Escrow"), symbol_short!("AutoRel"), escrow_id),
            EscrowAutoReleasedEventData { time: now },
        );

        let gross = escrow.amount;
        Self::_do_release(&env, &mut escrow, &key, gross);
    }

    pub fn dispute(env: Env, caller: Address, escrow_id: u64, reason: Symbol) {
        let key = (ESCROW_SYM, escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let mut escrow: Escrow = env.storage().persistent().get(&key).expect("Escrow not found");

        if escrow.status != EscrowStatus::Active {
            panic!("Escrow not active");
        }

        caller.require_auth();

        if caller != escrow.mentor && caller != escrow.learner {
            panic!("Caller not authorized to dispute");
        }

        escrow.status = EscrowStatus::Disputed;
        escrow.dispute_reason = reason.clone();
        env.storage().persistent().set(&key, &escrow);

        env.events().publish(
            (symbol_short!("disp_opnd"), escrow_id),
            (escrow_id, caller, reason, escrow.token_address)
        );
    }

    /// Resolve a disputed escrow by splitting funds between mentor and learner.
    ///
    /// Admin only. Can only be called on `Disputed` escrows.
    ///
    /// - `mentor_pct`: percentage (0–100) of `escrow.amount` sent to the mentor.
    ///   The remainder (`100 - mentor_pct`) goes to the learner. No platform fee
    ///   is deducted — the full escrowed amount is split between the parties.
    ///
    /// Examples:
    /// - `mentor_pct = 100` → full amount to mentor, nothing to learner.
    /// - `mentor_pct = 50`  → half to each party.
    /// - `mentor_pct = 0`   → full amount to learner, nothing to mentor.
    ///
    /// Stores the mentor's share in `escrow.net_amount`, the learner's share
    /// in `escrow.platform_fee` (repurposed as learner_amount for the resolved
    /// state), and records `resolved_at` timestamp.
    ///
    /// Panics if:
    /// - Contract is not initialized.
    /// - Escrow does not exist.
    /// - Escrow status is not `Disputed`.
    /// - `mentor_pct` > 100.
    pub fn resolve_dispute(env: Env, escrow_id: u64, mentor_pct: u32) {
        // --- Admin auth ---
        let admin: Address = env.storage().persistent().get(&ADMIN).expect("Not initialized");
        env.storage().persistent().extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        admin.require_auth();

        // --- Validate split percentage ---
        if mentor_pct > 100 {
            panic!("mentor_pct must be between 0 and 100");
        }

        let key = (ESCROW_SYM, escrow_id);
        env.storage().persistent().extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let mut escrow: Escrow = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Escrow not found");

        if escrow.status != EscrowStatus::Disputed {
            panic!("Escrow is not in Disputed status");
        }

        // --- Calculate split amounts ---
        let mentor_amount: i128 = escrow
            .amount
            .checked_mul(mentor_pct as i128)
            .expect("Overflow")
            .checked_div(100)
            .expect("Division error");
        let learner_amount: i128 = escrow.amount.checked_sub(mentor_amount).expect("Underflow");

        let token_client = token::Client::new(&env, &escrow.token_address);

        if mentor_amount > 0 {
            token_client.transfer(&env.current_contract_address(), &escrow.mentor, &mentor_amount);
        }
        if learner_amount > 0 {
            token_client.transfer(&env.current_contract_address(), &escrow.learner, &learner_amount);
        }

        let now = env.ledger().timestamp();
        escrow.status = EscrowStatus::Resolved;
        escrow.net_amount = mentor_amount;
        escrow.platform_fee = learner_amount;
        escrow.resolved_at = now;
        env.storage().persistent().set(&key, &escrow);

        env.events().publish(
            (symbol_short!("disp_res"), escrow_id),
            (escrow_id, mentor_pct, mentor_amount, learner_amount, escrow.token_address.clone(), now),
        );
    }

    /// Refund tokens to the learner (admin only).
    pub fn refund(env: Env, escrow_id: u64) {
        let admin: Address = env.storage().persistent().get(&ADMIN).expect("Admin not found");
        env.storage().persistent().extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        admin.require_auth();

        let key = (ESCROW_SYM, escrow_id);
        env.storage().persistent().extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let mut escrow: Escrow = env.storage().persistent().get(&key).expect("Escrow not found");

        if matches!(
            escrow.status,
            EscrowStatus::Released | EscrowStatus::Refunded | EscrowStatus::Resolved
        ) {
            panic!("Cannot refund");
        }

        let refund_amt = escrow.amount;
        let token_client = token::Client::new(&env, &escrow.token_address);
        token_client.transfer(&env.current_contract_address(), &escrow.learner, &refund_amt);

        escrow.status = EscrowStatus::Refunded;
        escrow.amount = 0;
        env.storage().persistent().set(&key, &escrow);

        let session_key = (SESSION_KEY, escrow.session_id.clone());
        env.storage().persistent().remove(&session_key);

        env.events().publish(
            (symbol_short!("refunded"), escrow_id),
            (escrow.learner.clone(), escrow.amount, escrow.token_address)
        );
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    pub fn get_escrow(env: Env, escrow_id: u64) -> Escrow {
        let key = (symbol_short!("ESCROW"), escrow_id);
        env.storage().persistent().extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        env.storage().persistent().get(&key).expect("Escrow not found")
    }

    pub fn get_escrow_count(env: Env) -> u64 {
        env.storage().persistent().extend_ttl(&ESCROW_COUNT, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        env.storage().persistent().get(&ESCROW_COUNT).unwrap_or(0)
    }

    pub fn get_fee_bps(env: Env) -> u32 {
        env.storage().persistent().extend_ttl(&FEE_BPS, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        env.storage().persistent().get(&FEE_BPS).unwrap_or(0)
    }

    pub fn get_treasury(env: Env) -> Address {
        env.storage().persistent().extend_ttl(&TREASURY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        env.storage().persistent().get(&TREASURY).expect("Treasury not set")
    }

    pub fn get_auto_release_delay(env: Env) -> u64 {
        env.storage().persistent().extend_ttl(&AUTO_REL_DLY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        env.storage().persistent().get(&AUTO_REL_DLY).unwrap_or(DEFAULT_AUTO_RELEASE_DELAY)
    }

    pub fn submit_review(env: Env, caller: Address, escrow_id: u64, reason: Symbol) {
        let key = (ESCROW_SYM, escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let escrow: Escrow = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Escrow not found");

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

        env.events().publish(
            (symbol_short!("Escrow"), symbol_short!("RevSub"), escrow_id),
            ReviewSubmittedEventData {
                caller,
                reason,
                mentor: escrow.mentor,
            },
        );
    }

    // -----------------------------------------------------------------------
    // Group session escrow
    // -----------------------------------------------------------------------

    pub fn create_group_escrow(
        env: Env,
        mentor: Address,
        max_learners: u32,
        price_per_learner: i128,
        token: Address,
        session_id: Symbol,
    ) -> u64 {
        mentor.require_auth();
        if max_learners < 2 {
            panic!("max_learners must be at least 2");
        }
        if price_per_learner <= 0 {
            panic!("price_per_learner must be positive");
        }
        if !Self::_is_token_approved(&env, &token) {
            panic!("Token not approved");
        }

        let session_dup_key = (SESSION_KEY, session_id.clone());
        if env.storage().persistent().has(&session_dup_key) {
            panic!("Session ID already used");
        }

        let mut count: u64 = env
            .storage()
            .persistent()
            .get(&GROUP_ESCROW_COUNT)
            .unwrap_or(0);
        count += 1;
        env.storage().persistent().set(&GROUP_ESCROW_COUNT, &count);
        env.storage().persistent().extend_ttl(
            &GROUP_ESCROW_COUNT,
            ESCROW_TTL_THRESHOLD,
            ESCROW_TTL_BUMP,
        );

        let group = GroupEscrow {
            id: count,
            mentor: mentor.clone(),
            max_learners,
            price_per_learner,
            token_address: token,
            session_id: session_id.clone(),
            learners: Vec::new(&env),
            status: GroupEscrowStatus::Open,
            created_at: env.ledger().timestamp(),
        };

        let key = (GROUP_ESCROW_SYM, count);
        env.storage().persistent().set(&key, &group);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        env.storage().persistent().set(&session_dup_key, &true);
        env.storage().persistent().extend_ttl(
            &session_dup_key,
            ESCROW_TTL_THRESHOLD,
            ESCROW_TTL_BUMP,
        );

        count
    }

    pub fn join_group_escrow(env: Env, learner: Address, escrow_id: u64) {
        let key = (GROUP_ESCROW_SYM, escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let mut group: GroupEscrow = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Group escrow not found");

        if !matches!(
            group.status,
            GroupEscrowStatus::Open | GroupEscrowStatus::Full
        ) {
            panic!("Group not accepting learners");
        }
        if group.status == GroupEscrowStatus::Full {
            panic!("Group is full");
        }

        learner.require_auth();

        for i in 0..group.learners.len() {
            if group.learners.get(i).unwrap() == learner {
                panic!("Learner already joined");
            }
        }

        let token_client = token::Client::new(&env, &group.token_address);
        if token_client.balance(&learner) < group.price_per_learner {
            panic!("Insufficient token balance");
        }

        token_client.transfer(
            &learner,
            &env.current_contract_address(),
            &group.price_per_learner,
        );

        group.learners.push_back(learner.clone());

        let n = group.learners.len();
        if n >= group.max_learners as u32 {
            group.status = GroupEscrowStatus::Full;
        }

        env.storage().persistent().set(&key, &group);

        env.events().publish(
            (Symbol::new(&env, "learner_joined"), escrow_id),
            LearnerJoinedEventData {
                escrow_id,
                learner,
                learners_count: group.learners.len(),
            },
        );
    }

    pub fn start_group_session(env: Env, escrow_id: u64) {
        let key = (GROUP_ESCROW_SYM, escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let mut group: GroupEscrow = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Group escrow not found");

        group.mentor.require_auth();

        if !matches!(
            group.status,
            GroupEscrowStatus::Open | GroupEscrowStatus::Full
        ) {
            panic!("Invalid status for start");
        }
        if group.learners.len() < 2 {
            panic!("Need at least 2 learners");
        }

        group.status = GroupEscrowStatus::Active;
        env.storage().persistent().set(&key, &group);

        env.events().publish(
            (Symbol::new(&env, "group_started"), escrow_id),
            GroupStartedEventData {
                escrow_id,
                learner_count: group.learners.len(),
            },
        );
    }

    pub fn release_group_funds(env: Env, escrow_id: u64) {
        let key = (GROUP_ESCROW_SYM, escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let mut group: GroupEscrow = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Group escrow not found");

        group.mentor.require_auth();

        if group.status != GroupEscrowStatus::Active {
            panic!("Group session not active");
        }

        let gross = (group.price_per_learner as i128)
            .checked_mul(group.learners.len() as i128)
            .expect("overflow");

        let fee_bps: u32 = env.storage().persistent().get(&FEE_BPS).unwrap_or(0u32);
        env.storage()
            .persistent()
            .extend_ttl(&FEE_BPS, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let platform_fee: i128 = gross
            .checked_mul(fee_bps as i128)
            .expect("Overflow")
            .checked_div(10_000)
            .expect("Division error");
        let net_amount: i128 = escrow.amount.checked_sub(platform_fee).expect("Underflow");
        let net_amount: i128 = gross.checked_sub(platform_fee).expect("Underflow");

        let treasury: Address = env
            .storage()
            .persistent()
            .get(&TREASURY)
            .expect("Treasury not found");
        env.storage().persistent().extend_ttl(&TREASURY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let token_client = token::Client::new(env, &escrow.token_address);
        env.storage()
            .persistent()
            .extend_ttl(&TREASURY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let token_client = token::Client::new(&env, &group.token_address);

        if platform_fee > 0 {
            token_client.transfer(&env.current_contract_address(), &treasury, &platform_fee);
        }
        token_client.transfer(&env.current_contract_address(), &group.mentor, &net_amount);

        group.status = GroupEscrowStatus::Released;
        env.storage().persistent().set(&key, &group);

        escrow.status = EscrowStatus::Released;
        escrow.platform_fee = platform_fee;
        escrow.net_amount = net_amount;
        env.storage().persistent().set(key, escrow);

        env.events().publish(
            (symbol_short!("released"), escrow.id),
            (
                escrow.mentor.clone(),
                escrow.amount,
                net_amount,
                platform_fee,
                escrow.token_address.clone(),
            )
        );
    }

    fn _set_token_approved(env: &Env, token_address: &Address, approved: bool) {
        let key = (APPROVED_TOKEN_KEY, token_address.clone());
        env.storage().persistent().set(&key, &approved);
        env.storage().persistent().extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
    }

    fn _is_token_approved(env: &Env, token_address: &Address) -> bool {
        let key = (APPROVED_TOKEN_KEY, token_address.clone());
        env.storage().persistent().get::<_, bool>(&key).unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test {
    extern crate std;
    use super::*;
    use soroban_sdk::{
        testutils::{ Address as _, Ledger },
        token::{ Client as TokenClient, StellarAssetClient },
        Address,
        Env,
        Vec,
    };
        let session_key = (SESSION_KEY, group.session_id.clone());
        env.storage().persistent().remove(&session_key);

        env.events().publish(
            (Symbol::new(&env, "group_released"), escrow_id),
            GroupReleasedEventData {
                escrow_id,
                gross,
                net_amount,
                platform_fee,
                token_address: group.token_address.clone(),
            },
        );
    }

    pub fn cancel_group_escrow(env: Env, escrow_id: u64) {
        let key = (GROUP_ESCROW_SYM, escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let mut group: GroupEscrow = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Group escrow not found");

        group.mentor.require_auth();

        if group.status == GroupEscrowStatus::Active
            || group.status == GroupEscrowStatus::Released
            || group.status == GroupEscrowStatus::Cancelled
        {
            panic!("Cannot cancel");
        }

        let token_client = token::Client::new(&env, &group.token_address);
        for i in 0..group.learners.len() {
            let learner = group.learners.get(i).unwrap();
            token_client.transfer(
                &env.current_contract_address(),
                &learner,
                &group.price_per_learner,
            );
        }

        group.status = GroupEscrowStatus::Cancelled;
        env.storage().persistent().set(&key, &group);

        let session_key = (SESSION_KEY, group.session_id.clone());
        env.storage().persistent().remove(&session_key);
    }

    pub fn get_group_escrow(env: Env, escrow_id: u64) -> GroupEscrow {
        let key = (GROUP_ESCROW_SYM, escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        env.storage()
            .persistent()
            .get(&key)
            .expect("Group escrow not found")
    }

    // -----------------------------------------------------------------------
    // Milestone escrow
    // -----------------------------------------------------------------------

    fn create_token<'a>(env: &'a Env, admin: &Address) -> (Address, StellarAssetClient<'a>) {
        let token_address = env.register_stellar_asset_contract_v2(admin.clone()).address();
        let sac = StellarAssetClient::new(env, &token_address);
        (token_address, sac)
    }

    struct TestFixture {
    pub fn create_milestone_escrow(
        env: Env,
        mentor: Address,
        learner: Address,
        milestones: Vec<MilestoneSpec>,
        token_address: Address,
    }

    impl TestFixture {
        fn setup() -> Self {
            Self::setup_with_fee(500)
    ) -> u64 {
        if milestones.is_empty() {
            panic!("At least one milestone required");
        }

        fn setup_with_fee(fee_bps: u32) -> Self {
            Self::setup_with_fee_and_delay(fee_bps, 0)
        }

        fn setup_with_fee_and_delay(fee_bps: u32, auto_release_delay_secs: u64) -> Self {
            let env = Env::default();
            env.mock_all_auths();
            // Advance time so timestamp is not 0
            env.ledger().with_mut(|li| {
                li.timestamp = 14400;
            });

            let contract_id = env.register_contract(None, EscrowContract);
            let admin = Address::generate(&env);
            let mentor = Address::generate(&env);
            let learner = Address::generate(&env);
            let treasury = Address::generate(&env);

            let (token_address, token_sac) = create_token(&env, &admin);
            token_sac.mint(&learner, &10_000);

            let client = EscrowContractClient::new(&env, &contract_id);
            let mut approved = Vec::new(&env);
            approved.push_back(token_address.clone());
            client.initialize(&admin, &treasury, &fee_bps, &approved, &auto_release_delay_secs);

            TestFixture {
                env,
                contract_id,
                admin,
                mentor,
                learner,
                treasury,
                token_address,
            }
        }

        fn client(&self) -> EscrowContractClient {
            EscrowContractClient::new(&self.env, &self.contract_id)
        }

        fn token(&self) -> TokenClient {
            TokenClient::new(&self.env, &self.token_address)
        }

        fn sac(&self) -> StellarAssetClient {
            StellarAssetClient::new(&self.env, &self.token_address)
        }

        /// Helper: create an escrow with a given session_end_time.
        fn create_escrow_at(&self, session_end_time: u64) -> u64 {
            self.client().create_escrow(
                &self.mentor,
                &self.learner,
                &1_000,
                &symbol_short!("S1"),
                &self.token_address,
                &session_end_time
            )
        }

        /// Helper: open a dispute on an existing escrow.
        fn open_dispute(&self, escrow_id: u64) {
            self.client().dispute(&self.learner, &escrow_id, &symbol_short!("NO_SHOW"));
        }
    }

    // -----------------------------------------------------------------------
    // Initialization
    // -----------------------------------------------------------------------

    #[test]
    fn test_initialize_and_prevent_reinit() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let approved: Vec<Address> = Vec::new(&env);

        client.initialize(&admin, &treasury, &500u32, &approved, &0u64);

        let result = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                let other = Address::generate(&env);
                client.initialize(&other, &treasury, &500u32, &approved, &0u64);
            })
        );
        assert!(result.is_err(), "Re-initialization should panic");
    }

    #[test]
    fn test_default_auto_release_delay() {
        let f = TestFixture::setup(); // passes 0 → should store 72 h
        assert_eq!(f.client().get_auto_release_delay(), 72 * 60 * 60);
    }

    #[test]
    fn test_custom_auto_release_delay_stored() {
        let f = TestFixture::setup_with_fee_and_delay(500, 3_600); // 1 hour
        assert_eq!(f.client().get_auto_release_delay(), 3_600);
    }

    // -----------------------------------------------------------------------
    // Token allowlist
    // -----------------------------------------------------------------------

    #[test]
    fn test_unapproved_token_rejected() {
        let f = TestFixture::setup();
        let unapproved = Address::generate(&f.env);
        let result = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                f.client().create_escrow(
                    &f.mentor,
                    &f.learner,
                    &500,
                    &symbol_short!("S1"),
                    &unapproved,
                    &0u64
                );
            })
        );
        assert!(result.is_err(), "Unapproved token should be rejected");
    }

    #[test]
    fn test_approved_token_accepted() {
        let f = TestFixture::setup();
        let id = f.create_escrow_at(0);
        assert_eq!(id, 1);
    }

    #[test]
    fn test_set_approved_token_by_admin() {
        let f = TestFixture::setup();
        let client = f.client();
        let new_token = Address::generate(&f.env);
        assert!(!client.is_token_approved(&new_token));
        client.set_approved_token(&new_token, &true);
        assert!(client.is_token_approved(&new_token));
        client.set_approved_token(&new_token, &false);
        assert!(!client.is_token_approved(&new_token));
    }

    // -----------------------------------------------------------------------
    // Balance check
    // -----------------------------------------------------------------------

    #[test]
    fn test_insufficient_balance_rejected() {
        let f = TestFixture::setup();
        let result = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                f.client().create_escrow(
                    &f.mentor,
                    &f.learner,
                    &999_999,
                    &symbol_short!("S1"),
                    &f.token_address,
                    &0u64
                );
            })
        );
        assert!(result.is_err(), "Insufficient balance should panic");
    }

    // -----------------------------------------------------------------------
    // Amount validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_zero_amount_rejected() {
        let f = TestFixture::setup();
        let result = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                f.client().create_escrow(
                    &f.mentor,
                    &f.learner,
                    &0,
                    &symbol_short!("S1"),
                    &f.token_address,
                    &0u64
                );
            })
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_negative_amount_rejected() {
        let f = TestFixture::setup();
        let result = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                f.client().create_escrow(
                    &f.mentor,
                    &f.learner,
                    &-1,
                    &symbol_short!("S1"),
                    &f.token_address,
                    &0u64
                );
            })
        );
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Counter persistence
    // -----------------------------------------------------------------------

    #[test]
    fn test_escrow_counter_increments_correctly() {
        let f = TestFixture::setup();
        let client = f.client();
        assert_eq!(client.get_escrow_count(), 0);
        let id1 = f.create_escrow_at(0);
        assert_eq!(id1, 1);
        assert_eq!(client.get_escrow_count(), 1);
        let id2 = f.create_escrow_at(0);
        assert_eq!(id2, 2);
        assert_eq!(client.get_escrow_count(), 2);
    }

    // -----------------------------------------------------------------------
    // Token transfer — create_escrow
    // -----------------------------------------------------------------------

    #[test]
    fn test_tokens_held_by_contract_after_create() {
        let f = TestFixture::setup();
        let token = f.token();
        let before = token.balance(&f.learner);
        f.create_escrow_at(0);
        assert_eq!(token.balance(&f.learner), before - 1_000);
        assert_eq!(token.balance(&f.contract_id), 1_000);
    }

    // -----------------------------------------------------------------------
    // Token transfer — release_funds
    // -----------------------------------------------------------------------

    #[test]
    fn test_release_funds_by_learner() {
        let f = TestFixture::setup();
        let client = f.client();
        let token = f.token();
        let id = f.create_escrow_at(0);
        let mentor_before = token.balance(&f.mentor);
        let treasury_before = token.balance(&f.treasury);
        client.release_funds(&f.learner, &id);
        assert_eq!(token.balance(&f.mentor), mentor_before + 950);
        assert_eq!(token.balance(&f.treasury), treasury_before + 50);
        assert_eq!(token.balance(&f.contract_id), 0);
        let escrow = client.get_escrow(&id);
        assert_eq!(escrow.status, EscrowStatus::Released);
        assert_eq!(escrow.platform_fee, 50);
        assert_eq!(escrow.net_amount, 950);
    }

    #[test]
    fn test_release_funds_by_admin() {
        let f = TestFixture::setup();
        let client = f.client();
        let id = client.create_escrow(
            &f.mentor,
            &f.learner,
            &500,
            &symbol_short!("S1"),
            &f.token_address,
            &0u64
        );
        client.release_funds(&f.admin, &id);
        assert_eq!(client.get_escrow(&id).status, EscrowStatus::Released);
    }

    #[test]
    fn test_release_funds_unauthorized() {
        let f = TestFixture::setup();
        let rando = Address::generate(&f.env);
        let id = f.create_escrow_at(0);
        let result = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                f.client().release_funds(&rando, &id);
            })
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_cannot_release_twice() {
        let f = TestFixture::setup();
        let client = f.client();
        let id = f.create_escrow_at(0);
        client.release_funds(&f.learner, &id);
        let result = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                client.release_funds(&f.learner, &id);
            })
        );
        assert!(result.is_err(), "Double-release should panic");
    }

    // -----------------------------------------------------------------------
    // Token transfer — refund
    // -----------------------------------------------------------------------

    #[test]
    fn test_refund_by_admin() {
        let f = TestFixture::setup();
        let client = f.client();
        let token = f.token();
        let id = f.create_escrow_at(0);
        let learner_before = token.balance(&f.learner);
        client.refund(&id);
        assert_eq!(token.balance(&f.learner), learner_before + 1_000);
        assert_eq!(token.balance(&f.contract_id), 0);
        assert_eq!(client.get_escrow(&id).status, EscrowStatus::Refunded);
    }

    #[test]
    fn test_refund_after_dispute() {
        let f = TestFixture::setup();
        let client = f.client();
        let id = f.create_escrow_at(0);
        client.dispute(&f.mentor, &id, &symbol_short!("LATE"));
        client.refund(&id);
        assert_eq!(client.get_escrow(&id).status, EscrowStatus::Refunded);
    }

    #[test]
    fn test_cannot_refund_released() {
        let f = TestFixture::setup();
        let client = f.client();
        let id = f.create_escrow_at(0);
        client.release_funds(&f.learner, &id);
        let result = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                client.refund(&id);
            })
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_cannot_refund_resolved() {
        let f = TestFixture::setup_with_fee(0);
        let client = f.client();
        let id = f.create_escrow_at(0);
        f.open_dispute(id);
        client.resolve_dispute(&id, &50u32);
        let result = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                client.refund(&id);
            })
        );
        assert!(result.is_err(), "Cannot refund a resolved escrow");
    }

    // -----------------------------------------------------------------------
    // Dispute — updated tests (reason parameter)
    // -----------------------------------------------------------------------

    #[test]
    fn test_dispute_by_mentor_stores_reason() {
        let f = TestFixture::setup();
        let client = f.client();
        let id = f.create_escrow_at(0);
        let reason = symbol_short!("NO_SHOW");
        client.dispute(&f.mentor, &id, &reason);
        let escrow = client.get_escrow(&id);
        assert_eq!(escrow.status, EscrowStatus::Disputed);
        assert_eq!(escrow.dispute_reason, reason);
    }

    #[test]
    fn test_dispute_by_learner_stores_reason() {
        let f = TestFixture::setup();
        let client = f.client();
        let id = f.create_escrow_at(0);
        let reason = symbol_short!("BAD_SVC");
        client.dispute(&f.learner, &id, &reason);
        let escrow = client.get_escrow(&id);
        assert_eq!(escrow.status, EscrowStatus::Disputed);
        assert_eq!(escrow.dispute_reason, reason);
    }

    #[test]
    fn test_dispute_by_unauthorized_rejected() {
        let f = TestFixture::setup();
        let rando = Address::generate(&f.env);
        let id = f.create_escrow_at(0);
        let result = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                f.client().dispute(&rando, &id, &symbol_short!("FRAUD"));
            })
        );
        assert!(result.is_err(), "Unauthorized dispute should panic");
    }

    #[test]
    fn test_cannot_dispute_non_active_escrow() {
        let f = TestFixture::setup();
        let client = f.client();
        let id = f.create_escrow_at(0);
        client.release_funds(&f.learner, &id);
        let result = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                client.dispute(&f.mentor, &id, &symbol_short!("LATE"));
            })
        );
        assert!(result.is_err(), "Dispute on released escrow should panic");
    }

    // -----------------------------------------------------------------------
    // resolve_dispute — core acceptance criteria
    // -----------------------------------------------------------------------

    /// Helper: create escrow, open dispute, return id.
    fn setup_disputed(f: &TestFixture) -> u64 {
        let id = f.create_escrow_at(0);
        f.open_dispute(id);
        id
    }

    #[test]
    fn test_resolve_dispute_100_0_all_to_mentor() {
        let f = TestFixture::setup_with_fee(0); // fee=0 so math is clean
        let client = f.client();
        let token = f.token();
        let id = setup_disputed(&f);

        let mentor_before = token.balance(&f.mentor);
        let learner_before = token.balance(&f.learner);

        client.resolve_dispute(&id, &100u32);

        assert_eq!(token.balance(&f.mentor), mentor_before + 1_000);
        assert_eq!(token.balance(&f.learner), learner_before); // learner gets nothing
        assert_eq!(token.balance(&f.contract_id), 0);

        let escrow = client.get_escrow(&id);
        assert_eq!(escrow.status, EscrowStatus::Resolved);
        assert_eq!(escrow.net_amount, 1_000); // mentor share
        assert_eq!(escrow.platform_fee, 0); // learner share
        assert!(escrow.resolved_at > 0);
    }

    #[test]
    fn test_resolve_dispute_50_50_equal_split() {
        let f = TestFixture::setup_with_fee(0);
        let client = f.client();
        let token = f.token();
        let id = setup_disputed(&f);

        let mentor_before = token.balance(&f.mentor);
        let learner_before = token.balance(&f.learner);

        client.resolve_dispute(&id, &50u32);

        assert_eq!(token.balance(&f.mentor), mentor_before + 500);
        assert_eq!(token.balance(&f.learner), learner_before + 500);
        assert_eq!(token.balance(&f.contract_id), 0);

        let escrow = client.get_escrow(&id);
        assert_eq!(escrow.status, EscrowStatus::Resolved);
        assert_eq!(escrow.net_amount, 500); // mentor share
        assert_eq!(escrow.platform_fee, 500); // learner share
        assert!(escrow.resolved_at > 0);
    }

    #[test]
    fn test_resolve_dispute_0_100_all_to_learner() {
        let f = TestFixture::setup_with_fee(0);
        let client = f.client();
        let token = f.token();
        let id = setup_disputed(&f);

        let mentor_before = token.balance(&f.mentor);
        let learner_before = token.balance(&f.learner);

        client.resolve_dispute(&id, &0u32);

        assert_eq!(token.balance(&f.mentor), mentor_before); // mentor gets nothing
        assert_eq!(token.balance(&f.learner), learner_before + 1_000);
        assert_eq!(token.balance(&f.contract_id), 0);

        let escrow = client.get_escrow(&id);
        assert_eq!(escrow.status, EscrowStatus::Resolved);
        assert_eq!(escrow.net_amount, 0); // mentor share
        assert_eq!(escrow.platform_fee, 1_000); // learner share
        assert!(escrow.resolved_at > 0);
    }

    #[test]
    fn test_resolve_dispute_rejects_invalid_pct() {
        let f = TestFixture::setup_with_fee(0);
        let id = setup_disputed(&f);
        let result = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                f.client().resolve_dispute(&id, &101u32);
            })
        );
        assert!(result.is_err(), "mentor_pct > 100 should panic");
    }

    #[test]
    fn test_resolve_dispute_only_works_on_disputed() {
        let f = TestFixture::setup_with_fee(0);
        let client = f.client();
        let id = f.create_escrow_at(0);
        // escrow is Active, not Disputed
        let result = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                client.resolve_dispute(&id, &50u32);
            })
        );
        assert!(result.is_err(), "resolve_dispute on Active escrow should panic");
    }

    #[test]
    fn test_resolve_dispute_cannot_resolve_twice() {
        let f = TestFixture::setup_with_fee(0);
        let client = f.client();
        let id = setup_disputed(&f);
        client.resolve_dispute(&id, &50u32);
        let result = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                client.resolve_dispute(&id, &50u32);
            })
        );
        assert!(result.is_err(), "Double-resolve should panic");
    }

    #[test]
    fn test_resolve_dispute_rounding_preserves_full_amount() {
        // 1_000 tokens, 33% mentor → mentor gets 330, learner gets 670.
        // No dust is lost: 330 + 670 == 1_000.
        let f = TestFixture::setup_with_fee(0);
        let client = f.client();
        let token = f.token();
        let id = setup_disputed(&f);

        let mentor_before = token.balance(&f.mentor);
        let learner_before = token.balance(&f.learner);

        client.resolve_dispute(&id, &33u32);

        let mentor_received = token.balance(&f.mentor) - mentor_before;
        let learner_received = token.balance(&f.learner) - learner_before;

        assert_eq!(mentor_received, 330);
        assert_eq!(learner_received, 670);
        assert_eq!(mentor_received + learner_received, 1_000); // no dust lost
        assert_eq!(token.balance(&f.contract_id), 0);
    }

    #[test]
    fn test_resolve_dispute_resolved_at_timestamp_set() {
        let f = TestFixture::setup_with_fee(0);
        let client = f.client();
        let id = setup_disputed(&f);
        let now = f.env.ledger().timestamp();
        client.resolve_dispute(&id, &50u32);
        let escrow = client.get_escrow(&id);
        assert_eq!(escrow.resolved_at, now);
    }

    #[test]
    fn test_resolve_dispute_dispute_reason_preserved() {
        let f = TestFixture::setup_with_fee(0);
        let client = f.client();
        let id = f.create_escrow_at(0);
        let reason = symbol_short!("PARTIAL");
        client.dispute(&f.learner, &id, &reason);
        client.resolve_dispute(&id, &75u32);
        assert_eq!(client.get_escrow(&id).dispute_reason, reason);
    }

    // -----------------------------------------------------------------------
    // Platform fee
    // -----------------------------------------------------------------------

    #[test]
    fn test_fee_zero_percent() {
        let f = TestFixture::setup_with_fee(0);
        let client = f.client();
        let token = f.token();
        let id = f.create_escrow_at(0);
        let mentor_before = token.balance(&f.mentor);
        let treasury_before = token.balance(&f.treasury);
        client.release_funds(&f.learner, &id);
        assert_eq!(token.balance(&f.mentor), mentor_before + 1_000);
        assert_eq!(token.balance(&f.treasury), treasury_before);
        assert_eq!(token.balance(&f.contract_id), 0);
        let escrow = client.get_escrow(&id);
        assert_eq!(escrow.platform_fee, 0);
        assert_eq!(escrow.net_amount, 1_000);
    }

    #[test]
    fn test_fee_five_percent() {
        let f = TestFixture::setup_with_fee(500);
        let client = f.client();
        let token = f.token();
        let id = f.create_escrow_at(0);
        client.release_funds(&f.learner, &id);
        let escrow = client.get_escrow(&id);
        assert_eq!(escrow.platform_fee, 50);
        assert_eq!(escrow.net_amount, 950);
        assert_eq!(token.balance(&f.treasury), 50);
        assert_eq!(token.balance(&f.mentor), 950);
    }

    #[test]
    fn test_fee_ten_percent() {
        let f = TestFixture::setup_with_fee(1_000);
        let client = f.client();
        let token = f.token();
        let id = client.create_escrow(
            &f.mentor,
            &f.learner,
            &2_000,
            &symbol_short!("S1"),
            &f.token_address,
            &0u64
        );
        client.release_funds(&f.learner, &id);
        let escrow = client.get_escrow(&id);
        assert_eq!(escrow.platform_fee, 200);
        assert_eq!(escrow.net_amount, 1_800);
        assert_eq!(token.balance(&f.treasury), 200);
        assert_eq!(token.balance(&f.mentor), 1_800);
    }

    #[test]
    fn test_fee_rounding_truncates_toward_zero() {
        let f = TestFixture::setup_with_fee(500);
        let client = f.client();
        let id = client.create_escrow(
            &f.mentor,
            &f.learner,
            &1,
            &symbol_short!("S1"),
            &f.token_address,
            &0u64
        );
        client.release_funds(&f.learner, &id);
        let escrow = client.get_escrow(&id);
        assert_eq!(escrow.platform_fee, 0);
        assert_eq!(escrow.net_amount, 1);
    }

    // -----------------------------------------------------------------------
    // update_fee
    // -----------------------------------------------------------------------

    #[test]
    fn test_update_fee_by_admin() {
        let f = TestFixture::setup();
        let client = f.client();
        assert_eq!(client.get_fee_bps(), 500);
        client.update_fee(&200u32);
        assert_eq!(client.get_fee_bps(), 200);
    }

    #[test]
    fn test_update_fee_exceeds_cap_rejected() {
        let f = TestFixture::setup();
        let result = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                f.client().update_fee(&1_001u32);
            })
        );
        assert!(result.is_err(), "Fee over 1000 bps should panic");
    }

    #[test]
    fn test_update_fee_at_max_allowed() {
        let f = TestFixture::setup();
        let client = f.client();
        client.update_fee(&1_000u32);
        assert_eq!(client.get_fee_bps(), 1_000);
    }

    #[test]
    fn test_initialize_fee_over_cap_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let approved: Vec<Address> = Vec::new(&env);
        let result = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                client.initialize(&admin, &treasury, &1_001u32, &approved, &0u64);
            })
        );
        assert!(result.is_err(), "initialize with fee > 1000 bps should panic");
    }

    // -----------------------------------------------------------------------
    // update_treasury
    // -----------------------------------------------------------------------

    #[test]
    fn test_update_treasury_by_admin() {
        let f = TestFixture::setup();
        let client = f.client();
        let new_treasury = Address::generate(&f.env);
        client.update_treasury(&new_treasury);
        assert_eq!(client.get_treasury(), new_treasury);
    }

    #[test]
    fn test_fee_goes_to_updated_treasury() {
        let f = TestFixture::setup_with_fee(500);
        let client = f.client();
        let token = f.token();
        let new_treasury = Address::generate(&f.env);
        client.update_treasury(&new_treasury);
        let id = f.create_escrow_at(0);
        client.release_funds(&f.learner, &id);
        assert_eq!(token.balance(&new_treasury), 50);
        assert_eq!(token.balance(&f.treasury), 0);
    }

    // -----------------------------------------------------------------------
    // Auto-release
    // -----------------------------------------------------------------------

    /// Advance ledger timestamp by `secs` seconds.
    fn advance_time(env: &Env, secs: u64) {
        env.ledger().with_mut(|li| {
            li.timestamp += secs;
        });
    }

    #[test]
    fn test_auto_release_fields_stored_on_escrow() {
        // 1-hour delay configured at init; session ends 100 s from now.
        let f = TestFixture::setup_with_fee_and_delay(500, 3_600);
        let now = f.env.ledger().timestamp();
        let session_end = now + 200;
        let id = f.create_escrow_at(session_end);
        let escrow = f.client().get_escrow(&id);
        assert_eq!(escrow.session_end_time, session_end);
        assert_eq!(escrow.auto_release_delay, 3_600);
    }

    #[test]
    fn test_auto_release_triggers_after_delay() {
        // 1-hour delay; session ended in the past.
        let f = TestFixture::setup_with_fee_and_delay(500, 3_600);
        let token = f.token();
        let now = f.env.ledger().timestamp();

        let session_end: u64 = now.checked_sub(200).expect("Underflow");
        let id = f.create_escrow_at(session_end);

        // Wind clock past session_end + delay (1 h = 3 600 s).
        advance_time(&f.env, 3_600 + 1);

        let mentor_before = token.balance(&f.mentor);
        let treasury_before = token.balance(&f.treasury);

        f.client().try_auto_release(&id);

        // 5% fee on 1_000 → 50 fee, 950 net
        assert_eq!(token.balance(&f.mentor), mentor_before + 950);
        assert_eq!(token.balance(&f.treasury), treasury_before + 50);
        assert_eq!(token.balance(&f.contract_id), 0);

        let escrow = f.client().get_escrow(&id);
        assert_eq!(escrow.status, EscrowStatus::Released);
        assert_eq!(escrow.platform_fee, 50);
        assert_eq!(escrow.net_amount, 950);
    }

    #[test]
    fn test_auto_release_triggers_exactly_at_boundary() {
        let f = TestFixture::setup_with_fee_and_delay(0, 3_600);
        let now = f.env.ledger().timestamp();
        let session_end: u64 = now.checked_sub(200).expect("Underflow");
        let id = f.create_escrow_at(session_end);

        // Advance to exactly session_end + delay (boundary is inclusive).
        // session_end = now - 200.
        // target = session_end + 3600 = now - 200 + 3600 = now + 3400.
        advance_time(&f.env, 3_600 - 200);

        f.client().try_auto_release(&id); // must succeed
        assert_eq!(f.client().get_escrow(&id).status, EscrowStatus::Released);
        let total_amount = milestones.iter().fold(0i128, |acc, m| {
            acc.checked_add(m.amount).expect("Amount overflow")
        });

        if total_amount <= 0 {
            panic!("Total amount must be greater than zero");
        }

        learner.require_auth();

        let token_client = token::Client::new(&env, &token_address);
        if token_client.balance(&learner) < total_amount {
            panic!("Insufficient token balance");
        }

        let mut count: u64 = env
            .storage()
            .persistent()
            .get(&MILESTONE_ESCROW_COUNT)
            .unwrap_or(0);
        count += 1;
        env.storage()
            .persistent()
            .set(&MILESTONE_ESCROW_COUNT, &count);
        env.storage().persistent().extend_ttl(
            &MILESTONE_ESCROW_COUNT,
            ESCROW_TTL_THRESHOLD,
            ESCROW_TTL_BUMP,
        );

        token_client.transfer(&learner, &env.current_contract_address(), &total_amount);

        let mut milestone_statuses: Vec<MilestoneStatus> = Vec::new(&env);
        let n = milestones.len();
        for i in 0..n {
            let _ = i;
            milestone_statuses.push_back(MilestoneStatus::Pending);
        }

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

        let key = (MESCROW_SYM, count);
        env.storage().persistent().set(&key, &milestone_escrow);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        env.events().publish(
            (symbol_short!("ms_crt"), count),
            (mentor, learner, total_amount, milestones.len()),
        );

        count
    }

    pub fn complete_milestone(env: Env, escrow_id: u64, milestone_index: u32) {
        let key = (MESCROW_SYM, escrow_id);
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

        if milestone_index as u32 >= milestone_escrow.milestones.len() as u32 {
            panic!("Invalid milestone index");
        }

        let current = milestone_escrow
            .milestone_statuses
            .get(milestone_index as u32)
            .unwrap();
        if current != MilestoneStatus::Pending {
            panic!("Milestone not pending");
        }

        milestone_escrow.learner.require_auth();

        let milestone = milestone_escrow
            .milestones
            .get(milestone_index as u32)
            .unwrap();

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
        let net_amount: i128 = milestone
            .amount
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

        let token_client = token::Client::new(&env, &milestone_escrow.token_address);

        if platform_fee > 0 {
            token_client.transfer(&env.current_contract_address(), &treasury, &platform_fee);
        }

        token_client.transfer(
            &env.current_contract_address(),
            &milestone_escrow.mentor,
            &net_amount,
        );

        let mut new_statuses: Vec<MilestoneStatus> = Vec::new(&env);
        for i in 0..milestone_escrow.milestone_statuses.len() {
            let st = milestone_escrow.milestone_statuses.get(i).unwrap();
            if i == milestone_index as u32 {
                new_statuses.push_back(MilestoneStatus::Completed);
            } else {
                new_statuses.push_back(st);
            }
        }
        milestone_escrow.milestone_statuses = new_statuses;

        milestone_escrow.platform_fee = milestone_escrow
            .platform_fee
            .checked_add(platform_fee)
            .expect("Overflow");
        milestone_escrow.net_amount = milestone_escrow
            .net_amount
            .checked_add(net_amount)
            .expect("Overflow");

        let all_done = (0..milestone_escrow.milestone_statuses.len()).all(|i| {
            milestone_escrow.milestone_statuses.get(i).unwrap() == MilestoneStatus::Completed
        });
        if all_done {
            milestone_escrow.status = EscrowStatus::Released;
        }

        env.storage().persistent().set(&key, &milestone_escrow);

        env.events().publish(
            (symbol_short!("ms_cmp"), escrow_id),
            (milestone_index, milestone.amount, net_amount),
        );
    }

    pub fn dispute_milestone(env: Env, escrow_id: u64, milestone_index: u32, reason: Symbol) {
        let key = (MESCROW_SYM, escrow_id);
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

        if milestone_index as u32 >= milestone_escrow.milestones.len() as u32 {
            panic!("Invalid milestone index");
        }

        let current = milestone_escrow
            .milestone_statuses
            .get(milestone_index as u32)
            .unwrap();
        if current != MilestoneStatus::Pending {
            panic!("Milestone not pending");
        }

        milestone_escrow.mentor.require_auth();
        milestone_escrow.learner.require_auth();

        let mut new_statuses: Vec<MilestoneStatus> = Vec::new(&env);
        for i in 0..milestone_escrow.milestone_statuses.len() {
            let st = milestone_escrow.milestone_statuses.get(i).unwrap();
            if i == milestone_index as u32 {
                new_statuses.push_back(MilestoneStatus::Disputed);
            } else {
                new_statuses.push_back(st);
            }
        }
        milestone_escrow.milestone_statuses = new_statuses;
        milestone_escrow.status = EscrowStatus::Disputed;

        env.storage().persistent().set(&key, &milestone_escrow);

        env.events().publish(
            (symbol_short!("ms_dis"), escrow_id),
            (milestone_index, reason),
        );
    }

    pub fn get_milestone_escrow(env: Env, escrow_id: u64) -> MilestoneEscrow {
        let key = (MESCROW_SYM, escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        env.storage()
            .persistent()
            .get(&key)
            .expect("Milestone escrow not found")
    }

    pub fn get_milestone_escrow_count(env: Env) -> u64 {
        env.storage().persistent().extend_ttl(
            &MILESTONE_ESCROW_COUNT,
            ESCROW_TTL_THRESHOLD,
            ESCROW_TTL_BUMP,
        );
        env.storage()
            .persistent()
            .get(&MILESTONE_ESCROW_COUNT)
            .unwrap_or(0)
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    pub fn get_escrow(env: Env, escrow_id: u64) -> Escrow {
        let key = (ESCROW_SYM, escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        env.storage()
            .persistent()
            .get(&key)
            .expect("Escrow not found")
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

    pub fn get_escrows_by_mentor(
        env: Env,
        mentor: Address,
        page: u32,
        page_size: u32,
    ) -> Vec<Escrow> {
        let page_size = if page_size > 50 { 50 } else { page_size };
        let mentor_key = (MENTOR_ESCROWS, mentor);
        let mentor_escrows: Vec<u64> = env
            .storage()
            .persistent()
            .get(&mentor_key)
            .unwrap_or(Vec::new(&env));
        let start = page.checked_mul(page_size).unwrap_or(0);
        let mut result = Vec::new(&env);

        if start >= mentor_escrows.len() {
            return result;
        }

        let end = (start + page_size).min(mentor_escrows.len());
        for i in start..end {
            let id = mentor_escrows.get(i).unwrap();
            let key = (ESCROW_SYM, id);
            if let Some(escrow) = env.storage().persistent().get::<_, Escrow>(&key) {
                result.push_back(escrow);
            }
        }
        result
    }

    pub fn get_escrows_by_learner(
        env: Env,
        learner: Address,
        page: u32,
        page_size: u32,
    ) -> Vec<Escrow> {
        let page_size = if page_size > 50 { 50 } else { page_size };
        let learner_key = (LEARNER_ESCROWS, learner);
        let learner_escrows: Vec<u64> = env
            .storage()
            .persistent()
            .get(&learner_key)
            .unwrap_or(Vec::new(&env));
        let start = page.checked_mul(page_size).unwrap_or(0);
        let mut result = Vec::new(&env);

        if start >= learner_escrows.len() {
            return result;
        }

        let end = (start + page_size).min(learner_escrows.len());
        for i in start..end {
            let id = learner_escrows.get(i).unwrap();
            let key = (ESCROW_SYM, id);
            if let Some(escrow) = env.storage().persistent().get::<_, Escrow>(&key) {
                result.push_back(escrow);
            }
        }
        result
    }

    pub fn get_escrows_by_status(env: Env, status: EscrowStatus) -> Vec<u64> {
        let count = env
            .storage()
            .persistent()
            .get(&ESCROW_COUNT)
            .unwrap_or(0u64);
        let mut result = Vec::new(&env);

        for i in 1..=count {
            let key = (ESCROW_SYM, i);
            if let Some(escrow) = env.storage().persistent().get::<_, Escrow>(&key) {
                if escrow.status == status {
                    result.push_back(i);
                }
            }
        }
        result
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    fn _do_release(env: &Env, escrow: &mut Escrow, key: &(Symbol, u64), gross: i128) {
        let fee_bps: u32 = env.storage().persistent().get(&FEE_BPS).unwrap_or(0u32);
        env.storage()
            .persistent()
            .extend_ttl(&FEE_BPS, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let platform_fee: i128 = gross
            .checked_mul(fee_bps as i128)
            .expect("Overflow")
            .checked_div(10_000)
            .expect("Division error");
        let net_amount: i128 = gross.checked_sub(platform_fee).expect("Underflow");

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
        escrow.platform_fee = escrow
            .platform_fee
            .checked_add(platform_fee)
            .expect("Overflow");
        escrow.net_amount = escrow.net_amount.checked_add(net_amount).expect("Overflow");
        escrow.amount = 0;

        env.storage().persistent().set(key, escrow);

        let session_key = (SESSION_KEY, escrow.session_id.clone());
        env.storage().persistent().remove(&session_key);

        env.events().publish(
            (
                symbol_short!("Escrow"),
                symbol_short!("Released"),
                escrow.id,
            ),
            EscrowReleasedEventData {
                mentor: escrow.mentor.clone(),
                amount: gross,
                net_amount,
                platform_fee,
                token_address: escrow.token_address.clone(),
            },
        );
    }

    fn _set_token_approved(env: &Env, token_address: &Address, approved: bool) {
        let key = (symbol_short!("APRV_TOK"), token_address.clone());
        env.storage().persistent().set(&key, &approved);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
    }

    fn _is_token_approved(env: &Env, token_address: &Address) -> bool {
        let key = (symbol_short!("APRV_TOK"), token_address.clone());
        env.storage()
            .persistent()
            .get::<_, bool>(&key)
            .unwrap_or(false)
    }
}

    #[test]
    fn test_auto_release_rejected_before_delay() {
        let f = TestFixture::setup_with_fee_and_delay(500, 3_600);
        let now = f.env.ledger().timestamp();
        let session_end: u64 = now.checked_add(100).expect("Overflow");
        let id = f.create_escrow_at(session_end);

        // Advance to one second before the window opens.
        // session_end + 3600 - 1 = now + 100 + 3600 - 1 = now + 3699.
        advance_time(&f.env, 100 + 3_600 - 1);

        let result = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                f.client().try_auto_release(&id);
            })
        );
        assert!(result.is_err(), "Early auto-release call should panic");
    }

    #[test]
    fn test_auto_release_permissionless_any_caller_can_trigger() {
        // try_auto_release requires no auth — anyone can call it.
        let f = TestFixture::setup_with_fee_and_delay(0, 3_600);
        let now = f.env.ledger().timestamp();
        let session_end: u64 = now;
        let id = f.create_escrow_at(session_end);
        advance_time(&f.env, 3_600 + 1);

        // Call with a completely unrelated address (no mock_all_auths needed
        // for the caller itself since try_auto_release does not require_auth).
        f.client().try_auto_release(&id);
        assert_eq!(f.client().get_escrow(&id).status, EscrowStatus::Released);
    }

    #[test]
    fn test_auto_release_fails_if_already_released() {
        let f = TestFixture::setup_with_fee_and_delay(500, 3_600);
        let now = f.env.ledger().timestamp();
        let session_end: u64 = now;
        let id = f.create_escrow_at(session_end);

        // Manual release first.
        f.client().release_funds(&f.learner, &id);

        advance_time(&f.env, 3_600 + 1);

        let result = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                f.client().try_auto_release(&id);
            })
        );
        assert!(result.is_err(), "Auto-release on already-released escrow should panic");
    }

    #[test]
    fn test_auto_release_fails_if_disputed() {
        // A disputed escrow should NOT auto-release — dispute blocks the timer.
        let f = TestFixture::setup_with_fee_and_delay(500, 3_600);
        let now = f.env.ledger().timestamp();
        let session_end: u64 = now;
        let id = f.create_escrow_at(session_end);

        f.client().dispute(&f.learner, &id, &symbol_short!("LATE"));

        advance_time(&f.env, 3_600 + 1);

        let result = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                f.client().try_auto_release(&id);
            })
        );
        assert!(result.is_err(), "Auto-release on disputed escrow should panic");
    }

    #[test]
    fn test_auto_release_default_72h_delay() {
        // Passing 0 at init should store 72 hours; verify auto-release
        // triggers after exactly 72 h.
        let f = TestFixture::setup_with_fee_and_delay(0, 0); // 0 → default 72 h
        let now = f.env.ledger().timestamp();
        let session_end: u64 = now;
        let id = f.create_escrow_at(session_end);

        let delay_72h: u64 = 72 * 60 * 60;

        // One second before window.
        advance_time(&f.env, delay_72h - 1);
        let too_early = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                f.client().try_auto_release(&id);
            })
        );
        assert!(too_early.is_err());

        // Advance the remaining second.
        advance_time(&f.env, 1);
        f.client().try_auto_release(&id);
        assert_eq!(f.client().get_escrow(&id).status, EscrowStatus::Released);
    }

    #[test]
    fn test_amount_max_i128_overflow_protection() {
        let f = TestFixture::setup_with_fee(500); // 5% fee
        // Use a large amount that doesn't overflow i128 itself but
        // amount * fee_bps will overflow.
        // i128::MAX is ~1.7e38. fee_bps is 500.
        // amount = i128::MAX / 100 is ~1.7e36.
        // amount * 500 = 8.5e38 > i128::MAX.
        let amount = i128::MAX / 100;

        // Mint to learner
        f.sac().mint(&f.learner, &amount);

        // Create escrow with max i128
        let id = f
            .client()
            .create_escrow(
                &f.mentor,
                &f.learner,
                &amount,
                &symbol_short!("MAX"),
                &f.token_address,
                &0u64
            );

        // Releasing should panic due to overflow in platform fee calculation
        // (amount * 500 / 10000) -> i128::MAX * 500 will overflow before division
        let result = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                f.client().release_funds(&f.learner, &id);
            })
        );
        assert!(result.is_err(), "Should panic on overflow during fee calculation");
    }

    #[test]
    fn test_zero_amount_validation() {
        let f = TestFixture::setup();
        let result = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                f.client().create_escrow(
                    &f.mentor,
                    &f.learner,
                    &0,
                    &symbol_short!("ZERO"),
                    &f.token_address,
                    &0u64
                );
            })
        );
        assert!(result.is_err(), "Should panic on zero amount");
// ---------------------------------------------------------------------------
// Unit tests — group escrow (issue #91)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod group_tests {
    extern crate std;
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::token::Client as TokenClient;

    #[test]
    fn test_group_three_learners_start_release() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let mentor = Address::generate(&env);
        let l1 = Address::generate(&env);
        let l2 = Address::generate(&env);
        let l3 = Address::generate(&env);
        let treasury = Address::generate(&env);

        let sac = env.register_stellar_asset_contract_v2(admin.clone());
        let token = sac.address();
        sac.mint(&l1, &10_000);
        sac.mint(&l2, &10_000);
        sac.mint(&l3, &10_000);

        let mut approved = Vec::new(&env);
        approved.push_back(token.clone());
        client.initialize(&admin, &treasury, &500u32, &approved, &0u64);

        let gid = client.create_group_escrow(
            &mentor,
            &3u32,
            &100i128,
            &token,
            &Symbol::new(&env, "GSESS1"),
        );

        client.join_group_escrow(&l1, &gid);
        client.join_group_escrow(&l2, &gid);
        client.join_group_escrow(&l3, &gid);

        let g = client.get_group_escrow(&gid);
        assert_eq!(g.status, GroupEscrowStatus::Full);
        assert_eq!(g.learners.len(), 3);

        client.start_group_session(&gid);

        let mentor_before = TokenClient::new(&env, &token).balance(&mentor);
        let treasury_before = TokenClient::new(&env, &token).balance(&treasury);
        client.release_group_funds(&gid);

        // 300 gross, 5% = 15 fee, 285 net
        assert_eq!(
            TokenClient::new(&env, &token).balance(&mentor),
            mentor_before + 285
        );
        assert_eq!(
            TokenClient::new(&env, &token).balance(&treasury),
            treasury_before + 15
        );
        assert_eq!(
            client.get_group_escrow(&gid).status,
            GroupEscrowStatus::Released
        );
    }

    #[test]
    fn test_group_cancel_refunds() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let mentor = Address::generate(&env);
        let l1 = Address::generate(&env);
        let l2 = Address::generate(&env);
        let treasury = Address::generate(&env);

        let sac = env.register_stellar_asset_contract_v2(admin.clone());
        let token = sac.address();
        sac.mint(&l1, &10_000);
        sac.mint(&l2, &10_000);

        let mut approved = Vec::new(&env);
        approved.push_back(token.clone());
        client.initialize(&admin, &treasury, &500u32, &approved, &0u64);

        let gid = client.create_group_escrow(
            &mentor,
            &4u32,
            &50i128,
            &token,
            &Symbol::new(&env, "GCAN1"),
        );

        client.join_group_escrow(&l1, &gid);
        client.join_group_escrow(&l2, &gid);

        let b1_before = TokenClient::new(&env, &token).balance(&l1);
        let b2_before = TokenClient::new(&env, &token).balance(&l2);

        client.cancel_group_escrow(&gid);

        assert_eq!(TokenClient::new(&env, &token).balance(&l1), b1_before + 50);
        assert_eq!(TokenClient::new(&env, &token).balance(&l2), b2_before + 50);
        assert_eq!(
            client.get_group_escrow(&gid).status,
            GroupEscrowStatus::Cancelled
        );
    }
}
