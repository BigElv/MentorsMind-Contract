#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, String, Symbol, Vec};

// ── Storage keys ─────────────────────────────────────────────────────────────
const BACKEND: Symbol = symbol_short!("BACKEND");
const TTL_THRESHOLD: u32 = 500_000;
const TTL_BUMP: u32 = 1_000_000;
/// Schedule occupancy is tracked in 30-minute buckets.
const SLOT_SIZE_SECS: u64 = 1_800;
/// Minimum free time required between consecutive sessions on the same mentor.
const SCHEDULING_BUFFER_SECS: u64 = 900;

// ── Types ─────────────────────────────────────────────────────────────────────
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionStatus {
    Pending,
    Confirmed,
    Completed,
    Cancelled,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionRecord {
    pub session_id: Symbol,
    pub mentor: Address,
    pub learner: Address,
    pub scheduled_at: u64,
    pub duration_mins: u32,
    pub amount: i128,
    pub token: Address,
    pub status: SessionStatus,
    pub registered_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Session(Symbol),
    /// Deprecated: kept for backward compat, no longer written to
    MentorSessions(Address),
    /// Deprecated: kept for backward compat, no longer written to
    LearnerSessions(Address),
    MentorSessionCount(Address),
    MentorSessionAt(Address, u32),
    LearnerSessionCount(Address),
    LearnerSessionAt(Address, u32),
    SessionOracle,
    SessionMetadata(Symbol),
    /// Maps (mentor, time_bucket) → session_id occupying that 30-minute slot.
    MentorScheduleSlot(Address, u64),
}

// ── Errors ────────────────────────────────────────────────────────────────────
// Errors are surfaced via panics to keep compatibility with SDK 21 contractimpl.
// Error codes are documented here for reference:
// NotInitialized = 1, Unauthorized = 2, SessionNotFound = 3, DuplicateSession = 4

// ── Contract ──────────────────────────────────────────────────────────────────
#[contract]
pub struct SessionRegistry;

#[contractimpl]
impl SessionRegistry {
    /// Initialize with the platform backend address (only caller allowed to register/update).
    pub fn initialize(env: Env, backend: Address) {
        if env.storage().instance().has(&BACKEND) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&BACKEND, &backend);
        env.storage().instance().extend_ttl(TTL_THRESHOLD, TTL_BUMP);
    }

    /// Register a new session. Only callable by the platform backend.
    pub fn register_session(
        env: Env,
        session_id: Symbol,
        mentor: Address,
        learner: Address,
        scheduled_at: u64,
        duration_mins: u32,
        amount: i128,
        token: Address,
    ) -> Symbol {
        let backend = Self::require_backend(&env);
        backend.require_auth();

        let session_key = DataKey::Session(session_id.clone());
        if env.storage().persistent().has(&session_key) {
            panic!("Duplicate session");
        }

        // Occupied window includes the trailing scheduling buffer so the next
        // session cannot start within SCHEDULING_BUFFER_SECS of this one ending.
        let occupied_end = scheduled_at
            .saturating_add((duration_mins as u64).saturating_mul(60))
            .saturating_add(SCHEDULING_BUFFER_SECS);
        Self::assert_schedule_available(&env, &mentor, scheduled_at, occupied_end);
        Self::occupy_schedule_slots(&env, &mentor, &session_id, scheduled_at, occupied_end);

        let record = SessionRecord {
            session_id: session_id.clone(),
            mentor: mentor.clone(),
            learner: learner.clone(),
            scheduled_at,
            duration_mins,
            amount,
            token,
            status: SessionStatus::Pending,
            registered_at: env.ledger().timestamp(),
        };

        env.storage().persistent().set(&session_key, &record);
        env.storage()
            .persistent()
            .extend_ttl(&session_key, TTL_THRESHOLD, TTL_BUMP);

        // Index by mentor (indexed storage)
        let mentor_count_key = DataKey::MentorSessionCount(mentor.clone());
        let mentor_idx: u32 = env
            .storage()
            .persistent()
            .get(&mentor_count_key)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::MentorSessionAt(mentor.clone(), mentor_idx), &session_id.clone());
        env.storage()
            .persistent()
            .set(&mentor_count_key, &(mentor_idx + 1));

        // Index by learner (indexed storage)
        let learner_count_key = DataKey::LearnerSessionCount(learner.clone());
        let learner_idx: u32 = env
            .storage()
            .persistent()
            .get(&learner_count_key)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::LearnerSessionAt(learner.clone(), learner_idx), &session_id.clone());
        env.storage()
            .persistent()
            .set(&learner_count_key, &(learner_idx + 1));

        // Emit event
        env.events().publish(
            (
                symbol_short!("session"),
                Symbol::new(&env, "session_registered"),
                session_id.clone(),
            ),
            (mentor, learner, scheduled_at),
        );

        session_id
    }

    /// Update session status. Only callable by the platform backend.
    pub fn update_status(env: Env, session_id: Symbol, status: SessionStatus) {
        let backend = Self::require_backend(&env);
        backend.require_auth();

        let session_key = DataKey::Session(session_id.clone());
        let mut record: SessionRecord = env
            .storage()
            .persistent()
            .get(&session_key)
            .expect("Session not found");

        let old_status = record.status.clone();
        if matches!(status, SessionStatus::Cancelled)
            && !matches!(old_status, SessionStatus::Cancelled)
        {
            Self::release_schedule_slots(
                &env,
                &record.mentor,
                record.scheduled_at,
                record.duration_mins,
            );
        }
        record.status = status.clone();
        env.storage().persistent().set(&session_key, &record);
        env.storage()
            .persistent()
            .extend_ttl(&session_key, TTL_THRESHOLD, TTL_BUMP);

        env.events().publish(
            (
                symbol_short!("session"),
                Symbol::new(&env, "session_status_changed"),
                session_id,
            ),
            (old_status, status),
        );
    }

    /// Cancel a session and release its mentor schedule buckets for re-booking.
    pub fn cancel_session(env: Env, session_id: Symbol) {
        Self::update_status(env, session_id, SessionStatus::Cancelled);
    }

    /// Returns availability for each 30-minute slot in `[from, to)`.
    /// Each entry is `(slot_start, is_available)`.
    pub fn get_mentor_availability(
        env: Env,
        mentor: Address,
        from: u64,
        to: u64,
    ) -> Vec<(u64, bool)> {
        let mut result = Vec::new(&env);
        if to <= from {
            return result;
        }
        let mut bucket = from / SLOT_SIZE_SECS;
        let end_bucket = (to + SLOT_SIZE_SECS - 1) / SLOT_SIZE_SECS;
        while bucket < end_bucket {
            let slot_start = bucket * SLOT_SIZE_SECS;
            if slot_start >= to {
                break;
            }
            if slot_start >= from {
                let key = DataKey::MentorScheduleSlot(mentor.clone(), bucket);
                let is_available = !env.storage().persistent().has(&key);
                result.push_back((slot_start, is_available));
            }
            bucket = bucket.saturating_add(1);
        }
        result
    }

    pub fn set_session_oracle(env: Env, oracle: Address) {
        let backend = Self::require_backend(&env);
        backend.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::SessionOracle, &oracle);
    }

    pub fn update_status_from_oracle(
        env: Env,
        oracle: Address,
        session_id: Symbol,
        status: SessionStatus,
    ) {
        let configured_oracle: Address = env
            .storage()
            .instance()
            .get(&DataKey::SessionOracle)
            .expect("Session oracle not configured");
        oracle.require_auth();
        if oracle != configured_oracle {
            panic!("Unauthorized");
        }

        let session_key = DataKey::Session(session_id.clone());
        let mut record: SessionRecord = env
            .storage()
            .persistent()
            .get(&session_key)
            .expect("Session not found");

        let old_status = record.status.clone();
        if matches!(status, SessionStatus::Cancelled)
            && !matches!(old_status, SessionStatus::Cancelled)
        {
            Self::release_schedule_slots(
                &env,
                &record.mentor,
                record.scheduled_at,
                record.duration_mins,
            );
        }
        record.status = status.clone();
        env.storage().persistent().set(&session_key, &record);
        env.events().publish(
            (
                symbol_short!("session"),
                Symbol::new(&env, "session_oracle_status_changed"),
                session_id,
            ),
            (old_status, status),
        );
    }

    /// Get a session record by session_id.
    pub fn get_session(env: Env, session_id: Symbol) -> SessionRecord {
        env.storage()
            .persistent()
            .get(&DataKey::Session(session_id))
            .expect("Session not found")
    }

    /// Get paginated session IDs for a mentor.
    /// `offset` is the starting index, `limit` is the max items to return.
    pub fn get_sessions_by_mentor_page(
        env: Env,
        mentor: Address,
        offset: u32,
        limit: u32,
    ) -> Vec<Symbol> {
        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::MentorSessionCount(mentor.clone()))
            .unwrap_or(0);
        let mut result = Vec::new(&env);
        let start = offset.min(count);
        let end = (offset + limit).min(count);
        for i in start..end {
            let key = DataKey::MentorSessionAt(mentor.clone(), i);
            if let Some(sid) = env.storage().persistent().get::<_, Symbol>(&key) {
                result.push_back(sid);
            }
        }
        result
    }

    /// Get paginated session IDs for a learner.
    pub fn get_sessions_by_learner_page(
        env: Env,
        learner: Address,
        offset: u32,
        limit: u32,
    ) -> Vec<Symbol> {
        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::LearnerSessionCount(learner.clone()))
            .unwrap_or(0);
        let mut result = Vec::new(&env);
        let start = offset.min(count);
        let end = (offset + limit).min(count);
        for i in start..end {
            let key = DataKey::LearnerSessionAt(learner.clone(), i);
            if let Some(sid) = env.storage().persistent().get::<_, Symbol>(&key) {
                result.push_back(sid);
            }
        }
        result
    }

    /// Deprecated: returns first 50 sessions for a mentor.
    /// Use `get_sessions_by_mentor_page` for full paginated access.
    pub fn get_sessions_by_mentor(env: Env, mentor: Address) -> Vec<Symbol> {
        Self::get_sessions_by_mentor_page(env, mentor, 0, 50)
    }

    /// Deprecated: returns first 50 sessions for a learner.
    /// Use `get_sessions_by_learner_page` for full paginated access.
    pub fn get_sessions_by_learner(env: Env, learner: Address) -> Vec<Symbol> {
        Self::get_sessions_by_learner_page(env, learner, 0, 50)
    }

    /// Get total session count for a mentor.
    pub fn get_mentor_session_count(env: Env, mentor: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::MentorSessionCount(mentor))
            .unwrap_or(0)
    }

    /// Get total session count for a learner.
    pub fn get_learner_session_count(env: Env, learner: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::LearnerSessionCount(learner))
            .unwrap_or(0)
    }

    fn require_backend(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&BACKEND)
            .expect("Not initialized")
    }

    fn occupied_end(scheduled_at: u64, duration_mins: u32) -> u64 {
        scheduled_at
            .saturating_add((duration_mins as u64).saturating_mul(60))
            .saturating_add(SCHEDULING_BUFFER_SECS)
    }

    fn assert_schedule_available(env: &Env, mentor: &Address, start: u64, end: u64) {
        if end <= start {
            return;
        }
        let mut bucket = start / SLOT_SIZE_SECS;
        let last_bucket = (end - 1) / SLOT_SIZE_SECS;
        while bucket <= last_bucket {
            let key = DataKey::MentorScheduleSlot(mentor.clone(), bucket);
            if let Some(conflicting) = env.storage().persistent().get::<_, Symbol>(&key) {
                // Keep panic style consistent with the rest of this contract.
                let _ = conflicting;
                panic!("SessionConflict");
            }
            if bucket == u64::MAX {
                break;
            }
            bucket += 1;
        }
    }

    fn occupy_schedule_slots(
        env: &Env,
        mentor: &Address,
        session_id: &Symbol,
        start: u64,
        end: u64,
    ) {
        if end <= start {
            return;
        }
        let mut bucket = start / SLOT_SIZE_SECS;
        let last_bucket = (end - 1) / SLOT_SIZE_SECS;
        while bucket <= last_bucket {
            let key = DataKey::MentorScheduleSlot(mentor.clone(), bucket);
            env.storage().persistent().set(&key, session_id);
            env.storage()
                .persistent()
                .extend_ttl(&key, TTL_THRESHOLD, TTL_BUMP);
            if bucket == u64::MAX {
                break;
            }
            bucket += 1;
        }
    }

    fn release_schedule_slots(env: &Env, mentor: &Address, scheduled_at: u64, duration_mins: u32) {
        let end = Self::occupied_end(scheduled_at, duration_mins);
        if end <= scheduled_at {
            return;
        }
        let mut bucket = scheduled_at / SLOT_SIZE_SECS;
        let last_bucket = (end - 1) / SLOT_SIZE_SECS;
        while bucket <= last_bucket {
            let key = DataKey::MentorScheduleSlot(mentor.clone(), bucket);
            env.storage().persistent().remove(&key);
            if bucket == u64::MAX {
                break;
            }
            bucket += 1;
        }
    }

    pub fn update_session_metadata(env: Env, session_id: Symbol, tags: soroban_sdk::Vec<soroban_sdk::String>) {
        let key = DataKey::SessionMetadata(session_id);
        env.storage().persistent().set(&key, &tags);
        env.storage().persistent().extend_ttl(&key, TTL_THRESHOLD, TTL_BUMP);
    }

    pub fn get_session_metadata(env: Env, session_id: Symbol) -> soroban_sdk::Vec<soroban_sdk::String> {
        let key = DataKey::SessionMetadata(session_id);
        env.storage().persistent().get(&key).unwrap_or(Vec::new(&env))
    }
    
    pub fn get_sessions_by_participant(env: Env, participant: Address) -> soroban_sdk::Vec<Symbol> {
        let mut result = Vec::new(&env);
        let mentor_sessions: Vec<Symbol> = env
            .storage()
            .persistent()
            .get(&DataKey::MentorSessions(participant.clone()))
            .unwrap_or(Vec::new(&env));
        for s in mentor_sessions.iter() {
            result.push_back(s);
        }
        let learner_sessions: Vec<Symbol> = env
            .storage()
            .persistent()
            .get(&DataKey::LearnerSessions(participant.clone()))
            .unwrap_or(Vec::new(&env));
        for s in learner_sessions.iter() {
            if !result.contains(&s) {
                result.push_back(s);
            }
        }
        result
    }

}

// ── Tests ─────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Env,
    };

    fn setup() -> (Env, SessionRegistryClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|li| li.timestamp = 1_000_000);

        let contract_id = env.register_contract(None, SessionRegistry);
        let client = SessionRegistryClient::new(&env, &contract_id);
        let backend = Address::generate(&env);
        client.initialize(&backend);

        (env, client, backend)
    }

    fn dummy_token(env: &Env) -> Address {
        Address::generate(env)
    }

    #[test]
    fn test_register_session() {
        let (env, client, _backend) = setup();
        let mentor = Address::generate(&env);
        let learner = Address::generate(&env);
        let session_id = Symbol::new(&env, "sess1");

        let returned_id = client.register_session(
            &session_id,
            &mentor,
            &learner,
            &2_000_000u64,
            &60u32,
            &100i128,
            &dummy_token(&env),
        );
        assert_eq!(returned_id, session_id);

        let record = client.get_session(&session_id);
        assert_eq!(record.status, SessionStatus::Pending);
        assert_eq!(record.mentor, mentor);
        assert_eq!(record.learner, learner);
        assert_eq!(record.duration_mins, 60);
    }

    #[test]
    fn test_update_status_full_lifecycle() {
        let (env, client, _backend) = setup();
        let mentor = Address::generate(&env);
        let learner = Address::generate(&env);
        let session_id = Symbol::new(&env, "sess2");

        client.register_session(
            &session_id,
            &mentor,
            &learner,
            &2_000_000u64,
            &45u32,
            &200i128,
            &dummy_token(&env),
        );

        client.update_status(&session_id, &SessionStatus::Confirmed);
        assert_eq!(
            client.get_session(&session_id).status,
            SessionStatus::Confirmed
        );

        client.update_status(&session_id, &SessionStatus::Completed);
        assert_eq!(
            client.get_session(&session_id).status,
            SessionStatus::Completed
        );
    }

    #[test]
    fn test_get_sessions_by_mentor_and_learner() {
        let (env, client, _backend) = setup();
        let mentor = Address::generate(&env);
        let learner = Address::generate(&env);
        let token = dummy_token(&env);

        for i in 1u32..=3 {
            let sid = match i {
                1 => Symbol::new(&env, "s1"),
                2 => Symbol::new(&env, "s2"),
                _ => Symbol::new(&env, "s3"),
            };
            // Non-overlapping starts past the prior occupied buckets.
            // 60-min + 15-min buffer ending 2_004_500 occupies through bucket
            // ending at 2_005_200, so space sessions by 5_400s.
            let start = 2_000_000u64 + ((i as u64 - 1) * 5_400);
            client.register_session(
                &sid,
                &mentor,
                &learner,
                &start,
                &60u32,
                &100i128,
                &token,
            );
        }

        let mentor_sessions = client.get_sessions_by_mentor(&mentor);
        assert_eq!(mentor_sessions.len(), 3);

        let learner_sessions = client.get_sessions_by_learner(&learner);
        assert_eq!(learner_sessions.len(), 3);

        // Test paginated queries
        let page1 = client.get_sessions_by_mentor_page(&mentor, &0u32, &2u32);
        assert_eq!(page1.len(), 2);

        let page2 = client.get_sessions_by_mentor_page(&mentor, &2u32, &2u32);
        assert_eq!(page2.len(), 1);

        // Test count functions
        assert_eq!(client.get_mentor_session_count(&mentor), 3);
        assert_eq!(client.get_learner_session_count(&learner), 3);
    }

    #[test]
    fn test_cancel_session() {
        let (env, client, _backend) = setup();
        let mentor = Address::generate(&env);
        let learner = Address::generate(&env);
        let session_id = Symbol::new(&env, "sess_cancel");

        client.register_session(
            &session_id,
            &mentor,
            &learner,
            &2_000_000u64,
            &30u32,
            &50i128,
            &dummy_token(&env),
        );

        client.update_status(&session_id, &SessionStatus::Cancelled);
        assert_eq!(
            client.get_session(&session_id).status,
            SessionStatus::Cancelled
        );
    }

    #[test]
    #[should_panic(expected = "Duplicate session")]
    fn test_duplicate_session_rejected() {
        let (env, client, _backend) = setup();
        let mentor = Address::generate(&env);
        let learner = Address::generate(&env);
        let session_id = Symbol::new(&env, "sess_dup");
        let token = dummy_token(&env);

        client.register_session(
            &session_id,
            &mentor,
            &learner,
            &2_000_000u64,
            &60u32,
            &100i128,
            &token,
        );
        client.register_session(
            &session_id,
            &mentor,
            &learner,
            &2_000_000u64,
            &60u32,
            &100i128,
            &token,
        );
    }

    #[test]
    #[should_panic(expected = "SessionConflict")]
    fn test_overlapping_session_rejected() {
        let (env, client, _backend) = setup();
        let mentor = Address::generate(&env);
        let learner_a = Address::generate(&env);
        let learner_b = Address::generate(&env);
        let token = dummy_token(&env);

        client.register_session(
            &Symbol::new(&env, "sess_a"),
            &mentor,
            &learner_a,
            &2_000_000u64,
            &60u32,
            &100i128,
            &token,
        );
        // Starts during the first session window.
        client.register_session(
            &Symbol::new(&env, "sess_b"),
            &mentor,
            &learner_b,
            &2_001_800u64,
            &30u32,
            &100i128,
            &token,
        );
    }

    #[test]
    #[should_panic(expected = "SessionConflict")]
    fn test_scheduling_buffer_enforced() {
        let (env, client, _backend) = setup();
        let mentor = Address::generate(&env);
        let learner_a = Address::generate(&env);
        let learner_b = Address::generate(&env);
        let token = dummy_token(&env);

        // 60-minute session ending at 2_003_600; buffer occupies until 2_004_500.
        client.register_session(
            &Symbol::new(&env, "sess_a"),
            &mentor,
            &learner_a,
            &2_000_000u64,
            &60u32,
            &100i128,
            &token,
        );
        // Starts only 10 minutes after end — inside the 15-minute buffer.
        client.register_session(
            &Symbol::new(&env, "sess_b"),
            &mentor,
            &learner_b,
            &2_004_200u64,
            &30u32,
            &100i128,
            &token,
        );
    }

    #[test]
    fn test_non_overlapping_sessions_ok() {
        let (env, client, _backend) = setup();
        let mentor = Address::generate(&env);
        let learner_a = Address::generate(&env);
        let learner_b = Address::generate(&env);
        let token = dummy_token(&env);

        client.register_session(
            &Symbol::new(&env, "sess_a"),
            &mentor,
            &learner_a,
            &2_000_000u64,
            &60u32,
            &100i128,
            &token,
        );
        // First ends 2_003_600 + buffer 900 => 2_004_500; bucket occupancy
        // clears at the next slot boundary 2_005_200.
        client.register_session(
            &Symbol::new(&env, "sess_b"),
            &mentor,
            &learner_b,
            &2_005_200u64,
            &30u32,
            &100i128,
            &token,
        );
        assert_eq!(client.get_mentor_session_count(&mentor), 2);
    }

    #[test]
    fn test_cancel_releases_schedule_for_rebooking() {
        let (env, client, _backend) = setup();
        let mentor = Address::generate(&env);
        let learner_a = Address::generate(&env);
        let learner_b = Address::generate(&env);
        let token = dummy_token(&env);
        let first = Symbol::new(&env, "sess_a");

        client.register_session(
            &first,
            &mentor,
            &learner_a,
            &2_000_000u64,
            &60u32,
            &100i128,
            &token,
        );
        client.cancel_session(&first);
        assert_eq!(
            client.get_session(&first).status,
            SessionStatus::Cancelled
        );

        // Same slot can be booked again after cancel.
        client.register_session(
            &Symbol::new(&env, "sess_b"),
            &mentor,
            &learner_b,
            &2_000_000u64,
            &60u32,
            &100i128,
            &token,
        );
    }

    #[test]
    fn test_get_mentor_availability() {
        let (env, client, _backend) = setup();
        let mentor = Address::generate(&env);
        let learner = Address::generate(&env);

        client.register_session(
            &Symbol::new(&env, "sess_a"),
            &mentor,
            &learner,
            &2_000_000u64,
            &60u32,
            &100i128,
            &dummy_token(&env),
        );

        let availability =
            client.get_mentor_availability(&mentor, &1_999_800u64, &2_007_200u64);
        assert!(availability.len() > 0);

        let mut found_busy = false;
        let mut found_free = false;
        for entry in availability.iter() {
            let (slot_start, is_available) = entry;
            // Session occupies buckets covering [2_000_000, 2_004_500).
            if slot_start <= 2_000_000 && slot_start + SLOT_SIZE_SECS > 2_000_000 {
                assert!(!is_available);
                found_busy = true;
            }
            if slot_start >= 2_005_400 && is_available {
                found_free = true;
            }
        }
        assert!(found_busy);
        assert!(found_free);
    }
}
