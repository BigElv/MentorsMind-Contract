#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, BytesN, Env, IntoVal,
    Symbol, Vec,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EscrowStatus {
    Active,
    Released,
    Disputed,
    Refunded,
    Resolved,
}

use shared::StateMachine;

impl StateMachine for EscrowStatus {
    type State = Self;
    fn is_valid_transition(_env: &Env, from: &Self::State, to: &Self::State) -> bool {
        matches!(
            (from, to),
            (EscrowStatus::Active, EscrowStatus::Released)
                | (EscrowStatus::Active, EscrowStatus::Disputed)
                | (EscrowStatus::Active, EscrowStatus::Refunded)
                | (EscrowStatus::Disputed, EscrowStatus::Resolved)
                | (EscrowStatus::Disputed, EscrowStatus::Refunded)
        )
    }
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Treasury,
    FeeBps,
    AutoRelDelay,
    EscrowCount,
    MilestoneEscrowCount,
    Escrow(u64),
    MilestoneEscrow(u64),
    ApprovedToken(Address),
    MentorEscrows(Address),
    LearnerEscrows(Address),
    StatusEscrows(EscrowStatus),
    Session(Symbol),
    KycRegistry,
    SanctionsRegistry,
    VelocityLimits,
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
pub struct EscrowParams {
    pub mentor: Address,
    pub learner: Address,
    pub amount: i128,
    pub session_id: Symbol,
    pub token_address: Address,
    pub session_end_time: u64,
    pub total_sessions: u32,
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
const KYC_REGISTRY: Symbol = symbol_short!("KYC_REG");
const SANCTIONS: Symbol = symbol_short!("SANCTION");
const MAX_FEE_BPS: u32 = 1_000;
const DEFAULT_AUTO_RELEASE_DELAY: u64 = 72 * 60 * 60;
const EXTEND_TTL_THRESHOLD: u32 = 500_000;
const EXTEND_TTL_BUMP: u32 = 1_000_000;

#[contract]
pub struct EscrowContract;

#[soroban_sdk::contractclient(name = "KycRegistryClient")]
pub trait KycRegistryTrait {
    fn is_kyc_valid(env: Env, user: Address, min_level: KycLevel) -> bool;
}

#[soroban_sdk::contractclient(name = "SanctionsClient")]
pub trait SanctionsTrait {
    fn is_sanctioned(env: Env, address: Address) -> bool;
}

#[contractimpl]
impl EscrowContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        treasury: Address,
        fee_bps: u32,
        approved_tokens: Vec<Address>,
        auto_release_delay_secs: u64,
        sanctions_contract: Option<Address>,
    ) {
        if env.storage().persistent().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        if fee_bps > MAX_FEE_BPS {
            panic!("Fee > 1000 bps");
        }
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::Treasury, &treasury);
        env.storage().persistent().set(&DataKey::FeeBps, &fee_bps);
        env.storage().persistent().set(&DataKey::EscrowCount, &0u64);
        env.storage()
            .persistent()
            .set(&DataKey::MilestoneEscrowCount, &0u64);
        let delay = if auto_release_delay_secs == 0 {
            DEFAULT_AUTO_RELEASE_DELAY
        } else {
            auto_release_delay_secs
        };
        env.storage()
            .persistent()
            .set(&DataKey::AutoRelDelay, &delay);
        for token_addr in approved_tokens.iter() {
            Self::_set_token_approved(&env, &token_addr, true);
        }

        if let Some(sc) = sanctions_contract {
            env.storage().persistent().set(&SANCTIONS, &sc);
        }
    }

    pub fn update_fee(env: Env, new_fee_bps: u32) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("Init required");
        admin.require_auth();
        if new_fee_bps > MAX_FEE_BPS {
            panic!("Fee > 1000 bps");
        }
        env.storage()
            .persistent()
            .set(&DataKey::FeeBps, &new_fee_bps);
    }

    pub fn update_treasury(env: Env, new_treasury: Address) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("Init required");
        admin.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::Treasury, &new_treasury);
    }

    pub fn set_approved_token(env: Env, token_address: Address, approved: bool) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("Init required");
        admin.require_auth();
        Self::_set_token_approved(&env, &token_address, approved);
    }

    pub fn set_compliance_contracts(env: Env, kyc: Address, sanctions: Address, velocity: Address) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("Init required");
        admin.require_auth();
        env.storage().persistent().set(&DataKey::KycRegistry, &kyc);
        env.storage()
            .persistent()
            .set(&DataKey::SanctionsRegistry, &sanctions);
        env.storage()
            .persistent()
            .set(&DataKey::VelocityLimits, &velocity);
    }

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
        // Sanctions screening
        if let Some(sc_addr) = env.storage().persistent().get::<Symbol, Address>(&SANCTIONS) {
            let sc = SanctionsClient::new(&env, &sc_addr);
            if sc.is_sanctioned(&mentor) {
                panic!("mentor is sanctioned");
            }
            if sc.is_sanctioned(&learner) {
                panic!("learner is sanctioned");
            }
        }
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
            session_id,
            token_address.clone(),
            session_end_time,
            0,
            amount,
            token_address.clone(),
            token_address,
            total_sessions,
        )
    }

    pub fn release_funds(env: Env, caller: Address, escrow_id: u64) {
        let key = DataKey::Escrow(escrow_id);
        let mut e: Escrow = env.storage().persistent().get(&key).expect("Not found");
        if !EscrowStatus::is_valid_transition(&env, &e.status, &EscrowStatus::Released) {
            panic!("Invalid transition");
        }
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("Admin not set");
        caller.require_auth();
        if caller != e.learner && caller != admin {
            panic!("Not authorized");
        }
        Self::_do_release(&env, &mut e, &key);
    }

    pub fn admin_release(env: Env, escrow_id: u64) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("Admin not set");
        admin.require_auth();
        let key = DataKey::Escrow(escrow_id);
        let mut e: Escrow = env.storage().persistent().get(&key).expect("Not found");
        if e.status != EscrowStatus::Active {
            panic!("Escrow not active");
        }
        Self::_do_release(&env, &mut e, &key);
    }

    pub fn release_partial(env: Env, caller: Address, escrow_id: u64) {
        let key = DataKey::Escrow(escrow_id);
        let mut e: Escrow = env.storage().persistent().get(&key).expect("Not found");
        if e.status != EscrowStatus::Active {
            panic!("Escrow not active");
        }
        if e.sessions_completed >= e.total_sessions {
            panic!("Completed");
        }
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("Admin not set");
        caller.require_auth();
        if caller != e.learner && caller != admin {
            panic!("Not authorized");
        }
        let amt = if e.sessions_completed + 1 == e.total_sessions {
            e.amount
        } else {
            e.quoted_token_amount / e.total_sessions as i128
        };
        let fee_bps = Self::get_fee_bps(env.clone());
        let fee = amt * fee_bps as i128 / 10_000;
        let net = amt - fee;
        let tc = token::Client::new(&env, &e.token_address);
        let treasury = Self::get_treasury(env.clone());
        if fee > 0 {
            tc.transfer(&env.current_contract_address(), &treasury, &fee);
        }
        tc.transfer(&env.current_contract_address(), &e.mentor, &net);
        e.sessions_completed += 1;
        e.amount -= amt;
        e.platform_fee += fee;
        e.net_amount += net;
        if e.sessions_completed == e.total_sessions {
            e.status = EscrowStatus::Released;
        }
        env.storage().persistent().set(&key, &e);
        env.events().publish(
            (symbol_short!("partial"), escrow_id),
            (e.sessions_completed, amt),
        );
    }

    pub fn try_auto_release(env: Env, escrow_id: u64) {
        let key = DataKey::Escrow(escrow_id);
        let mut e: Escrow = env.storage().persistent().get(&key).expect("Not found");
        if e.status != EscrowStatus::Active {
            panic!("Escrow not active");
        }
        if env.ledger().timestamp() < e.session_end_time + e.auto_release_delay {
            panic!("Window not elapsed");
        }
        Self::_do_release(&env, &mut e, &key);
    }

    pub fn dispute(env: Env, caller: Address, escrow_id: u64, reason: Symbol) {
        let key = DataKey::Escrow(escrow_id);
        let mut e: Escrow = env.storage().persistent().get(&key).expect("Not found");
        if !EscrowStatus::is_valid_transition(&env, &e.status, &EscrowStatus::Disputed) {
            panic!("Invalid transition");
        }
        caller.require_auth();
        if caller != e.mentor && caller != e.learner {
            panic!("Unauthorized");
        }
        let old_status = e.status.clone();
        e.status = EscrowStatus::Disputed;
        e.dispute_reason = reason.clone();
        env.storage().persistent().set(&key, &e);
        Self::_update_status_index(&env, e.id, &old_status, &EscrowStatus::Disputed);
        env.events().publish(
            (
                symbol_short!("Escrow"),
                symbol_short!("disputed"),
                escrow_id,
            ),
            reason,
        );
    }

    pub fn resolve_dispute(env: Env, escrow_id: u64, mentor_win: bool) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("Admin not set");
        admin.require_auth();
        let key = DataKey::Escrow(escrow_id);
        let mut e: Escrow = env.storage().persistent().get(&key).expect("Not found");
        if !EscrowStatus::is_valid_transition(&env, &e.status, &EscrowStatus::Resolved) {
            panic!("Invalid transition");
        }
        let old_status = e.status.clone();
        if mentor_win {
            Self::_do_release(&env, &mut e, &key);
        } else {
            token::Client::new(&env, &e.token_address).transfer(
                &env.current_contract_address(),
                &e.learner,
                &e.amount,
            );
            e.platform_fee = e.amount;
            e.amount = 0;
        }
        e.status = EscrowStatus::Resolved;
        e.resolved_at = env.ledger().timestamp();
        env.storage().persistent().set(&key, &e);
        Self::_update_status_index(&env, e.id, &old_status, &EscrowStatus::Resolved);
    }

    pub fn refund(env: Env, escrow_id: u64) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("Admin not set");
        admin.require_auth();
        let key = DataKey::Escrow(escrow_id);
        let mut e: Escrow = env.storage().persistent().get(&key).expect("Not found");
        if !EscrowStatus::is_valid_transition(&env, &e.status, &EscrowStatus::Refunded) {
            panic!("Invalid transition");
        }
        let old_status = e.status.clone();
        token::Client::new(&env, &e.token_address).transfer(
            &env.current_contract_address(),
            &e.learner,
            &e.amount,
        );
        e.status = EscrowStatus::Refunded;
        e.amount = 0;
        env.storage().persistent().set(&key, &e);
        Self::_update_status_index(&env, e.id, &old_status, &EscrowStatus::Refunded);
    }

    pub fn create_milestone_escrow(
        env: Env,
        mentor: Address,
        learner: Address,
        milestones: Vec<MilestoneSpec>,
        token_address: Address,
    ) -> u64 {
        if milestones.is_empty() {
            panic!("Empty milestones");
        }
        if !Self::_is_token_approved(&env, &token_address) {
            panic!("Token not approved");
        }
        let total = milestones.iter().fold(0i128, |acc, m| acc + m.amount);
        learner.require_auth();
        token::Client::new(&env, &token_address).transfer(
            &learner,
            &env.current_contract_address(),
            &total,
        );
        let mut count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::MilestoneEscrowCount)
            .unwrap_or(0);
        count += 1;
        env.storage()
            .persistent()
            .set(&DataKey::MilestoneEscrowCount, &count);
        let mut statuses = Vec::new(&env);
        for _ in 0..milestones.len() {
            statuses.push_back(MilestoneStatus::Pending);
        }
        let me = MilestoneEscrow {
            id: count,
            mentor,
            learner,
            total_amount: total,
            milestones,
            milestone_statuses: statuses,
            status: EscrowStatus::Active,
            created_at: env.ledger().timestamp(),
            token_address,
            platform_fee: 0,
            net_amount: 0,
        };
        env.storage()
            .persistent()
            .set(&DataKey::MilestoneEscrow(count), &me);
        count
    }

    pub fn complete_milestone(env: Env, escrow_id: u64, milestone_index: u32) {
        let key = DataKey::MilestoneEscrow(escrow_id);
        let mut me: MilestoneEscrow = env.storage().persistent().get(&key).expect("Not found");
        me.learner.require_auth();
        let m = me.milestones.get(milestone_index).expect("Invalid index");
        let fee = m.amount * Self::get_fee_bps(env.clone()) as i128 / 10_000;
        let net = m.amount - fee;
        let tc = token::Client::new(&env, &me.token_address);
        let tr = Self::get_treasury(env.clone());
        if fee > 0 {
            tc.transfer(&env.current_contract_address(), &tr, &fee);
        }
        tc.transfer(&env.current_contract_address(), &me.mentor, &net);
        me.milestone_statuses
            .set(milestone_index, MilestoneStatus::Completed);
        if me
            .milestone_statuses
            .iter()
            .all(|s| s == MilestoneStatus::Completed)
        {
            me.status = EscrowStatus::Released;
        }
        env.storage().persistent().set(&key, &me);
    }

    pub fn get_escrow(env: Env, id: u64) -> Escrow {
        env.storage()
            .persistent()
            .get(&DataKey::Escrow(id))
            .expect("Not found")
    }
    pub fn get_escrow_count(env: Env) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::EscrowCount)
            .unwrap_or(0)
    }
    pub fn get_auto_release_delay(env: Env) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::AutoRelDelay)
            .unwrap_or(DEFAULT_AUTO_RELEASE_DELAY)
    }
    pub fn get_fee_bps(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::FeeBps)
            .unwrap_or(0)
    }
    pub fn get_treasury(env: Env) -> Address {
        env.storage()
            .persistent()
            .get(&DataKey::Treasury)
            .expect("Not set")
    }
    pub fn is_token_approved(env: Env, t: Address) -> bool {
        Self::_is_token_approved(&env, &t)
    }
    pub fn get_milestone_escrow(env: Env, id: u64) -> MilestoneEscrow {
        env.storage()
            .persistent()
            .get(&DataKey::MilestoneEscrow(id))
            .expect("Not found")
    }
    pub fn get_escrows_by_mentor(
        env: Env,
        mentor: Address,
        page: u32,
        page_size: u32,
    ) -> Vec<Escrow> {
        let ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::MentorEscrows(mentor))
            .unwrap_or(Vec::new(&env));
        let size = page_size.min(50);
        let start = page * size;
        let mut res = Vec::new(&env);
        for i in start..(start + size).min(ids.len()) {
            if let Some(id) = ids.get(i) {
                if let Some(e) = env.storage().persistent().get(&DataKey::Escrow(id)) {
                    res.push_back(e);
                }
            }
        }
        res
    }
    pub fn get_escrows_by_learner(
        env: Env,
        learner: Address,
        page: u32,
        page_size: u32,
    ) -> Vec<Escrow> {
        let ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::LearnerEscrows(learner))
            .unwrap_or(Vec::new(&env));
        let size = page_size.min(50);
        let start = page * size;
        let mut res = Vec::new(&env);
        for i in start..(start + size).min(ids.len()) {
            if let Some(id) = ids.get(i) {
                if let Some(e) = env.storage().persistent().get(&DataKey::Escrow(id)) {
                    res.push_back(e);
                }
            }
        }
        res
    }
    pub fn get_escrows_by_status(env: Env, status: EscrowStatus) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::StatusEscrows(status))
            .unwrap_or(Vec::new(&env))
    }

    #[allow(clippy::too_many_arguments)]
    fn _create_escrow_internal(
        env: &Env,
        mentor: Address,
        learner: Address,
        amount: i128,
        session_id: Symbol,
        token_address: Address,
        session_end_time: u64,
        usd_amount: i128,
        quoted_token_amount: i128,
        _send_asset: Address,
        _dest_asset: Address,
        total_sessions: u32,
    ) -> u64 {
        if amount <= 0 {
            panic!("Amount <= 0");
        }
        if !Self::_is_token_approved(env, &token_address) {
            panic!("Token not approved");
        }
        if env
            .storage()
            .persistent()
            .has(&DataKey::Session(session_id.clone()))
        {
            panic!("Duplicate session_id");
        }

        // --- COMPLIANCE CHECKS ---
        if let Some(kyc_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::KycRegistry)
        {
            let is_approved: bool = env.invoke_contract(
                &kyc_addr,
                &symbol_short!("is_kyc"),
                (learner.clone(),).into_val(env),
            );
            if !is_approved {
                panic!("KYC required");
            }
        }
        if let Some(sanc_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::SanctionsRegistry)
        {
            let is_sanctioned: bool = env.invoke_contract(
                &sanc_addr,
                &Symbol::new(env, "is_sanctioned"),
                (learner.clone(),).into_val(env),
            );
            if is_sanctioned {
                panic!("Sanctioned");
            }
        }
        if let Some(vel_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::VelocityLimits)
        {
            let ok: bool = env.invoke_contract(
                &vel_addr,
                &Symbol::new(env, "check_and_record"),
                (learner.clone(), amount).into_val(env),
            );
            if !ok {
                panic!("Velocity limit exceeded");
            }
        }
        // -------------------------
        learner.require_auth();
        token::Client::new(env, &token_address).transfer(
            &learner,
            &env.current_contract_address(),
            &amount,
        );
        let mut count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::EscrowCount)
            .unwrap_or(0);
        count += 1;
        env.storage()
            .persistent()
            .set(&DataKey::EscrowCount, &count);
        let delay = env
            .storage()
            .persistent()
            .get(&DataKey::AutoRelDelay)
            .unwrap_or(DEFAULT_AUTO_RELEASE_DELAY);
        let e = Escrow {
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
            auto_release_delay: delay,
            dispute_reason: Symbol::new(env, ""),
            resolved_at: 0,
            usd_amount,
            quoted_token_amount,
            send_asset: token_address.clone(),
            dest_asset: token_address.clone(),
            total_sessions,
            sessions_completed: 0,
        };
        env.storage().persistent().set(&DataKey::Escrow(count), &e);
        env.storage()
            .persistent()
            .set(&DataKey::Session(session_id.clone()), &count);
        let mut m_escrows: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::MentorEscrows(mentor.clone()))
            .unwrap_or(Vec::new(env));
        m_escrows.push_back(count);
        env.storage()
            .persistent()
            .set(&DataKey::MentorEscrows(mentor.clone()), &m_escrows);
        let mut l_escrows: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::LearnerEscrows(learner.clone()))
            .unwrap_or(Vec::new(env));
        l_escrows.push_back(count);
        env.storage()
            .persistent()
            .set(&DataKey::LearnerEscrows(learner.clone()), &l_escrows);
        let mut s_escrows: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::StatusEscrows(EscrowStatus::Active))
            .unwrap_or(Vec::new(env));
        s_escrows.push_back(count);
        env.storage()
            .persistent()
            .set(&DataKey::StatusEscrows(EscrowStatus::Active), &s_escrows);
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

    fn _do_release(env: &Env, e: &mut Escrow, key: &DataKey) {
        let old_status = e.status.clone();
        let fee = e.amount
            * env
                .storage()
                .persistent()
                .get(&DataKey::FeeBps)
                .unwrap_or(0u32) as i128
            / 10_000;
        let net = e.amount - fee;
        let tc = token::Client::new(env, &e.token_address);
        let tr = env
            .storage()
            .persistent()
            .get(&DataKey::Treasury)
            .expect("Treasury not set");
        if fee > 0 {
            tc.transfer(&env.current_contract_address(), &tr, &fee);
        }
        tc.transfer(&env.current_contract_address(), &e.mentor, &net);
        e.status = EscrowStatus::Released;
        e.platform_fee += fee;
        e.net_amount += net;
        e.amount = 0;
        env.storage().persistent().set(key, e);
        Self::_update_status_index(env, e.id, &old_status, &EscrowStatus::Released);
        env.events().publish(
            (symbol_short!("Escrow"), symbol_short!("Released"), e.id),
            EscrowReleasedEventData {
                mentor: e.mentor.clone(),
                amount: fee + net,
                net_amount: net,
                platform_fee: fee,
                token_address: e.token_address.clone(),
            },
        );
    }

    fn _update_status_index(env: &Env, id: u64, from: &EscrowStatus, to: &EscrowStatus) {
        let mut f_vec: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::StatusEscrows(from.clone()))
            .unwrap_or(Vec::new(env));
        if let Some(pos) = f_vec.iter().position(|x| x == id) {
            f_vec.remove(pos as u32);
        }
        env.storage()
            .persistent()
            .set(&DataKey::StatusEscrows(from.clone()), &f_vec);
        let mut t_vec: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::StatusEscrows(to.clone()))
            .unwrap_or(Vec::new(env));
        t_vec.push_back(id);
        env.storage()
            .persistent()
            .set(&DataKey::StatusEscrows(to.clone()), &t_vec);
    }

    fn _set_token_approved(env: &Env, tok: &Address, approved: bool) {
        let key = DataKey::ApprovedToken(tok.clone());
        env.storage().persistent().set(&key, &approved);
        env.storage()
            .persistent()
            .extend_ttl(&key, EXTEND_TTL_THRESHOLD, EXTEND_TTL_BUMP);
    }
    fn _is_token_approved(env: &Env, tok: &Address) -> bool {
        env.storage()
            .persistent()
            .get::<_, bool>(&DataKey::ApprovedToken(tok.clone()))
            .unwrap_or(false)
    }
}
