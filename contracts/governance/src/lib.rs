#![no_std]

use shared::StateMachine;
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, vec, Address, Bytes, BytesN, Env, IntoVal,
    Symbol, Vec,
};

// Instance storage: frequently read config
const ADMIN: Symbol = symbol_short!("ADMIN");
const TOKEN: Symbol = symbol_short!("TOKEN");
const SNAPSHOT: Symbol = symbol_short!("SNAPSHOT");
const PROPOSAL_COUNT: Symbol = symbol_short!("PROP_CNT");
const VOTING_PERIOD_SECS: Symbol = symbol_short!("VOT_PER");
const QUORUM_BPS: Symbol = symbol_short!("QRM_BPS");
const CURRENT_FEE_BPS: Symbol = symbol_short!("FEE_BPS");
const CURRENT_AUTO_RELEASE_SECS: Symbol = symbol_short!("AUTO_REL");
const TEMPLATES: Symbol = symbol_short!("TMPLATES");

const DEFAULT_VOTING_PERIOD_SECS: u64 = 7 * 24 * 60 * 60;
const DEFAULT_QUORUM_BPS: u32 = 1_000; // 10%
const CUSTOM_PROPOSAL_QUORUM_BPS: u32 = 3_000; // 30%

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProposalAction {
    UpdateFee(u32),
    UpdateAutoRelease(u64),
    AddAsset(Address),
    UpdateAdmin(Address),
    ExecuteCall(Address, Symbol, Vec<u64>),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProposalStatus {
    Active,
    Passed,
    Failed,
    Executed,
    Cancelled,
}

impl StateMachine for ProposalStatus {
    type State = ProposalStatus;

    fn is_valid_transition(_env: &Env, from: &Self::State, to: &Self::State) -> bool {
        matches!(
            (from, to),
            (ProposalStatus::Active, ProposalStatus::Passed)
                | (ProposalStatus::Active, ProposalStatus::Failed)
                | (ProposalStatus::Active, ProposalStatus::Cancelled)
                | (ProposalStatus::Passed, ProposalStatus::Executed)
        )
    }
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Proposal {
    pub id: u32,
    pub proposer: Address,
    pub title: Bytes,
    pub description_hash: BytesN<32>,
    pub action: ProposalAction,
    pub status: ProposalStatus,
    pub created_at: u64,
    pub voting_ends_at: u64,
    pub snapshot_ledger: u32,
    pub total_supply_snapshot: i128,
    pub votes_for: i128,
    pub votes_against: i128,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Proposal(u32),
    Vote(u32, Address),
    VoteWeight(u32, Address),
    ApprovedAsset(Address),
    Timelock,
    CustomProposal(u32),
}

#[contract]
pub struct GovernanceContract;

#[contractimpl]
impl GovernanceContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        mnt_token: Address,
        snapshot_contract: Address,
        voting_period_secs: Option<u64>,
        quorum_bps: Option<u32>,
    ) {
        if env.storage().instance().has(&ADMIN) {
            panic!("already initialized");
        }

        let period = voting_period_secs.unwrap_or(DEFAULT_VOTING_PERIOD_SECS);
        if period == 0 {
            panic!("invalid voting period");
        }

        let quorum = quorum_bps.unwrap_or(DEFAULT_QUORUM_BPS);
        if quorum == 0 || quorum > 10_000 {
            panic!("invalid quorum bps");
        }

        env.storage().persistent().set(&ADMIN, &admin);
        env.storage().persistent().set(&TOKEN, &mnt_token);

        env.storage()
            .persistent()
            .set(&SNAPSHOT, &snapshot_contract);
        env.storage().persistent().set(&VOTING_PERIOD_SECS, &period);

        env.storage().persistent().set(&VOTING_PERIOD_SECS, &period);

        env.storage().persistent().set(&QUORUM_BPS, &quorum);
        env.storage().persistent().set(&PROPOSAL_COUNT, &0u32);
    }

    pub fn set_timelock(env: Env, timelock: Address) {
        let admin: Address = env.storage().persistent().get(&ADMIN).unwrap();
        admin.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::Timelock, &timelock);
    }

    pub fn set_templates_contract(env: Env, templates_contract: Address) {
        let admin: Address = env.storage().persistent().get(&ADMIN).unwrap();
        admin.require_auth();
        env.storage()
            .persistent()
            .set(&TEMPLATES, &templates_contract);
    }

    pub fn create_proposal(
        env: Env,
        proposer: Address,
        title: Bytes,
        description_hash: BytesN<32>,
        action: ProposalAction,
    ) -> u32 {
        proposer.require_auth();
        Self::require_initialized(&env);

        let mut count: u32 = env.storage().instance().get(&PROPOSAL_COUNT).unwrap_or(0);
        count = count.checked_add(1).expect("proposal overflow");

        if let ProposalAction::ExecuteCall(target, function, args) = &action {
            if let Some(templates_contract) =
                env.storage().persistent().get::<_, Address>(&TEMPLATES)
            {
                let opt_hash: Option<BytesN<32>> = env.invoke_contract(
                    &templates_contract,
                    &Symbol::new(&env, "get_template_hash"),
                    (target.clone(), function.clone()).into_val(&env),
                );

                if let Some(expected_hash) = opt_hash {
                    let args_hash = Self::compute_args_hash(&env, args);
                    if args_hash != expected_hash {
                        panic!("args do not match template hash");
                    }
                } else {
                    env.storage()
                        .persistent()
                        .set(&DataKey::CustomProposal(count), &true);
                }
            }
        }

        let now = env.ledger().timestamp();
        let voting_period_secs: u64 = env
            .storage()
            .instance()
            .get(&VOTING_PERIOD_SECS)
            .unwrap_or(DEFAULT_VOTING_PERIOD_SECS);

        let snapshot_contract: Address = env
            .storage()
            .persistent()
            .get(&SNAPSHOT)
            .expect("snapshot not set");
        env.invoke_contract::<()>(
            &snapshot_contract,
            &Symbol::new(&env, "record_snapshot"),
            (count,).into_val(&env),
        );

        let total_supply_snapshot: i128 = env.invoke_contract(
            &snapshot_contract,
            &Symbol::new(&env, "get_total_supply_at"),
            (count,).into_val(&env),
        );

        let proposal = Proposal {
            id: count,
            proposer: proposer.clone(),
            title,
            description_hash,
            action,
            status: ProposalStatus::Active,
            created_at: now,
            voting_ends_at: now
                .checked_add(voting_period_secs)
                .expect("voting end overflow"),
            snapshot_ledger: env.ledger().sequence(),
            total_supply_snapshot,
            votes_for: 0,
            votes_against: 0,
        };

        env.storage().instance().set(&PROPOSAL_COUNT, &count);
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(count), &proposal);

        env.events().publish(
            (
                Symbol::new(&env, "governance"),
                Symbol::new(&env, "proposal_created"),
                count,
            ),
            (proposer, proposal.snapshot_ledger, proposal.voting_ends_at),
        );

        count
    }

    pub fn vote(env: Env, voter: Address, proposal_id: u32, support: bool) {
        voter.require_auth();
        let mut proposal = Self::get_proposal(env.clone(), proposal_id);
        Self::require_active_proposal(&env, &proposal);

        let key = DataKey::Vote(proposal_id, voter.clone());
        if env.storage().persistent().has(&key) {
            panic!("already voted");
        }

        let snapshot_contract: Address = env
            .storage()
            .persistent()
            .get(&SNAPSHOT)
            .expect("snapshot not set");
        let weight: i128 = env.invoke_contract(
            &snapshot_contract,
            &Symbol::new(&env, "get_voting_power"),
            (proposal_id, voter.clone()).into_val(&env),
        );

        if weight <= 0 {
            panic!("no voting power");
        }

        if support {
            proposal.votes_for = proposal
                .votes_for
                .checked_add(weight)
                .expect("votes for overflow");
        } else {
            proposal.votes_against = proposal
                .votes_against
                .checked_add(weight)
                .expect("votes against overflow");
        }

        env.storage().persistent().set(&key, &support);
        env.storage()
            .persistent()
            .set(&DataKey::VoteWeight(proposal_id, voter.clone()), &weight);
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        env.events().publish(
            (
                Symbol::new(&env, "governance"),
                symbol_short!("vote_cast"),
                proposal_id,
            ),
            (voter, support, weight),
        );
    }

    pub fn execute_proposal(env: Env, proposal_id: u32) {
        let mut proposal = Self::get_proposal(env.clone(), proposal_id);

        if proposal.status == ProposalStatus::Executed {
            panic!("proposal already executed");
        }

        if env.ledger().timestamp() < proposal.voting_ends_at {
            panic!("voting period not ended");
        }

        if proposal.status == ProposalStatus::Cancelled || proposal.status == ProposalStatus::Failed
        {
            panic!("proposal not executable");
        }

        let quorum_bps: u32 = if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::CustomProposal(proposal_id))
            .unwrap_or(false)
        {
            CUSTOM_PROPOSAL_QUORUM_BPS
        } else {
            env.storage()
                .instance()
                .get(&QUORUM_BPS)
                .unwrap_or(DEFAULT_QUORUM_BPS)
        };
        let total_votes = proposal
            .votes_for
            .checked_add(proposal.votes_against)
            .expect("vote overflow");

        let quorum_met = if proposal.total_supply_snapshot <= 0 {
            false
        } else {
            total_votes.checked_mul(10_000).expect("quorum overflow")
                >= proposal
                    .total_supply_snapshot
                    .checked_mul(quorum_bps as i128)
                    .expect("quorum threshold overflow")
        };

        let passed = quorum_met && proposal.votes_for > proposal.votes_against;

        if !passed {
            proposal.status = ProposalStatus::Failed;
            env.storage()
                .persistent()
                .set(&DataKey::Proposal(proposal_id), &proposal);
            return;
        }

        proposal.status = ProposalStatus::Passed;
        Self::apply_action(&env, &proposal.action);
        proposal.status = ProposalStatus::Executed;
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        env.events().publish(
            (
                Symbol::new(&env, "governance"),
                Symbol::new(&env, "proposal_executed"),
                proposal_id,
            ),
            true,
        );
    }

    pub fn cancel_proposal(env: Env, proposal_id: u32) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN)
            .expect("not initialized");
        admin.require_auth();

        let mut proposal = Self::get_proposal(env.clone(), proposal_id);
        if proposal.status == ProposalStatus::Executed {
            panic!("cannot cancel executed proposal");
        }

        proposal.status = ProposalStatus::Cancelled;
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);
    }

    pub fn get_proposal(env: Env, id: u32) -> Proposal {
        env.storage()
            .persistent()
            .get(&DataKey::Proposal(id))
            .expect("proposal not found")
    }

    pub fn get_vote(env: Env, id: u32, voter: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Vote(id, voter))
            .unwrap_or(false)
    }

    pub fn get_vote_weight(env: Env, id: u32, voter: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::VoteWeight(id, voter))
            .unwrap_or(0)
    }

    fn require_initialized(env: &Env) {
        if !env.storage().instance().has(&ADMIN) {
            panic!("not initialized");
        }
    }

    fn require_active_proposal(env: &Env, proposal: &Proposal) {
        if proposal.status != ProposalStatus::Active {
            panic!("proposal not active");
        }

        if env.ledger().timestamp() >= proposal.voting_ends_at {
            panic!("voting period ended");
        }
    }

    fn token_address(env: &Env) -> Address {
        env.storage().instance().get(&TOKEN).expect("token not set")
    }

    fn get_balance(env: &Env, addr: &Address) -> i128 {
        let token = Self::token_address(env);
        let fn_name = Symbol::new(env, "balance");
        let args = vec![env, addr.clone().into_val(env)];
        env.invoke_contract::<i128>(&token, &fn_name, args)
    }

    fn get_total_supply(env: &Env) -> i128 {
        let token = Self::token_address(env);
        let fn_name = Symbol::new(env, "total_supply");
        let args = vec![env];
        env.invoke_contract::<i128>(&token, &fn_name, args)
    }

    fn apply_action(env: &Env, action: &ProposalAction) {
        match action {
            ProposalAction::UpdateFee(new_fee_bps) => {
                env.storage().instance().set(&CURRENT_FEE_BPS, new_fee_bps);
            }
            ProposalAction::UpdateAutoRelease(new_delay) => {
                env.storage()
                    .instance()
                    .set(&CURRENT_AUTO_RELEASE_SECS, new_delay);
            }
            ProposalAction::AddAsset(asset) => {
                env.storage()
                    .persistent()
                    .set(&DataKey::ApprovedAsset(asset.clone()), &true);
            }
            ProposalAction::UpdateAdmin(new_admin) => {
                env.storage().instance().set(&ADMIN, new_admin);
            }
            ProposalAction::ExecuteCall(target, function, args) => {
                let mut val_args = vec![env];
                for arg in args.iter() {
                    val_args.push_back(soroban_sdk::Val::from_payload(arg));
                }
                env.invoke_contract::<soroban_sdk::Val>(target, function, val_args);
            }
        }
    }

    fn compute_args_hash(env: &Env, args: &Vec<u64>) -> BytesN<32> {
        let mut buf = Bytes::new(env);
        for arg in args.iter() {
            let b = arg.to_be_bytes();
            for byte in b.iter() {
                buf.push_back(*byte);
            }
        }
        env.crypto().sha256(&buf).into()
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};

    #[contract]
    pub struct MockMntToken;

    #[contractimpl]
    impl MockMntToken {
        pub fn set_total_supply(env: Env, amount: i128) {
            env.storage()
                .persistent()
                .set(&symbol_short!("TOT_SUP"), &amount);
        }

        pub fn set_balance(env: Env, addr: Address, amount: i128) {
            env.storage()
                .persistent()
                .set(&(symbol_short!("BAL"), addr), &amount);
        }

        pub fn balance(env: Env, addr: Address) -> i128 {
            env.storage()
                .persistent()
                .get(&(symbol_short!("BAL"), addr))
                .unwrap_or(0)
        }

        pub fn total_supply(env: Env) -> i128 {
            env.storage()
                .persistent()
                .get(&symbol_short!("TOT_SUP"))
                .unwrap_or(0)
        }
    }

    #[contract]
    pub struct MockSnapshot;

    #[contractimpl]
    impl MockSnapshot {
        pub fn record_snapshot(env: Env, _id: u32) {
            env.storage()
                .persistent()
                .set(&symbol_short!("TOT_SUP"), &1000i128);
        }
        pub fn get_total_supply_at(env: Env, _id: u32) -> i128 {
            env.storage()
                .persistent()
                .get(&symbol_short!("TOT_SUP"))
                .unwrap_or(0)
        }
        pub fn get_voting_power(env: Env, _id: u32, voter: Address) -> i128 {
            let token: Address = env
                .storage()
                .persistent()
                .get(&symbol_short!("TOKEN"))
                .unwrap();
            let args = vec![&env, voter.into_val(&env)];
            env.invoke_contract::<i128>(&token, &Symbol::new(&env, "balance"), args)
        }
        pub fn set_token(env: Env, token: Address) {
            env.storage()
                .persistent()
                .set(&symbol_short!("TOKEN"), &token);
        }
    }

    #[test]
    fn test_full_proposal_lifecycle() {
        let env = Env::default();
        env.mock_all_auths();

        let gov_id = env.register_contract(None, GovernanceContract);
        let token_id = env.register_contract(None, MockMntToken);
        let snapshot_id = env.register_contract(None, MockSnapshot);
        let gov = GovernanceContractClient::new(&env, &gov_id);
        let token = MockMntTokenClient::new(&env, &token_id);
        let snapshot = MockSnapshotClient::new(&env, &snapshot_id);
        snapshot.set_token(&token_id);

        let admin = Address::generate(&env);
        let voter = Address::generate(&env);
        gov.initialize(
            &admin,
            &token_id,
            &snapshot_id,
            &Some(10u64),
            &Some(1_000u32),
        );
        token.set_total_supply(&1_000i128);
        token.set_balance(&voter, &200i128);

        let title = Bytes::from_slice(&env, b"Update fee");
        let description_hash = BytesN::from_array(&env, &[1u8; 32]);
        let proposal_id = gov.create_proposal(
            &voter,
            &title,
            &description_hash,
            &ProposalAction::UpdateFee(300),
        );

        gov.vote(&voter, &proposal_id, &true);
        assert!(gov.get_vote(&proposal_id, &voter));

        env.ledger().set_timestamp(env.ledger().timestamp() + 11);
        gov.execute_proposal(&proposal_id);

        let proposal = gov.get_proposal(&proposal_id);
        assert_eq!(proposal.status, ProposalStatus::Executed);
    }

    #[test]
    fn test_quorum_failure() {
        let env = Env::default();
        env.mock_all_auths();

        let gov_id = env.register_contract(None, GovernanceContract);
        let token_id = env.register_contract(None, MockMntToken);
        let snapshot_id = env.register_contract(None, MockSnapshot);
        let gov = GovernanceContractClient::new(&env, &gov_id);
        let token = MockMntTokenClient::new(&env, &token_id);
        let snapshot = MockSnapshotClient::new(&env, &snapshot_id);
        snapshot.set_token(&token_id);

        let admin = Address::generate(&env);
        let voter = Address::generate(&env);
        gov.initialize(
            &admin,
            &token_id,
            &snapshot_id,
            &Some(10u64),
            &Some(1_000u32),
        );

        token.set_total_supply(&10_000i128);
        token.set_balance(&voter, &100i128);

        let title = Bytes::from_slice(&env, b"Raise delay");
        let description_hash = BytesN::from_array(&env, &[2u8; 32]);
        let proposal_id = gov.create_proposal(
            &voter,
            &title,
            &description_hash,
            &ProposalAction::UpdateAutoRelease(86_400),
        );

        gov.vote(&voter, &proposal_id, &true);
        env.ledger().set_timestamp(env.ledger().timestamp() + 11);
        gov.execute_proposal(&proposal_id);

        let proposal = gov.get_proposal(&proposal_id);
        assert_eq!(proposal.status, ProposalStatus::Failed);
    }

    #[test]
    #[should_panic(expected = "already voted")]
    fn test_double_vote_prevention() {
        let env = Env::default();
        env.mock_all_auths();

        let gov_id = env.register_contract(None, GovernanceContract);
        let token_id = env.register_contract(None, MockMntToken);
        let snapshot_id = env.register_contract(None, MockSnapshot);
        let gov = GovernanceContractClient::new(&env, &gov_id);
        let token = MockMntTokenClient::new(&env, &token_id);
        let snapshot = MockSnapshotClient::new(&env, &snapshot_id);
        snapshot.set_token(&token_id);

        let admin = Address::generate(&env);
        let voter = Address::generate(&env);
        gov.initialize(
            &admin,
            &token_id,
            &snapshot_id,
            &Some(10u64),
            &Some(1_000u32),
        );
        token.set_total_supply(&1_000i128);
        token.set_balance(&voter, &200i128);

        let title = Bytes::from_slice(&env, b"Asset listing");
        let description_hash = BytesN::from_array(&env, &[3u8; 32]);
        let proposal_id = gov.create_proposal(
            &voter,
            &title,
            &description_hash,
            &ProposalAction::AddAsset(Address::generate(&env)),
        );

        gov.vote(&voter, &proposal_id, &true);
        gov.vote(&voter, &proposal_id, &false);
    }

    // --- Template validation tests ---

    #[contract]
    pub struct MockTemplates;

    #[contractimpl]
    impl MockTemplates {
        pub fn add_template(
            env: Env,
            _admin: Address,
            target: Address,
            function: Symbol,
            args_schema_hash: BytesN<32>,
        ) {
            env.storage().persistent().set(
                &(symbol_short!("TMPL"), target, function),
                &args_schema_hash,
            );
        }

        pub fn get_template_hash(
            env: Env,
            target: Address,
            function: Symbol,
        ) -> Option<BytesN<32>> {
            env.storage()
                .persistent()
                .get(&(symbol_short!("TMPL"), target, function))
        }
    }

    fn compute_args_hash(env: &Env, args: &Vec<u64>) -> BytesN<32> {
        let mut buf = Bytes::new(env);
        for arg in args.iter() {
            let b = arg.to_be_bytes();
            for byte in b.iter() {
                buf.push_back(*byte);
            }
        }
        env.crypto().sha256(&buf).into()
    }

    #[test]
    fn test_execute_call_with_matching_template() {
        let env = Env::default();
        env.mock_all_auths();

        let gov_id = env.register_contract(None, GovernanceContract);
        let token_id = env.register_contract(None, MockMntToken);
        let snapshot_id = env.register_contract(None, MockSnapshot);
        let templates_id = env.register_contract(None, MockTemplates);
        let gov = GovernanceContractClient::new(&env, &gov_id);
        let token = MockMntTokenClient::new(&env, &token_id);
        let snapshot = MockSnapshotClient::new(&env, &snapshot_id);
        let templates = MockTemplatesClient::new(&env, &templates_id);
        snapshot.set_token(&token_id);

        let admin = Address::generate(&env);
        let voter = Address::generate(&env);

        gov.initialize(
            &admin,
            &token_id,
            &snapshot_id,
            &Some(10u64),
            &Some(1_000u32),
        );
        gov.set_templates_contract(&templates_id);

        token.set_total_supply(&1_000i128);
        token.set_balance(&voter, &200i128);

        let target = Address::generate(&env);
        let function = Symbol::new(&env, "set_fee_bps");
        let args = vec![&env, 300u64];
        let args_hash = compute_args_hash(&env, &args);
        templates.add_template(&admin, &target, &function, &args_hash);

        let title = Bytes::from_slice(&env, b"Set fee via template");
        let description_hash = BytesN::from_array(&env, &[4u8; 32]);
        let proposal_id = gov.create_proposal(
            &voter,
            &title,
            &description_hash,
            &ProposalAction::ExecuteCall(target, function, args),
        );

        gov.vote(&voter, &proposal_id, &true);
        env.ledger().set_timestamp(env.ledger().timestamp() + 11);
        gov.execute_proposal(&proposal_id);

        let proposal = gov.get_proposal(&proposal_id);
        assert_eq!(proposal.status, ProposalStatus::Executed);
    }

    #[test]
    #[should_panic(expected = "args do not match template hash")]
    fn test_execute_call_with_non_matching_args() {
        let env = Env::default();
        env.mock_all_auths();

        let gov_id = env.register_contract(None, GovernanceContract);
        let token_id = env.register_contract(None, MockMntToken);
        let snapshot_id = env.register_contract(None, MockSnapshot);
        let templates_id = env.register_contract(None, MockTemplates);
        let gov = GovernanceContractClient::new(&env, &gov_id);
        let token = MockMntTokenClient::new(&env, &token_id);
        let snapshot = MockSnapshotClient::new(&env, &snapshot_id);
        let templates = MockTemplatesClient::new(&env, &templates_id);
        snapshot.set_token(&token_id);

        let admin = Address::generate(&env);
        let voter = Address::generate(&env);

        gov.initialize(
            &admin,
            &token_id,
            &snapshot_id,
            &Some(10u64),
            &Some(1_000u32),
        );
        gov.set_templates_contract(&templates_id);

        token.set_total_supply(&1_000i128);
        token.set_balance(&voter, &200i128);

        let target = Address::generate(&env);
        let function = Symbol::new(&env, "set_fee_bps");
        let allowed_args = vec![&env, 300u64];
        let allowed_hash = compute_args_hash(&env, &allowed_args);
        templates.add_template(&admin, &target, &function, &allowed_hash);

        let bad_args = vec![&env, 500u64];
        let title = Bytes::from_slice(&env, b"Set fee to wrong value");
        let description_hash = BytesN::from_array(&env, &[5u8; 32]);
        gov.create_proposal(
            &voter,
            &title,
            &description_hash,
            &ProposalAction::ExecuteCall(target, function, bad_args),
        );
    }

    #[test]
    fn test_execute_call_custom_quorum_no_template() {
        let env = Env::default();
        env.mock_all_auths();

        let gov_id = env.register_contract(None, GovernanceContract);
        let token_id = env.register_contract(None, MockMntToken);
        let snapshot_id = env.register_contract(None, MockSnapshot);
        let templates_id = env.register_contract(None, MockTemplates);
        let gov = GovernanceContractClient::new(&env, &gov_id);
        let token = MockMntTokenClient::new(&env, &token_id);
        let snapshot = MockSnapshotClient::new(&env, &snapshot_id);
        let _templates = MockTemplatesClient::new(&env, &templates_id);
        snapshot.set_token(&token_id);

        let admin = Address::generate(&env);
        let voter = Address::generate(&env);

        gov.initialize(
            &admin,
            &token_id,
            &snapshot_id,
            &Some(10u64),
            &Some(1_000u32),
        );
        gov.set_templates_contract(&templates_id);

        // total_supply = 1000, 30% quorum = 300 votes needed
        // standard 10% = 100 votes needed
        token.set_total_supply(&1_000i128);
        token.set_balance(&voter, &200i128);

        let target = Address::generate(&env);
        let function = Symbol::new(&env, "some_call");
        let args = vec![&env, 42u64];

        let title = Bytes::from_slice(&env, b"Custom call");
        let description_hash = BytesN::from_array(&env, &[6u8; 32]);
        let proposal_id = gov.create_proposal(
            &voter,
            &title,
            &description_hash,
            &ProposalAction::ExecuteCall(target, function, args),
        );

        // Vote yes with 200 voting power — enough for 10% (100) but not 30% (300)
        gov.vote(&voter, &proposal_id, &true);

        env.ledger().set_timestamp(env.ledger().timestamp() + 11);
        gov.execute_proposal(&proposal_id);

        let proposal = gov.get_proposal(&proposal_id);
        assert_eq!(proposal.status, ProposalStatus::Failed);
    }

    #[test]
    fn test_execute_call_custom_quorum_met() {
        let env = Env::default();
        env.mock_all_auths();

        let gov_id = env.register_contract(None, GovernanceContract);
        let token_id = env.register_contract(None, MockMntToken);
        let snapshot_id = env.register_contract(None, MockSnapshot);
        let templates_id = env.register_contract(None, MockTemplates);
        let gov = GovernanceContractClient::new(&env, &gov_id);
        let token = MockMntTokenClient::new(&env, &token_id);
        let snapshot = MockSnapshotClient::new(&env, &snapshot_id);
        let _templates = MockTemplatesClient::new(&env, &templates_id);
        snapshot.set_token(&token_id);

        let admin = Address::generate(&env);
        let voter1 = Address::generate(&env);
        let voter2 = Address::generate(&env);

        gov.initialize(
            &admin,
            &token_id,
            &snapshot_id,
            &Some(10u64),
            &Some(1_000u32),
        );
        gov.set_templates_contract(&templates_id);

        token.set_total_supply(&1_000i128);
        token.set_balance(&voter1, &200i128);
        token.set_balance(&voter2, &200i128);

        let target = Address::generate(&env);
        let function = Symbol::new(&env, "some_call");
        let args = vec![&env, 42u64];

        let title = Bytes::from_slice(&env, b"Custom call meet quorum");
        let description_hash = BytesN::from_array(&env, &[7u8; 32]);
        let proposal_id = gov.create_proposal(
            &voter1,
            &title,
            &description_hash,
            &ProposalAction::ExecuteCall(target, function, args),
        );

        gov.vote(&voter1, &proposal_id, &true);
        gov.vote(&voter2, &proposal_id, &true);

        env.ledger().set_timestamp(env.ledger().timestamp() + 11);
        gov.execute_proposal(&proposal_id);

        let proposal = gov.get_proposal(&proposal_id);
        assert_eq!(proposal.status, ProposalStatus::Executed);
    }
}
