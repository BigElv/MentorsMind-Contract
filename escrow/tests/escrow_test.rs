#![cfg(test)]

use mentorminds_escrow::{EscrowContract, EscrowContractClient, EscrowStatus, MilestoneSpec, MilestoneStatus, MilestoneEscrow};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env, Vec, symbol_short, Symbol, BytesN,
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
    fn setup() -> Self { Self::setup_with_fee(500) }
    fn setup_with_fee(fee_bps: u32) -> Self { Self::setup_full(fee_bps, 0) }

    fn setup_full(fee_bps: u32, auto_release_delay_secs: u64) -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|li| li.timestamp = 14_400);

        let contract_id = env.register_contract(None, EscrowContract);
        let admin    = Address::generate(&env);
        let mentor   = Address::generate(&env);
        let learner  = Address::generate(&env);
        let treasury = Address::generate(&env);

        let (token_address, sac) = create_token(&env, &admin);
        sac.mint(&learner, &100_000);

        let client = EscrowContractClient::new(&env, &contract_id);
        let mut approved = Vec::new(&env);
        approved.push_back(token_address.clone());
        client.initialize(&admin, &treasury, &fee_bps, &approved, &auto_release_delay_secs);

        TestFixture { env, contract_id, admin, mentor, learner, treasury, token_address }
    }

    fn client(&self) -> EscrowContractClient { EscrowContractClient::new(&self.env, &self.contract_id) }
    fn token(&self)  -> TokenClient          { TokenClient::new(&self.env, &self.token_address) }
    fn sac(&self)    -> StellarAssetClient   { StellarAssetClient::new(&self.env, &self.token_address) }

    fn create_escrow_at(&self, amount: i128, session_end_time: u64, session_id: &str) -> u64 {
        self.client().create_escrow(
            &self.mentor, &self.learner, &amount,
            &Symbol::new(&self.env, session_id), &self.token_address, &session_end_time,
        )
    }

    fn open_dispute(&self, escrow_id: u64) {
        self.client().dispute(&self.learner, &escrow_id, &symbol_short!("NO_SHOW"));
    }
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

#[test]
fn test_session_id_uniqueness() {
    let f = TestFixture::setup();
    f.create_escrow_at(1_000, 0, "S1");
    
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.create_escrow_at(1_000, 0, "S1");
    }));
    assert!(result.is_err(), "Duplicate session_id must panic");
    
    // Different session_id should work
    f.create_escrow_at(1_000, 0, "S2");
}

#[test]
fn test_release_partial() {
    let f = TestFixture::setup_with_fee(500); // 5% fee
    let id = f.create_escrow_at(1_000, 0, "S1");
    
    let mentor_before = f.token().balance(&f.mentor);
    let treasury_before = f.token().balance(&f.treasury);
    
    // Release 400
    f.client().release_partial(&f.learner, &id, &400);
    
    // 400 * 0.05 = 20 fee, 380 net
    assert_eq!(f.token().balance(&f.mentor), mentor_before + 380);
    assert_eq!(f.token().balance(&f.treasury), treasury_before + 20);
    
    let e = f.client().get_escrow(&id);
    assert_eq!(e.amount, 600);
    assert_eq!(e.status, EscrowStatus::Active);
    assert_eq!(e.platform_fee, 20);
    assert_eq!(e.net_amount, 380);
    
    // Release remaining 600
    f.client().release_partial(&f.learner, &id, &600);
    
    // 600 * 0.05 = 30 fee, 570 net. Total: 50 fee, 950 net.
    assert_eq!(f.token().balance(&f.mentor), mentor_before + 950);
    assert_eq!(f.token().balance(&f.treasury), treasury_before + 50);
    
    let e2 = f.client().get_escrow(&id);
    assert_eq!(e2.amount, 0);
    assert_eq!(e2.status, EscrowStatus::Released);
    assert_eq!(e2.platform_fee, 50);
    assert_eq!(e2.net_amount, 950);
}

#[test]
fn test_resolve_dispute_to_mentor() {
    let f = TestFixture::setup_with_fee(500);
    let id = f.create_escrow_at(1_000, 0, "S1");
    f.open_dispute(id);
    
    let mentor_before = f.token().balance(&f.mentor);
    
    // Resolve to mentor (true)
    f.client().resolve_dispute(&id, &true);
    
    // Should behave like _do_release: 950 to mentor, 50 to treasury
    assert_eq!(f.token().balance(&f.mentor), mentor_before + 950);
    assert_eq!(f.token().balance(&f.treasury), 50);
    
    let e = f.client().get_escrow(&id);
    assert_eq!(e.status, EscrowStatus::Resolved);
    assert_eq!(e.net_amount, 950);
    assert_eq!(e.platform_fee, 50);
}

#[test]
fn test_resolve_dispute_to_learner() {
    let f = TestFixture::setup_with_fee(500);
    let id = f.create_escrow_at(1_000, 0, "S1");
    f.open_dispute(id);
    
    let learner_before = f.token().balance(&f.learner);
    
    // Resolve to learner (false)
    f.client().resolve_dispute(&id, &false);
    
    // Full refund, no fees
    assert_eq!(f.token().balance(&f.learner), learner_before + 1_000);
    
    let e = f.client().get_escrow(&id);
    assert_eq!(e.status, EscrowStatus::Resolved);
    assert_eq!(e.net_amount, 0);
    assert_eq!(e.platform_fee, 1_000); // repurposed for learner share
}

#[test]
fn test_admin_release() {
    let f = TestFixture::setup_with_fee(500);
    let id = f.create_escrow_at(1_000, 0, "S1");
    
    f.client().admin_release(&id);
    
    let e = f.client().get_escrow(&id);
    assert_eq!(e.status, EscrowStatus::Released);
    assert_eq!(f.token().balance(&f.mentor), 950);
}

#[test]
fn test_try_auto_release() {
    let f = TestFixture::setup_full(500, 3600);
    let now = f.env.ledger().timestamp();
    let id = f.create_escrow_at(1_000, now, "S1");
    
    advance_time(&f.env, 3600 + 1);
    f.client().try_auto_release(&id);
    
    let e = f.client().get_escrow(&id);
    assert_eq!(e.status, EscrowStatus::Released);
}

// -----------------------------------------------------------------------
// Milestone Tests
// -----------------------------------------------------------------------

#[test]
fn test_create_milestone_escrow() {
    let f = TestFixture::setup();
    
    let mut milestones = Vec::new(&f.env);
    milestones.push_back(MilestoneSpec {
        description_hash: BytesN::from_array(&f.env, &[1; 32]),
        amount: 1000,
    });
    milestones.push_back(MilestoneSpec {
        description_hash: BytesN::from_array(&f.env, &[2; 32]),
        amount: 2000,
    });
    milestones.push_back(MilestoneSpec {
        description_hash: BytesN::from_array(&f.env, &[3; 32]),
        amount: 1500,
    });
    
    let mentor_before = f.token().balance(&f.mentor);
    let learner_before = f.token().balance(&f.learner);
    let contract_before = f.token().balance(&f.client().contract_id);
    
    let escrow_id = f.client().create_milestone_escrow(
        &f.mentor,
        &f.learner,
        &milestones,
        &f.token_address,
    );
    
    // Verify total amount transferred (1000 + 2000 + 1500 = 4500)
    assert_eq!(f.token().balance(&f.learner), learner_before - 4500);
    assert_eq!(f.token().balance(&f.client().contract_id), contract_before + 4500);
    assert_eq!(f.token().balance(&f.mentor), mentor_before);
    
    let escrow = f.client().get_milestone_escrow(&escrow_id);
    assert_eq!(escrow.id, escrow_id);
    assert_eq!(escrow.mentor, f.mentor);
    assert_eq!(escrow.learner, f.learner);
    assert_eq!(escrow.total_amount, 4500);
    assert_eq!(escrow.milestones.len(), 3);
    assert_eq!(escrow.milestone_statuses.len(), 3);
    assert_eq!(escrow.status, EscrowStatus::Active);
    
    // All milestones should start as Pending
    for status in escrow.milestone_statuses.iter() {
        assert_eq!(status, MilestoneStatus::Pending);
    }
}

#[test]
fn test_create_milestone_escrow_validation() {
    let f = TestFixture::setup();
    
    // Test empty milestones
    let empty_milestones = Vec::new(&f.env);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client().create_milestone_escrow(&f.mentor, &f.learner, &empty_milestones, &f.token_address);
    }));
    assert!(result.is_err());
    
    // Test zero amount milestone
    let mut zero_milestones = Vec::new(&f.env);
    zero_milestones.push_back(MilestoneSpec {
        description_hash: BytesN::from_array(&f.env, &[1; 32]),
        amount: 0,
    });
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client().create_milestone_escrow(&f.mentor, &f.learner, &zero_milestones, &f.token_address);
    }));
    assert!(result.is_err());
}

#[test]
fn test_complete_all_milestones() {
    let f = TestFixture::setup_with_fee(500); // 5% fee
    
    let mut milestones = Vec::new(&f.env);
    milestones.push_back(MilestoneSpec {
        description_hash: BytesN::from_array(&f.env, &[1; 32]),
        amount: 1000,
    });
    milestones.push_back(MilestoneSpec {
        description_hash: BytesN::from_array(&f.env, &[2; 32]),
        amount: 2000,
    });
    milestones.push_back(MilestoneSpec {
        description_hash: BytesN::from_array(&f.env, &[3; 32]),
        amount: 1500,
    });
    
    let escrow_id = f.client().create_milestone_escrow(
        &f.mentor,
        &f.learner,
        &milestones,
        &f.token_address,
    );
    
    let mentor_before = f.token().balance(&f.mentor);
    let treasury_before = f.token().balance(&f.treasury);
    
    // Complete milestone 0 (1000 - 5% = 950 to mentor, 50 to treasury)
    f.client().complete_milestone(&escrow_id, &0);
    assert_eq!(f.token().balance(&f.mentor), mentor_before + 950);
    assert_eq!(f.token().balance(&f.treasury), treasury_before + 50);
    
    let escrow = f.client().get_milestone_escrow(&escrow_id);
    assert_eq!(escrow.milestone_statuses.get(0).unwrap(), &MilestoneStatus::Completed);
    assert_eq!(escrow.milestone_statuses.get(1).unwrap(), &MilestoneStatus::Pending);
    assert_eq!(escrow.milestone_statuses.get(2).unwrap(), &MilestoneStatus::Pending);
    assert_eq!(escrow.status, EscrowStatus::Active);
    
    // Complete milestone 1 (2000 - 5% = 1900 to mentor, 100 to treasury)
    f.client().complete_milestone(&escrow_id, &1);
    assert_eq!(f.token().balance(&f.mentor), mentor_before + 950 + 1900);
    assert_eq!(f.token().balance(&f.treasury), treasury_before + 50 + 100);
    
    // Complete milestone 2 (1500 - 5% = 1425 to mentor, 75 to treasury)
    f.client().complete_milestone(&escrow_id, &2);
    assert_eq!(f.token().balance(&f.mentor), mentor_before + 950 + 1900 + 1425);
    assert_eq!(f.token().balance(&f.treasury), treasury_before + 50 + 100 + 75);
    
    let final_escrow = f.client().get_milestone_escrow(&escrow_id);
    assert_eq!(final_escrow.status, EscrowStatus::Released);
    assert_eq!(final_escrow.platform_fee, 225); // 50 + 100 + 75
    assert_eq!(final_escrow.net_amount, 4275); // 950 + 1900 + 1425
    
    // All milestones should be completed
    for status in final_escrow.milestone_statuses.iter() {
        assert_eq!(status, MilestoneStatus::Completed);
    }
}

#[test]
fn test_dispute_milestone() {
    let f = TestFixture::setup();
    
    let mut milestones = Vec::new(&f.env);
    milestones.push_back(MilestoneSpec {
        description_hash: BytesN::from_array(&f.env, &[1; 32]),
        amount: 1000,
    });
    milestones.push_back(MilestoneSpec {
        description_hash: BytesN::from_array(&f.env, &[2; 32]),
        amount: 2000,
    });
    
    let escrow_id = f.client().create_milestone_escrow(
        &f.mentor,
        &f.learner,
        &milestones,
        &f.token_address,
    );
    
    // Complete first milestone
    f.client().complete_milestone(&escrow_id, &0);
    
    // Dispute second milestone
    f.client().dispute_milestone(&escrow_id, &1, &symbol_short!("QUALITY_ISSUE"));
    
    let escrow = f.client().get_milestone_escrow(&escrow_id);
    assert_eq!(escrow.milestone_statuses.get(0).unwrap(), &MilestoneStatus::Completed);
    assert_eq!(escrow.milestone_statuses.get(1).unwrap(), &MilestoneStatus::Disputed);
    assert_eq!(escrow.status, EscrowStatus::Disputed);
}

#[test]
fn test_milestone_validation() {
    let f = TestFixture::setup();
    
    let mut milestones = Vec::new(&f.env);
    milestones.push_back(MilestoneSpec {
        description_hash: BytesN::from_array(&f.env, &[1; 32]),
        amount: 1000,
    });
    
    let escrow_id = f.client().create_milestone_escrow(
        &f.mentor,
        &f.learner,
        &milestones,
        &f.token_address,
    );
    
    // Test invalid milestone index
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client().complete_milestone(&escrow_id, &5);
    }));
    assert!(result.is_err());
    
    // Test completing already completed milestone
    f.client().complete_milestone(&escrow_id, &0);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client().complete_milestone(&escrow_id, &0);
    }));
    assert!(result.is_err());
    
    // Test disputing completed milestone
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client().dispute_milestone(&escrow_id, &0, &symbol_short!("LATE"));
    }));
    assert!(result.is_err());
}

#[test]
fn test_milestone_escrow_count() {
    let f = TestFixture::setup();
    
    assert_eq!(f.client().get_milestone_escrow_count(), 0);
    
    let mut milestones = Vec::new(&f.env);
    milestones.push_back(MilestoneSpec {
        description_hash: BytesN::from_array(&f.env, &[1; 32]),
        amount: 1000,
    });
    
    f.client().create_milestone_escrow(&f.mentor, &f.learner, &milestones, &f.token_address);
    assert_eq!(f.client().get_milestone_escrow_count(), 1);
    
    f.client().create_milestone_escrow(&f.mentor, &f.learner, &milestones, &f.token_address);
    assert_eq!(f.client().get_milestone_escrow_count(), 2);
}
