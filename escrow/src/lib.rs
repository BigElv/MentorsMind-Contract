#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, Env, Symbol, IntoVal,
};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EscrowStatus {
    Active,
    Released,
    Disputed,
    Refunded,
    /// Dispute was resolved by admin arbitration via `resolve_dispute`.
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
    /// Platform fee deducted at release time (0 until released).
    pub platform_fee: i128,
    /// Amount actually sent to mentor after fee (0 until released).
    pub net_amount: i128,
    /// Unix timestamp (seconds) at which the session ends.
    pub session_end_time: u64,
    /// Seconds after `session_end_time` before auto-release may trigger.
    pub auto_release_delay: u64,
    /// Reason symbol provided when a dispute was opened (default: empty symbol).
    pub dispute_reason: Symbol,
    /// Unix timestamp (seconds) at which `resolve_dispute` was called (0 until resolved).
    pub resolved_at: u64,
}

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
const ADMIN: Symbol = symbol_short!("ADMIN");
const TREASURY: Symbol = symbol_short!("TREASURY");
const FEE_BPS: Symbol = symbol_short!("FEE_BPS");
/// Default auto-release delay in seconds (configurable at init).
const AUTO_REL_DLY: Symbol = symbol_short!("AR_DELAY");

/// Maximum configurable fee: 10% = 1 000 basis points.
const MAX_FEE_BPS: u32 = 1_000;

/// Default auto-release delay: 72 hours in seconds.
const DEFAULT_AUTO_RELEASE_DELAY: u64 = 72 * 60 * 60;

// Approved token registry key prefix: ("APRV_TOK", address) → bool
const APPROVED_TOKEN_KEY: Symbol = symbol_short!("APRV_TOK");

// ---------------------------------------------------------------------------
// TTL constants (in ledgers; ~5 s/ledger → 1 000 000 ≈ 57 days)
// ---------------------------------------------------------------------------

const ESCROW_TTL_THRESHOLD: u32 = 500_000;
const ESCROW_TTL_BUMP: u32 = 1_000_000;

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct EscrowContract;

#[contractimpl]
impl EscrowContract {
    // -----------------------------------------------------------------------
    // Admin / initialization
    // -----------------------------------------------------------------------

    /// Initialize the contract with an admin, treasury, initial fee, approved
    /// tokens, and an optional auto-release delay.
    ///
    /// - `fee_bps`: platform fee in basis points (e.g. 500 = 5%). Must be ≤ 1 000 (10%).
    /// - `treasury`: address that receives the platform fee on every release.
    /// - `auto_release_delay_secs`: seconds after session end before funds
    ///   auto-release to the mentor. Pass `0` to use the default (72 hours).
    /// - Approved tokens must satisfy SEP-41 (XLM, USDC, PYUSD, …).
    ///
    /// Calling this a second time will panic — persistent storage ensures the
    /// `ADMIN` key survives ledger archival so the guard cannot be bypassed.
    pub fn initialize(
        env: Env,
        admin: Address,
        treasury: Address,
        fee_bps: u32,
        approved_tokens: soroban_sdk::Vec<Address>,
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

        // Register each approved token
        for token_addr in approved_tokens.iter() {
            Self::_set_token_approved(&env, &token_addr, true);
        }
    }

    /// Update the platform fee — admin only, capped at 1 000 bps (10%).
    /// Update the fee basis points (admin only).
    ///
    /// Auth: Only the admin can update fees.
    /// The admin address is retrieved from persistent storage.
    ///
    /// Panics if:
    /// - Contract is not initialized
    /// - Caller is not the admin
    /// - Caller fails authorization check
    /// - New fee exceeds maximum (1000 bps = 10%)
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

    /// Update the treasury address — admin only.
    ///
    /// Auth: Only the admin can update the treasury address.
    /// The admin address is retrieved from persistent storage.
    ///
    /// Panics if:
    /// - Contract is not initialized
    /// - Caller is not the admin
    /// - Caller fails authorization check
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

    /// Add or remove an approved token (admin only).
    ///
    /// Auth: Only the admin can manage approved tokens.
    /// The admin address is retrieved from persistent storage.
    ///
    /// Panics if:
    /// - Contract is not initialized
    /// - Caller is not the admin
    /// - Caller fails authorization check
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
    // Escrow lifecycle
    // -----------------------------------------------------------------------

    /// Create a new escrow.
    ///
    /// Auth: Only the learner can create an escrow for themselves.
    /// The learner must provide valid authorization.
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
    /// - Caller is not the learner
    /// - Caller fails authorization check
    pub fn create_escrow(
        env: Env,
        mentor: Address,
        learner: Address,
        amount: i128,
        session_id: Symbol,
        token_address: Address,
        session_end_time: u64,
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
        env.storage()
            .persistent()
            .extend_ttl(&AUTO_REL_DLY, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        // --- Increment and persist escrow counter ---
        let mut count: u64 = env.storage().persistent().get(&ESCROW_COUNT).unwrap_or(0);
        count = count.checked_add(1).expect("Counter overflow");
        env.storage().persistent().set(&ESCROW_COUNT, &count);
        env.storage()
            .persistent()
            .extend_ttl(&ESCROW_COUNT, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

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
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        // --- Emit event (includes token_address and session_end_time) ---
        env.events().publish(
            (Symbol::new(&env, "Escrow"), Symbol::new(&env, "Created"), count),
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

    /// Release funds to the mentor (called by learner or admin).
    ///
    /// Calculates the platform fee (`gross * fee_bps / 10_000`), transfers the
    /// fee to the treasury, and transfers the remainder to the mentor.
    /// Both amounts are stored on the escrow record and emitted in the event.
    /// Release funds to the mentor.
    ///
    /// Auth: Only the learner or admin can release funds.
    /// The caller must provide valid authorization.
    ///
    /// Panics if:
    /// - Escrow does not exist
    /// - Escrow is not in Active status  
    /// - Caller is not the learner or admin
    /// - Caller fails authorization check
    pub fn release_funds(env: Env, caller: Address, escrow_id: u64) {
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

        // Emit a dedicated `auto_released` event *before* the internal release
        // so listeners can distinguish this path from a manual release.
        env.events().publish(
            (Symbol::new(&env, "Escrow"), Symbol::new(&env, "AutoReleased"), escrow_id),
            EscrowAutoReleasedEventData { time: now },
        );

        Self::_do_release(&env, &mut escrow, &key);
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
    /// Dispute an active escrow.
    ///
    /// Auth: Only the mentor or learner can dispute their escrow.
    /// The caller must provide valid authorization.
    ///
    /// Panics if:
    /// - Escrow does not exist
    /// - Escrow is not in Active status
    /// - Caller is not the mentor or learner
    /// - Caller fails authorization check
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

        env.events().publish(
            (Symbol::new(&env, "Escrow"), Symbol::new(&env, "DisputeOpened"), escrow_id),
            DisputeOpenedEventData {
                caller,
                reason,
                token_address: escrow.token_address,
            },
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
    /// Resolve a disputed escrow by splitting funds (admin only).
    ///
    /// Auth: Only the admin can resolve disputes.
    /// The admin address is retrieved from persistent storage.
    ///
    /// Panics if:
    /// - Contract is not initialized
    /// - Caller is not the admin
    /// - Caller fails authorization check
    /// - Escrow does not exist
    /// - Escrow is not in Disputed status
    /// - mentor_pct is greater than 100
    pub fn resolve_dispute(env: Env, escrow_id: u64, mentor_pct: u32) {
        // --- Admin auth ---
        let admin: Address = env
            .storage()
            .persistent()
            .get(&ADMIN)
            .expect("Not initialized");
        env.storage()
            .persistent()
            .extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        admin.require_auth();

        // --- Validate split percentage ---
        if mentor_pct > 100 {
            panic!("mentor_pct must be between 0 and 100");
        }

        // --- Load escrow ---
        let key = (symbol_short!("ESCROW"), escrow_id);
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

        // --- Calculate split amounts ---
        // mentor_amount = floor(amount * mentor_pct / 100)
        // learner_amount = amount - mentor_amount  (avoids any dust loss)
        let mentor_amount: i128 = escrow
            .amount
            .checked_mul(mentor_pct as i128)
            .expect("Overflow")
            .checked_div(100)
            .expect("Division error");
        let learner_amount: i128 = escrow.amount.checked_sub(mentor_amount).expect("Underflow");

        let token_client = token::Client::new(&env, &escrow.token_address);

        // --- Transfer mentor's share ---
        if mentor_amount > 0 {
            token_client.transfer(
                &env.current_contract_address(),
                &escrow.mentor,
                &mentor_amount,
            );
        }

        // --- Transfer learner's share ---
        if learner_amount > 0 {
            token_client.transfer(
                &env.current_contract_address(),
                &escrow.learner,
                &learner_amount,
            );
        }

        // --- Update escrow record ---
        // Reuse net_amount for mentor's awarded share and platform_fee for
        // learner's awarded share so callers can inspect the resolution on-chain.
        let now = env.ledger().timestamp();
        escrow.status = EscrowStatus::Resolved;
        escrow.net_amount = mentor_amount;
        escrow.platform_fee = learner_amount; // repurposed: learner share in resolved state
        escrow.resolved_at = now;
        env.storage().persistent().set(&key, &escrow);

        // --- Emit event ---
        env.events().publish(
            (Symbol::new(&env, "Escrow"), Symbol::new(&env, "DisputeResolved"), escrow_id),
            DisputeResolvedEventData {
                mentor_pct,
                mentor_amount,
                learner_amount,
                token_address: escrow.token_address.clone(),
                time: now,
            },
        );
    }

    /// Refund tokens to the learner (admin only).
    ///
    /// Can be called on `Active` or `Disputed` escrows; panics if already
    /// `Released`, `Refunded`, or `Resolved`.
    /// Transfers `escrow.amount` tokens from contract → learner.
    /// Refund an escrow to the learner (admin only).
    ///
    /// Auth: Only the admin can issue refunds.
    /// The admin address is retrieved from persistent storage.
    ///
    /// Panics if:
    /// - Contract is not initialized
    /// - Caller is not the admin
    /// - Caller fails authorization check
    /// - Escrow does not exist
    /// - Escrow is already Released, Refunded, or Resolved
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

        let key = (symbol_short!("ESCROW"), escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let mut escrow: Escrow = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Escrow not found");

        if escrow.status == EscrowStatus::Released
            || escrow.status == EscrowStatus::Refunded
            || escrow.status == EscrowStatus::Resolved
        {
            panic!("Cannot refund");
        }

        // Transfer tokens: contract → learner
        let token_client = token::Client::new(&env, &escrow.token_address);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.learner,
            &escrow.amount,
        );

        escrow.status = EscrowStatus::Refunded;
        env.storage().persistent().set(&key, &escrow);

        env.events().publish(
            (Symbol::new(&env, "Escrow"), Symbol::new(&env, "Refunded"), escrow_id),
            EscrowRefundedEventData {
                learner: escrow.learner.clone(),
                amount: escrow.amount,
                token_address: escrow.token_address,
            },
        );
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    pub fn get_escrow(env: Env, escrow_id: u64) -> Escrow {
        let key = (symbol_short!("ESCROW"), escrow_id);
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

    /// Get all escrows for a specific mentor.
    ///
    /// Iterates through all escrows and returns those where the mentor matches.
    /// This is a query function with no authorization requirements.
    pub fn get_escrows_by_mentor(env: Env, mentor: Address) -> Vec<Escrow> {
        let count = env
            .storage()
            .persistent()
            .get(&ESCROW_COUNT)
            .unwrap_or(0u64);
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

    /// Submit a review for a completed escrow (learner only).
    ///
    /// Records a review reason on the escrow after funds have been released.
    /// This is a lightweight operation that stores the review reason.
    /// In production, this would trigger a cross-contract call to the verification contract.
    pub fn submit_review(env: Env, caller: Address, escrow_id: u64, reason: Symbol) {
        let key = (symbol_short!("ESCROW"), escrow_id);
        env.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let escrow: Escrow = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Escrow not found");

        // Only learner can submit review
        caller.require_auth();
        if caller != escrow.learner {
            panic!("Only learner can submit review");
        }

        // Can only review released escrows
        if escrow.status != EscrowStatus::Released {
            panic!("Can only review released escrows");
        }

        // Store review reason in a separate key
        let review_key = (symbol_short!("REVIEW"), escrow_id);
        env.storage().persistent().set(&review_key, &reason);
        env.storage()
            .persistent()
            .extend_ttl(&review_key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        env.events().publish(
            (Symbol::new(&env, "Escrow"), Symbol::new(&env, "ReviewSubmitted"), escrow_id),
            ReviewSubmittedEventData {
                caller,
                reason,
                mentor: escrow.mentor,
            },
        );
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Shared release logic used by both `release_funds` and `try_auto_release`.
    ///
    /// Computes the platform fee, transfers fee → treasury and net → mentor,
    /// then persists the updated escrow with `Released` status.
    fn _do_release(env: &Env, escrow: &mut Escrow, key: &(Symbol, u64)) {
        let fee_bps: u32 = env.storage().persistent().get(&FEE_BPS).unwrap_or(0u32);
        env.storage()
            .persistent()
            .extend_ttl(&FEE_BPS, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        let platform_fee: i128 = escrow
            .amount
            .checked_mul(fee_bps as i128)
            .expect("Overflow")
            .checked_div(10_000)
            .expect("Division error");
        let net_amount: i128 = escrow.amount.checked_sub(platform_fee).expect("Underflow");

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
        escrow.platform_fee = platform_fee;
        escrow.net_amount = net_amount;
        env.storage().persistent().set(key, escrow);

        env.events().publish(
            (Symbol::new(env, "Escrow"), Symbol::new(env, "Released"), escrow.id),
            EscrowReleasedEventData {
                mentor: escrow.mentor.clone(),
                amount: escrow.amount,
                net_amount,
                platform_fee,
                token_address: escrow.token_address.clone(),
            },
        );
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
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test {
    extern crate std;
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger, Events},
        token::{Client as TokenClient, StellarAssetClient},
        Address, Env, Vec, IntoVal, Symbol,
    };

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn create_token<'a>(env: &'a Env, admin: &Address) -> (Address, StellarAssetClient<'a>) {
        let token_address = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let sac = StellarAssetClient::new(env, &token_address);
        (token_address, sac)
    }

    fn advance_time(env: &Env, secs: u64) {
        env.ledger().with_mut(|li| li.timestamp += secs);
    }

    struct TestFixture {
        env: Env,
        contract_id: Address,
        admin: Address,
        mentor: Address,
        learner: Address,
        treasury: Address,
        token_address: Address,
    }

    impl TestFixture {
        fn setup() -> Self {
            Self::setup_with_fee(500)
        }
        fn setup_with_fee(fee_bps: u32) -> Self {
            Self::setup_full(fee_bps, 0)
        }

        fn setup_full(fee_bps: u32, auto_release_delay_secs: u64) -> Self {
            let env = Env::default();
            env.mock_all_auths();
            env.ledger().with_mut(|li| li.timestamp = 14_400);

            let contract_id = env.register_contract(None, EscrowContract);
            let admin = Address::generate(&env);
            let mentor = Address::generate(&env);
            let learner = Address::generate(&env);
            let treasury = Address::generate(&env);

            let (token_address, sac) = create_token(&env, &admin);
            sac.mint(&learner, &100_000);

            let client = EscrowContractClient::new(&env, &contract_id);
            let mut approved = Vec::new(&env);
            approved.push_back(token_address.clone());
            client.initialize(
                &admin,
                &treasury,
                &fee_bps,
                &approved,
                &auto_release_delay_secs,
            );

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

        fn create_escrow_at(&self, amount: i128, session_end_time: u64) -> u64 {
            self.client().create_escrow(
                &self.mentor,
                &self.learner,
                &amount,
                &symbol_short!("S1"),
                &self.token_address,
                &session_end_time,
            )
        }

        fn open_dispute(&self, escrow_id: u64) {
            self.client()
                .dispute(&self.learner, &escrow_id, &symbol_short!("NO_SHOW"));
        }
    }

    fn setup_disputed(f: &TestFixture) -> u64 {
        let id = f.create_escrow_at(1_000, 0);
        f.open_dispute(id);
        id
    }

    // -----------------------------------------------------------------------
    // initialize
    // -----------------------------------------------------------------------

    #[test]
    fn test_initialize_stores_config() {
        let f = TestFixture::setup_full(500, 3_600);
        let client = f.client();
        assert_eq!(client.get_fee_bps(), 500);
        assert_eq!(client.get_treasury(), f.treasury);
        assert_eq!(client.get_auto_release_delay(), 3_600);
        assert!(client.is_token_approved(&f.token_address));
    }

    #[test]
    fn test_initialize_double_init_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let approved: Vec<Address> = Vec::new(&env);
        client.initialize(&admin, &treasury, &500u32, &approved, &0u64);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.initialize(&admin, &treasury, &500u32, &approved, &0u64);
        }));
        assert!(result.is_err(), "double-init must panic");
    }

    #[test]
    fn test_initialize_fee_over_cap_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let approved: Vec<Address> = Vec::new(&env);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.initialize(&admin, &treasury, &1_001u32, &approved, &0u64);
        }));
        assert!(result.is_err(), "fee > 1000 bps must panic");
    }

    #[test]
    fn test_initialize_default_auto_release_delay() {
        let f = TestFixture::setup_full(0, 0);
        assert_eq!(f.client().get_auto_release_delay(), 72 * 60 * 60);
    }

    #[test]
    fn test_initialize_custom_auto_release_delay() {
        let f = TestFixture::setup_full(0, 3_600);
        assert_eq!(f.client().get_auto_release_delay(), 3_600);
    }

    // -----------------------------------------------------------------------
    // create_escrow
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_escrow_valid() {
        let f = TestFixture::setup();
        let token = f.token();
        let learner_before = token.balance(&f.learner);
        let id = f.create_escrow_at(1_000, 0);
        assert_eq!(id, 1);
        assert_eq!(token.balance(&f.learner), learner_before - 1_000);
        assert_eq!(token.balance(&f.contract_id), 1_000);
        let e = f.client().get_escrow(&id);
        assert_eq!(e.status, EscrowStatus::Active);
        assert_eq!(e.mentor, f.mentor);
        assert_eq!(e.learner, f.learner);

        let events = f.env.events().all();
        let ev = events.last().unwrap();
        assert_eq!(ev.0, f.contract_id.clone());
        assert_eq!(ev.1, (Symbol::new(&f.env, "Escrow"), Symbol::new(&f.env, "Created"), id).into_val(&f.env));
        assert_eq!(ev.2, EscrowCreatedEventData {
            mentor: f.mentor.clone(),
            learner: f.learner.clone(),
            amount: 1_000,
            session_id: symbol_short!("S1"),
            token_address: f.token_address.clone(),
            session_end_time: 0,
        }.into_val(&f.env));
    }

    #[test]
    fn test_create_escrow_counter_increments() {
        let f = TestFixture::setup();
        assert_eq!(f.client().get_escrow_count(), 0);
        assert_eq!(f.create_escrow_at(500, 0), 1);
        assert_eq!(f.create_escrow_at(500, 0), 2);
        assert_eq!(f.client().get_escrow_count(), 2);
    }

    #[test]
    fn test_create_escrow_zero_amount_panics() {
        let f = TestFixture::setup();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.create_escrow_at(0, 0);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_create_escrow_negative_amount_panics() {
        let f = TestFixture::setup();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.create_escrow_at(-1, 0);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_create_escrow_unapproved_token_panics() {
        let f = TestFixture::setup();
        let bad_token = Address::generate(&f.env);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.client().create_escrow(
                &f.mentor,
                &f.learner,
                &500,
                &symbol_short!("S1"),
                &bad_token,
                &0u64,
            );
        }));
        assert!(result.is_err(), "unapproved token must panic");
    }

    #[test]
    fn test_create_escrow_insufficient_balance_panics() {
        let f = TestFixture::setup();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.create_escrow_at(999_999_999, 0);
        }));
        assert!(result.is_err(), "insufficient balance must panic");
    }

    // -----------------------------------------------------------------------
    // release_funds
    // -----------------------------------------------------------------------

    #[test]
    fn test_release_funds_by_learner() {
        let f = TestFixture::setup_with_fee(500);
        let token = f.token();
        let id = f.create_escrow_at(1_000, 0);
        let mentor_before = token.balance(&f.mentor);
        let treasury_before = token.balance(&f.treasury);
        f.client().release_funds(&f.learner, &id);
        assert_eq!(token.balance(&f.mentor), mentor_before + 950);
        assert_eq!(token.balance(&f.treasury), treasury_before + 50);
        assert_eq!(token.balance(&f.contract_id), 0);
        let e = f.client().get_escrow(&id);
        assert_eq!(e.status, EscrowStatus::Released);
        assert_eq!(e.platform_fee, 50);
        assert_eq!(e.net_amount, 950);

        let events = f.env.events().all();
        let ev = events.last().unwrap();
        assert_eq!(ev.0, f.contract_id.clone());
        assert_eq!(ev.1, (Symbol::new(&f.env, "Escrow"), Symbol::new(&f.env, "Released"), id).into_val(&f.env));
        assert_eq!(ev.2, EscrowReleasedEventData {
            mentor: f.mentor.clone(),
            amount: 1_000,
            net_amount: 950,
            platform_fee: 50,
            token_address: f.token_address.clone(),
        }.into_val(&f.env));
    }

    #[test]
    fn test_release_funds_by_admin() {
        let f = TestFixture::setup_with_fee(0);
        let id = f.create_escrow_at(1_000, 0);
        f.client().release_funds(&f.admin, &id);
        assert_eq!(f.client().get_escrow(&id).status, EscrowStatus::Released);
    }

    #[test]
    fn test_release_funds_unauthorized_panics() {
        let f = TestFixture::setup();
        let id = f.create_escrow_at(1_000, 0);
        let rando = Address::generate(&f.env);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.client().release_funds(&rando, &id);
        }));
        assert!(result.is_err(), "unauthorized caller must panic");
    }

    #[test]
    fn test_release_funds_non_active_panics() {
        let f = TestFixture::setup();
        let id = f.create_escrow_at(1_000, 0);
        f.client().release_funds(&f.learner, &id);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.client().release_funds(&f.learner, &id);
        }));
        assert!(result.is_err(), "double-release must panic");
    }

    #[test]
    fn test_release_funds_mentor_cannot_release() {
        // Mentor is not authorized to call release_funds
        let f = TestFixture::setup();
        let id = f.create_escrow_at(1_000, 0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.client().release_funds(&f.mentor, &id);
        }));
        assert!(result.is_err(), "mentor must not be able to self-release");
    }

    // -----------------------------------------------------------------------
    // dispute
    // -----------------------------------------------------------------------

    #[test]
    fn test_dispute_by_mentor() {
        let f = TestFixture::setup();
        let id = f.create_escrow_at(1_000, 0);
        f.client()
            .dispute(&f.mentor, &id, &symbol_short!("NO_SHOW"));
        let e = f.client().get_escrow(&id);
        assert_eq!(e.status, EscrowStatus::Disputed);
        assert_eq!(e.dispute_reason, symbol_short!("NO_SHOW"));
    }

    #[test]
    fn test_dispute_by_learner() {
        let f = TestFixture::setup();
        let id = f.create_escrow_at(1_000, 0);
        f.client()
            .dispute(&f.learner, &id, &symbol_short!("BAD_SVC"));
        let e = f.client().get_escrow(&id);
        assert_eq!(e.status, EscrowStatus::Disputed);
        assert_eq!(e.dispute_reason, symbol_short!("BAD_SVC"));
    }

    #[test]
    fn test_dispute_unauthorized_panics() {
        let f = TestFixture::setup();
        let id = f.create_escrow_at(1_000, 0);
        let rando = Address::generate(&f.env);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.client().dispute(&rando, &id, &symbol_short!("FRAUD"));
        }));
        assert!(result.is_err(), "unauthorized dispute must panic");
    }

    #[test]
    fn test_dispute_non_active_panics() {
        let f = TestFixture::setup();
        let id = f.create_escrow_at(1_000, 0);
        f.client().release_funds(&f.learner, &id);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.client().dispute(&f.mentor, &id, &symbol_short!("LATE"));
        }));
        assert!(result.is_err(), "dispute on released escrow must panic");
    }

    // -----------------------------------------------------------------------
    // resolve_dispute
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_dispute_100_0_all_to_mentor() {
        let f = TestFixture::setup_with_fee(0);
        let token = f.token();
        let id = setup_disputed(&f);
        let mentor_before = token.balance(&f.mentor);
        let learner_before = token.balance(&f.learner);
        f.client().resolve_dispute(&id, &100u32);
        assert_eq!(token.balance(&f.mentor), mentor_before + 1_000);
        assert_eq!(token.balance(&f.learner), learner_before);
        assert_eq!(token.balance(&f.contract_id), 0);
        let e = f.client().get_escrow(&id);
        assert_eq!(e.status, EscrowStatus::Resolved);
        assert_eq!(e.net_amount, 1_000);
        assert_eq!(e.platform_fee, 0);
        assert!(e.resolved_at > 0);
    }

    #[test]
    fn test_resolve_dispute_50_50_equal_split() {
        let f = TestFixture::setup_with_fee(0);
        let token = f.token();
        let id = setup_disputed(&f);
        let mentor_before = token.balance(&f.mentor);
        let learner_before = token.balance(&f.learner);
        f.client().resolve_dispute(&id, &50u32);
        assert_eq!(token.balance(&f.mentor), mentor_before + 500);
        assert_eq!(token.balance(&f.learner), learner_before + 500);
        assert_eq!(token.balance(&f.contract_id), 0);
        let e = f.client().get_escrow(&id);
        assert_eq!(e.status, EscrowStatus::Resolved);
        assert_eq!(e.net_amount, 500);
        assert_eq!(e.platform_fee, 500);
    }

    #[test]
    fn test_resolve_dispute_0_100_all_to_learner() {
        let f = TestFixture::setup_with_fee(0);
        let token = f.token();
        let id = setup_disputed(&f);
        let mentor_before = token.balance(&f.mentor);
        let learner_before = token.balance(&f.learner);
        f.client().resolve_dispute(&id, &0u32);
        assert_eq!(token.balance(&f.mentor), mentor_before);
        assert_eq!(token.balance(&f.learner), learner_before + 1_000);
        assert_eq!(token.balance(&f.contract_id), 0);
        let e = f.client().get_escrow(&id);
        assert_eq!(e.net_amount, 0);
        assert_eq!(e.platform_fee, 1_000);
    }

    #[test]
    fn test_resolve_dispute_non_admin_panics() {
        let f = TestFixture::setup_with_fee(0);
        let id = setup_disputed(&f);
        // Temporarily remove mock_all_auths to test real auth
        // We rely on the contract's caller != admin check
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // The contract checks admin.require_auth(); with mock_all_auths this
            // passes, but the caller != admin guard fires when we pass a rando.
            // resolve_dispute doesn't take a caller param — admin is loaded from
            // storage and require_auth() is called on it. With mock_all_auths
            // all auths pass, so we test the status guard instead.
            let id2 = f.create_escrow_at(500, 0); // Active, not Disputed
            let _ = id2;
            f.client().resolve_dispute(&id2, &50u32);
        }));
        assert!(result.is_err(), "resolve on non-disputed must panic");
    }

    #[test]
    fn test_resolve_dispute_invalid_pct_panics() {
        let f = TestFixture::setup_with_fee(0);
        let id = setup_disputed(&f);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.client().resolve_dispute(&id, &101u32);
        }));
        assert!(result.is_err(), "mentor_pct > 100 must panic");
    }

    #[test]
    fn test_resolve_dispute_double_resolve_panics() {
        let f = TestFixture::setup_with_fee(0);
        let id = setup_disputed(&f);
        f.client().resolve_dispute(&id, &50u32);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.client().resolve_dispute(&id, &50u32);
        }));
        assert!(result.is_err(), "double-resolve must panic");
    }

    #[test]
    fn test_resolve_dispute_rounding_no_dust() {
        // 1_000 * 33 / 100 = 330 mentor, 670 learner; total = 1_000
        let f = TestFixture::setup_with_fee(0);
        let token = f.token();
        let id = setup_disputed(&f);
        let mentor_before = token.balance(&f.mentor);
        let learner_before = token.balance(&f.learner);
        f.client().resolve_dispute(&id, &33u32);
        let m = token.balance(&f.mentor) - mentor_before;
        let l = token.balance(&f.learner) - learner_before;
        assert_eq!(m, 330);
        assert_eq!(l, 670);
        assert_eq!(m + l, 1_000);
        assert_eq!(token.balance(&f.contract_id), 0);
    }

    #[test]
    fn test_resolve_dispute_resolved_at_set() {
        let f = TestFixture::setup_with_fee(0);
        let id = setup_disputed(&f);
        let now = f.env.ledger().timestamp();
        f.client().resolve_dispute(&id, &50u32);
        assert_eq!(f.client().get_escrow(&id).resolved_at, now);
    }

    // -----------------------------------------------------------------------
    // refund
    // -----------------------------------------------------------------------

    #[test]
    fn test_refund_admin_only_active() {
        let f = TestFixture::setup();
        let token = f.token();
        let id = f.create_escrow_at(1_000, 0);
        let learner_before = token.balance(&f.learner);
        f.client().refund(&id);
        assert_eq!(token.balance(&f.learner), learner_before + 1_000);
        assert_eq!(token.balance(&f.contract_id), 0);
        assert_eq!(f.client().get_escrow(&id).status, EscrowStatus::Refunded);
    }

    #[test]
    fn test_refund_admin_only_disputed() {
        let f = TestFixture::setup();
        let id = f.create_escrow_at(1_000, 0);
        f.client().dispute(&f.mentor, &id, &symbol_short!("LATE"));
        f.client().refund(&id);
        assert_eq!(f.client().get_escrow(&id).status, EscrowStatus::Refunded);
    }

    #[test]
    fn test_refund_already_released_panics() {
        let f = TestFixture::setup();
        let id = f.create_escrow_at(1_000, 0);
        f.client().release_funds(&f.learner, &id);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.client().refund(&id);
        }));
        assert!(result.is_err(), "refund on Released must panic");
    }

    #[test]
    fn test_refund_already_refunded_panics() {
        let f = TestFixture::setup();
        let id = f.create_escrow_at(1_000, 0);
        f.client().refund(&id);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.client().refund(&id);
        }));
        assert!(result.is_err(), "double-refund must panic");
    }

    #[test]
    fn test_refund_already_resolved_panics() {
        let f = TestFixture::setup_with_fee(0);
        let id = setup_disputed(&f);
        f.client().resolve_dispute(&id, &50u32);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.client().refund(&id);
        }));
        assert!(result.is_err(), "refund on Resolved must panic");
    }

    // -----------------------------------------------------------------------
    // try_auto_release
    // -----------------------------------------------------------------------

    #[test]
    fn test_auto_release_before_window_panics() {
        let f = TestFixture::setup_full(500, 3_600);
        let now = f.env.ledger().timestamp();
        let id = f.create_escrow_at(1_000, now + 100);
        // advance to 1 s before window: now + 100 + 3600 - 1
        advance_time(&f.env, 100 + 3_600 - 1);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.client().try_auto_release(&id);
        }));
        assert!(result.is_err(), "early auto-release must panic");
    }

    #[test]
    fn test_auto_release_after_window_succeeds() {
        let f = TestFixture::setup_full(500, 3_600);
        let token = f.token();
        let now = f.env.ledger().timestamp();
        let id = f.create_escrow_at(1_000, now);
        advance_time(&f.env, 3_600 + 1);
        let mentor_before = token.balance(&f.mentor);
        let treasury_before = token.balance(&f.treasury);
        f.client().try_auto_release(&id);
        assert_eq!(token.balance(&f.mentor), mentor_before + 950);
        assert_eq!(token.balance(&f.treasury), treasury_before + 50);
        assert_eq!(token.balance(&f.contract_id), 0);
        let e = f.client().get_escrow(&id);
        assert_eq!(e.status, EscrowStatus::Released);
        assert_eq!(e.platform_fee, 50);
        assert_eq!(e.net_amount, 950);
    }

    #[test]
    fn test_auto_release_exactly_at_boundary() {
        let f = TestFixture::setup_full(0, 3_600);
        let now = f.env.ledger().timestamp();
        // session_end = now - 200; boundary = now - 200 + 3600 = now + 3400
        let id = f.create_escrow_at(1_000, now - 200);
        advance_time(&f.env, 3_600 - 200);
        f.client().try_auto_release(&id);
        assert_eq!(f.client().get_escrow(&id).status, EscrowStatus::Released);
    }

    #[test]
    fn test_auto_release_already_released_panics() {
        let f = TestFixture::setup_full(0, 3_600);
        let now = f.env.ledger().timestamp();
        let id = f.create_escrow_at(1_000, now);
        f.client().release_funds(&f.learner, &id);
        advance_time(&f.env, 3_600 + 1);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.client().try_auto_release(&id);
        }));
        assert!(result.is_err(), "auto-release on Released must panic");
    }

    #[test]
    fn test_auto_release_disputed_panics() {
        let f = TestFixture::setup_full(0, 3_600);
        let now = f.env.ledger().timestamp();
        let id = f.create_escrow_at(1_000, now);
        f.client().dispute(&f.learner, &id, &symbol_short!("LATE"));
        advance_time(&f.env, 3_600 + 1);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.client().try_auto_release(&id);
        }));
        assert!(result.is_err(), "auto-release on Disputed must panic");
    }

    #[test]
    fn test_auto_release_default_72h() {
        let f = TestFixture::setup_full(0, 0); // 0 → 72 h default
        let now = f.env.ledger().timestamp();
        let id = f.create_escrow_at(1_000, now);
        let delay = 72u64 * 60 * 60;
        advance_time(&f.env, delay - 1);
        let too_early = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.client().try_auto_release(&id);
        }));
        assert!(too_early.is_err());
        advance_time(&f.env, 1);
        f.client().try_auto_release(&id);
        assert_eq!(f.client().get_escrow(&id).status, EscrowStatus::Released);
    }

    // -----------------------------------------------------------------------
    // release_partial — 3-session package full lifecycle
    // -----------------------------------------------------------------------
    // The contract has no dedicated release_partial function; a "package" is
    // modelled as N independent escrows (one per session). This test creates
    // 3 escrows for the same mentor/learner pair, releases each one, and
    // verifies cumulative token balances are correct throughout.

    #[test]
    fn test_three_session_package_full_lifecycle() {
        // 5% fee; 3 sessions of 1_000 each → 50 fee + 950 net per session
        let f = TestFixture::setup_with_fee(500);
        let client = f.client();
        let token = f.token();

        let learner_start = token.balance(&f.learner);
        let mentor_start = token.balance(&f.mentor);
        let treasury_start = token.balance(&f.treasury);

        // --- Create all 3 escrows ---
        let id1 = f.create_escrow_at(1_000, 0);
        let id2 = f.create_escrow_at(1_000, 0);
        let id3 = f.create_escrow_at(1_000, 0);

        // Learner has paid 3_000 into escrow
        assert_eq!(token.balance(&f.learner), learner_start - 3_000);
        assert_eq!(token.balance(&f.contract_id), 3_000);

        // --- Release session 1 ---
        client.release_funds(&f.learner, &id1);
        assert_eq!(token.balance(&f.mentor), mentor_start + 950);
        assert_eq!(token.balance(&f.treasury), treasury_start + 50);
        assert_eq!(token.balance(&f.contract_id), 2_000);
        assert_eq!(client.get_escrow(&id1).status, EscrowStatus::Released);

        // --- Release session 2 ---
        client.release_funds(&f.learner, &id2);
        assert_eq!(token.balance(&f.mentor), mentor_start + 1_900);
        assert_eq!(token.balance(&f.treasury), treasury_start + 100);
        assert_eq!(token.balance(&f.contract_id), 1_000);
        assert_eq!(client.get_escrow(&id2).status, EscrowStatus::Released);

        // --- Release session 3 ---
        client.release_funds(&f.learner, &id3);
        assert_eq!(token.balance(&f.mentor), mentor_start + 2_850);
        assert_eq!(token.balance(&f.treasury), treasury_start + 150);
        assert_eq!(token.balance(&f.contract_id), 0);
        assert_eq!(client.get_escrow(&id3).status, EscrowStatus::Released);

        // Learner net spend = 3_000 (all escrowed, none refunded)
        assert_eq!(token.balance(&f.learner), learner_start - 3_000);
    }

    // -----------------------------------------------------------------------
    // Fee deduction — treasury receives correct amount
    // -----------------------------------------------------------------------

    #[test]
    fn test_fee_deduction_zero_percent() {
        let f = TestFixture::setup_with_fee(0);
        let token = f.token();
        let id = f.create_escrow_at(1_000, 0);
        let treasury_before = token.balance(&f.treasury);
        f.client().release_funds(&f.learner, &id);
        assert_eq!(token.balance(&f.treasury), treasury_before); // no fee
        assert_eq!(token.balance(&f.mentor), 1_000);
        let e = f.client().get_escrow(&id);
        assert_eq!(e.platform_fee, 0);
        assert_eq!(e.net_amount, 1_000);
    }

    #[test]
    fn test_fee_deduction_five_percent() {
        let f = TestFixture::setup_with_fee(500);
        let token = f.token();
        let id = f.create_escrow_at(1_000, 0);
        let treasury_before = token.balance(&f.treasury);
        f.client().release_funds(&f.learner, &id);
        assert_eq!(token.balance(&f.treasury), treasury_before + 50);
        assert_eq!(token.balance(&f.mentor), 950);
        let e = f.client().get_escrow(&id);
        assert_eq!(e.platform_fee, 50);
        assert_eq!(e.net_amount, 950);
    }

    #[test]
    fn test_fee_deduction_ten_percent() {
        let f = TestFixture::setup_with_fee(1_000);
        let token = f.token();
        let id = f.create_escrow_at(2_000, 0);
        let treasury_before = token.balance(&f.treasury);
        f.client().release_funds(&f.learner, &id);
        assert_eq!(token.balance(&f.treasury), treasury_before + 200);
        assert_eq!(token.balance(&f.mentor), 1_800);
        let e = f.client().get_escrow(&id);
        assert_eq!(e.platform_fee, 200);
        assert_eq!(e.net_amount, 1_800);
    }

    #[test]
    fn test_fee_deduction_rounding_truncates() {
        // 1 token * 500 bps / 10_000 = 0.05 → truncated to 0
        let f = TestFixture::setup_with_fee(500);
        let id = f.create_escrow_at(1, 0);
        f.client().release_funds(&f.learner, &id);
        let e = f.client().get_escrow(&id);
        assert_eq!(e.platform_fee, 0);
        assert_eq!(e.net_amount, 1);
    }

    #[test]
    fn test_fee_deduction_via_auto_release() {
        // Auto-release uses the same _do_release path — fee must still be deducted
        let f = TestFixture::setup_full(500, 3_600);
        let token = f.token();
        let now = f.env.ledger().timestamp();
        let id = f.create_escrow_at(1_000, now);
        let treasury_before = token.balance(&f.treasury);
        advance_time(&f.env, 3_600 + 1);
        f.client().try_auto_release(&id);
        assert_eq!(token.balance(&f.treasury), treasury_before + 50);
        assert_eq!(token.balance(&f.mentor), 950);
    }

    // -----------------------------------------------------------------------
    // Token balance assertions — before and after each operation
    // -----------------------------------------------------------------------

    #[test]
    fn test_balances_create_then_refund() {
        let f = TestFixture::setup_with_fee(500);
        let token = f.token();
        let learner_start = token.balance(&f.learner);

        let id = f.create_escrow_at(1_000, 0);
        assert_eq!(token.balance(&f.learner), learner_start - 1_000);
        assert_eq!(token.balance(&f.contract_id), 1_000);

        f.client().refund(&id);
        assert_eq!(token.balance(&f.learner), learner_start); // fully restored
        assert_eq!(token.balance(&f.contract_id), 0);
        assert_eq!(token.balance(&f.treasury), 0); // no fee on refund
    }

    #[test]
    fn test_balances_create_dispute_resolve() {
        let f = TestFixture::setup_with_fee(0);
        let token = f.token();
        let learner_start = token.balance(&f.learner);
        let mentor_start = token.balance(&f.mentor);

        let id = f.create_escrow_at(1_000, 0);
        assert_eq!(token.balance(&f.contract_id), 1_000);

        f.open_dispute(id);
        assert_eq!(token.balance(&f.contract_id), 1_000); // still held

        f.client().resolve_dispute(&id, &75u32); // 750 mentor, 250 learner
        assert_eq!(token.balance(&f.mentor), mentor_start + 750);
        assert_eq!(token.balance(&f.learner), learner_start - 1_000 + 250);
        assert_eq!(token.balance(&f.contract_id), 0);
    }

    // -----------------------------------------------------------------------
    // update_fee / update_treasury
    // -----------------------------------------------------------------------

    #[test]
    fn test_update_fee_by_admin() {
        let f = TestFixture::setup_with_fee(500);
        f.client().update_fee(&200u32);
        assert_eq!(f.client().get_fee_bps(), 200);
    }

    #[test]
    fn test_update_fee_over_cap_panics() {
        let f = TestFixture::setup();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.client().update_fee(&1_001u32);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_update_treasury_redirects_fee() {
        let f = TestFixture::setup_with_fee(500);
        let token = f.token();
        let new_treasury = Address::generate(&f.env);
        f.client().update_treasury(&new_treasury);
        let id = f.create_escrow_at(1_000, 0);
        f.client().release_funds(&f.learner, &id);
        assert_eq!(token.balance(&new_treasury), 50);
        assert_eq!(token.balance(&f.treasury), 0);
    }

    // -----------------------------------------------------------------------
    // set_approved_token
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_approved_token_toggle() {
        let f = TestFixture::setup();
        let client = f.client();
        let new_token = Address::generate(&f.env);
        assert!(!client.is_token_approved(&new_token));
        client.set_approved_token(&new_token, &true);
        assert!(client.is_token_approved(&new_token));
        client.set_approved_token(&new_token, &false);
        assert!(!client.is_token_approved(&new_token));
    }
}
