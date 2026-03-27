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
        env.storage()
            .persistent()
            .extend_ttl(&AUTO_REL_DLY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        for token_addr in approved_tokens.iter() {
            Self::_set_token_approved(&env, &token_addr, true);
        }
    }

    pub fn update_fee(env: Env, new_fee_bps: u32) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&ADMIN)
            .expect("Not initialized");
        env.storage()
            .persistent()
            .extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
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
        let admin: Address = env
            .storage()
            .persistent()
            .get(&ADMIN)
            .expect("Not initialized");
        env.storage()
            .persistent()
            .extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        admin.require_auth();
        env.storage().persistent().set(&TREASURY, &new_treasury);
        env.storage()
            .persistent()
            .extend_ttl(&TREASURY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
    }

    pub fn set_approved_token(env: Env, token_address: Address, approved: bool) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&ADMIN)
            .expect("Not initialized");
        env.storage()
            .persistent()
            .extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        admin.require_auth();
        Self::_set_token_approved(&env, &token_address, approved);
    }

    // -----------------------------------------------------------------------
    // Single-learner escrow
    // -----------------------------------------------------------------------

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

        caller.require_auth();
        if caller != escrow.learner && caller != admin {
            panic!("Caller not authorized");
        }

        let gross = escrow.amount;
        Self::_do_release(&env, &mut escrow, &key, gross);
    }

    pub fn release_partial(env: Env, caller: Address, escrow_id: u64) {
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

        let mut escrow: Escrow = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Escrow not found");

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
            (
                symbol_short!("Escrow"),
                symbol_short!("DispOpen"),
                escrow_id,
            ),
            DisputeOpenedEventData {
                caller,
                reason,
                token_address: escrow.token_address.clone(),
            },
        );
    }

    /// Split disputed funds: `true` = release to mentor (with platform fee), `false` = full refund to learner.
    pub fn resolve_dispute(env: Env, escrow_id: u64, release_to_mentor: bool) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&ADMIN)
            .expect("Not initialized");
        env.storage()
            .persistent()
            .extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        admin.require_auth();

        let key = (ESCROW_SYM, escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let mut escrow: Escrow = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Escrow not found");

        if escrow.status != EscrowStatus::Disputed {
            panic!("Escrow is not in Disputed status");
        }

        let now = env.ledger().timestamp();
        let amount = escrow.amount;

        if release_to_mentor {
            let fee_bps: u32 = env.storage().persistent().get(&FEE_BPS).unwrap_or(0u32);
            env.storage()
                .persistent()
                .extend_ttl(&FEE_BPS, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

            let platform_fee: i128 = amount
                .checked_mul(fee_bps as i128)
                .expect("Overflow")
                .checked_div(10_000)
                .expect("Division error");
            let net_amount: i128 = amount.checked_sub(platform_fee).expect("Underflow");

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

            escrow.status = EscrowStatus::Resolved;
            escrow.platform_fee = platform_fee;
            escrow.net_amount = net_amount;
            escrow.amount = 0;
            escrow.resolved_at = now;
            env.storage().persistent().set(&key, &escrow);

            let session_key = (SESSION_KEY, escrow.session_id.clone());
            env.storage().persistent().remove(&session_key);
        } else {
            let token_client = token::Client::new(&env, &escrow.token_address);
            token_client.transfer(&env.current_contract_address(), &escrow.learner, &amount);
            escrow.status = EscrowStatus::Resolved;
            escrow.net_amount = 0;
            escrow.platform_fee = amount;
            escrow.amount = 0;
            escrow.resolved_at = now;
            env.storage().persistent().set(&key, &escrow);

            let session_key = (SESSION_KEY, escrow.session_id.clone());
            env.storage().persistent().remove(&session_key);
        }
    }

    pub fn refund(env: Env, escrow_id: u64) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&ADMIN)
            .expect("Admin not found");
        env.storage()
            .persistent()
            .extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        admin.require_auth();

        let key = (ESCROW_SYM, escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let mut escrow: Escrow = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Escrow not found");

        if matches!(
            escrow.status,
            EscrowStatus::Released | EscrowStatus::Refunded | EscrowStatus::Resolved
        ) {
            panic!("Cannot refund");
        }

        let refund_amt = escrow.amount;
        let token_client = token::Client::new(&env, &escrow.token_address);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.learner,
            &refund_amt,
        );

        escrow.status = EscrowStatus::Refunded;
        escrow.amount = 0;
        env.storage().persistent().set(&key, &escrow);

        let session_key = (SESSION_KEY, escrow.session_id.clone());
        env.storage().persistent().remove(&session_key);

        env.events().publish(
            (symbol_short!("Escrow"), symbol_short!("Refund"), escrow_id),
            EscrowRefundedEventData {
                learner: escrow.learner.clone(),
                amount: refund_amt,
                token_address: escrow.token_address,
            },
        );
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
        let net_amount: i128 = gross.checked_sub(platform_fee).expect("Underflow");

        let treasury: Address = env
            .storage()
            .persistent()
            .get(&TREASURY)
            .expect("Treasury not found");
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
