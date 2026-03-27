#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, IntoVal, Symbol,
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
pub enum DataKey {
    Admin,
    EscrowContract,
    MentorRating(Address),
    MentorReviewCount(Address),
}

#[contract]
pub struct ReputationContract;

#[contractimpl]
impl ReputationContract {
    pub fn initialize(env: Env, admin: Address, escrow: Address) {
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::EscrowContract, &escrow);
    }

    pub fn add_review(env: Env, learner: Address, escrow_id: u64, rating: u32) {
        learner.require_auth();
        if rating < 1 || rating > 5 {
            panic!("rating must be between 1 and 5");
        }

        let escrow_addr: Address = env
            .storage()
            .persistent()
            .get(&DataKey::EscrowContract)
            .unwrap();

        // Call Escrow to get details
        let escrow: Escrow = env.invoke_contract(
            &escrow_addr,
            &Symbol::new(&env, "get_escrow"),
            (escrow_id,).into_val(&env),
        );

        if escrow.learner != learner {
            panic!("not your escrow");
        }

        if escrow.status != EscrowStatus::Released && escrow.status != EscrowStatus::Resolved {
            panic!("escrow not completed");
        }

        let mentor = escrow.mentor;
        let r_key = DataKey::MentorRating(mentor.clone());
        let c_key = DataKey::MentorReviewCount(mentor.clone());

        let current_total: u64 = env.storage().persistent().get(&r_key).unwrap_or(0);
        let current_count: u32 = env.storage().persistent().get(&c_key).unwrap_or(0);

        env.storage()
            .persistent()
            .set(&r_key, &(current_total + rating as u64));
        env.storage().persistent().set(&c_key, &(current_count + 1));

        env.events().publish(
            (symbol_short!("reput"), symbol_short!("reviewed"), mentor),
            (learner, rating),
        );
    }

    pub fn get_rating(env: Env, mentor: Address) -> u32 {
        let total: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::MentorRating(mentor.clone()))
            .unwrap_or(0);
        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::MentorReviewCount(mentor))
            .unwrap_or(0);
        if count == 0 {
            0
        } else {
            (total / count as u64) as u32
        }
    }
}
