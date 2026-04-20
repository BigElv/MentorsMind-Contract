#![no_std]

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, Address, Env, Symbol, Vec,
};

const DEFAULT_WINDOW_SECS: u64 = 48 * 60 * 60;
const MAX_EVIDENCE_ITEMS: u32 = 5;

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
pub struct EvidenceItem {
    pub submitter: Address,
    pub evidence_ref: Symbol,
    pub submitted_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    EscrowContract,
    Evidence(u64),
    WindowSecs,
}

#[contractclient(name = "EscrowContractClient")]
pub trait EscrowContractTrait {
    fn get_escrow(env: Env, escrow_id: u64) -> Escrow;
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    Unauthorized = 2,
    InvalidEscrowState = 3,
    EvidenceWindowClosed = 4,
    EvidenceLimitReached = 5,
}

#[contract]
pub struct DisputeEvidenceContract;

#[contractimpl]
impl DisputeEvidenceContract {
    pub fn initialize(env: Env, admin: Address, escrow_contract: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::EscrowContract, &escrow_contract);
        env.storage()
            .instance()
            .set(&DataKey::WindowSecs, &DEFAULT_WINDOW_SECS);
        Ok(())
    }

    pub fn set_escrow_contract(
        env: Env,
        admin: Address,
        escrow_contract: Address,
    ) -> Result<(), Error> {
        Self::require_admin(&env, &admin)?;
        env.storage()
            .instance()
            .set(&DataKey::EscrowContract, &escrow_contract);
        Ok(())
    }

    pub fn submit_evidence(
        env: Env,
        escrow_id: u64,
        submitter: Address,
        evidence_ref: Symbol,
    ) -> Result<(), Error> {
        submitter.require_auth();
        let escrow = Self::load_escrow(&env, escrow_id);
        if escrow.status != EscrowStatus::Disputed {
            return Err(Error::InvalidEscrowState);
        }
        if submitter != escrow.mentor && submitter != escrow.learner {
            return Err(Error::Unauthorized);
        }

        let window_secs: u64 = env
            .storage()
            .instance()
            .get(&DataKey::WindowSecs)
            .unwrap_or(DEFAULT_WINDOW_SECS);
        if env.ledger().timestamp() > escrow.session_end_time.saturating_add(window_secs) {
            return Err(Error::EvidenceWindowClosed);
        }

        let key = DataKey::Evidence(escrow_id);
        let mut evidence: Vec<EvidenceItem> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(&env));
        if evidence.len() >= MAX_EVIDENCE_ITEMS {
            return Err(Error::EvidenceLimitReached);
        }

        let item = EvidenceItem {
            submitter: submitter.clone(),
            evidence_ref: evidence_ref.clone(),
            submitted_at: env.ledger().timestamp(),
        };
        evidence.push_back(item.clone());
        env.storage().persistent().set(&key, &evidence);
        env.events()
            .publish((Symbol::new(&env, "evidence_submitted"), escrow_id), item);
        Ok(())
    }

    pub fn get_evidence(env: Env, escrow_id: u64) -> Vec<EvidenceItem> {
        env.storage()
            .persistent()
            .get(&DataKey::Evidence(escrow_id))
            .unwrap_or(Vec::new(&env))
    }

    pub fn get_evidence_count(env: Env, escrow_id: u64) -> u32 {
        Self::get_evidence(env, escrow_id).len()
    }

    fn require_admin(env: &Env, admin: &Address) -> Result<(), Error> {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        if stored_admin != *admin {
            return Err(Error::Unauthorized);
        }
        Ok(())
    }

    fn load_escrow(env: &Env, escrow_id: u64) -> Escrow {
        let escrow_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::EscrowContract)
            .expect("escrow contract not configured");
        EscrowContractClient::new(env, &escrow_contract).get_escrow(&escrow_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{contractimpl, testutils::Address as _};

    #[contract]
    struct MockEscrow;

    #[contractimpl]
    impl MockEscrow {
        pub fn get_escrow(env: Env, escrow_id: u64) -> Escrow {
            Escrow {
                id: escrow_id,
                mentor: Address::generate(&env),
                learner: Address::generate(&env),
                amount: 100,
                session_id: Symbol::new(&env, "sess"),
                status: EscrowStatus::Disputed,
                created_at: env.ledger().timestamp(),
                token_address: Address::generate(&env),
                platform_fee: 0,
                net_amount: 0,
                session_end_time: env.ledger().timestamp(),
                auto_release_delay: 0,
                dispute_reason: Symbol::new(&env, "late"),
                resolved_at: 0,
                usd_amount: 0,
                quoted_token_amount: 100,
                send_asset: Address::generate(&env),
                dest_asset: Address::generate(&env),
                total_sessions: 1,
                sessions_completed: 0,
            }
        }
    }

    #[test]
    fn stores_evidence_until_cap() {
        let env = Env::default();
        env.mock_all_auths();

        let escrow_contract = env.register_contract(None, MockEscrow);
        let contract_id = env.register_contract(None, DisputeEvidenceContract);
        let client = DisputeEvidenceContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        client.initialize(&admin, &escrow_contract);
        let escrow = EscrowContractClient::new(&env, &escrow_contract).get_escrow(&1);

        for evidence in ["e1", "e2", "e3", "e4", "e5"] {
            client.submit_evidence(&1, &escrow.mentor, &Symbol::new(&env, evidence));
        }

        assert_eq!(client.get_evidence_count(&1), MAX_EVIDENCE_ITEMS);
    }
}
