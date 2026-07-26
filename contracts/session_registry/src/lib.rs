#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, String, Symbol, Vec};

// ── Storage keys ─────────────────────────────────────────────────────────────
const BACKEND: Symbol = symbol_short!("BACKEND");
const TTL_THRESHOLD: u32 = 500_000;
const TTL_BUMP: u32 = 1_000_000;

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


    pub fn update_session_metadata(env: Env, session_id: u64, tags: soroban_sdk::Vec<String>) {
        let key = (symbol_short!("SessMeta"), session_id);
        env.storage().persistent().set(&key, &tags);
    }
    
    pub fn get_sessions_by_participant(env: Env, participant: Address) -> soroban_sdk::Vec<u64> {
        soroban_sdk::Vec::new(&env)
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
            client.register_session(
                &sid,
                &mentor,
                &learner,
                &2_000_000u64,
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
}
