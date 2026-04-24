#![no_std]

use soroban_sdk::{contract, contractimpl, contracterror, contracttype, symbol_short, Address, Env, Symbol, TryIntoVal, Val, Vec};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized     = 2,
    NotAdmin           = 3, // reserved, currently unused
    NotSigner          = 4,
    AlreadySigner      = 5,
    ProposalNotFound   = 6,
    AlreadySigned      = 7,
    BelowThreshold     = 8,
    AlreadyExecuted    = 9,
    Cancelled          = 10,
    Expired            = 11,
    InvalidThreshold   = 12,
}

// ---------------------------------------------------------------------------
// Data Types
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
pub struct ProposalRecord {
    pub id:             u32,
    pub proposer:       Address,
    pub target:         Address,
    pub function:       Symbol,
    pub args:           Vec<Val>,
    pub approval_count: u32,
    pub expiry:         u64,
    pub executed:       bool,
    pub cancelled:      bool,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const EXPIRY_SECONDS: u64 = 7 * 24 * 60 * 60; // 604800 — 7 days

// ---------------------------------------------------------------------------
// Storage Keys
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    // Instance storage — config read on every invocation
    Threshold,
    SignerCount,
    ProposalCount,
    // Persistent storage — per-entity records
    Signer(Address),
    Proposal(u32),
    Approval(u32, Address),
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct MultisigAdminContract;

#[contractimpl]
impl MultisigAdminContract {
    pub fn initialize(
        env: Env,
        signers: Vec<Address>,
        threshold: u32,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Threshold) {
            return Err(Error::AlreadyInitialized);
        }
        if signers.is_empty() || threshold == 0 {
            return Err(Error::InvalidThreshold);
        }
        if threshold > signers.len() as u32 {
            return Err(Error::InvalidThreshold);
        }
        for signer in signers.iter() {
            if env.storage().persistent().has(&DataKey::Signer(signer.clone())) {
                return Err(Error::AlreadySigner);
            }
            env.storage().persistent().set(&DataKey::Signer(signer.clone()), &true);
        }
        env.storage().instance().set(&DataKey::Threshold, &threshold);
        env.storage().instance().set(&DataKey::SignerCount, &(signers.len() as u32));
        env.storage().instance().set(&DataKey::ProposalCount, &0u32);
        Ok(())
    }

    pub fn propose_action(
        env: Env,
        proposer: Address,
        target: Address,
        function: Symbol,
        args: Vec<Val>,
    ) -> Result<u32, Error> {
        if !env.storage().instance().has(&DataKey::Threshold) {
            return Err(Error::NotInitialized);
        }
        if !env.storage().persistent().get::<_, bool>(&DataKey::Signer(proposer.clone())).unwrap_or(false) {
            return Err(Error::NotSigner);
        }
        proposer.require_auth();
        let count: u32 = env.storage().instance().get(&DataKey::ProposalCount).unwrap_or(0);
        let new_id = count.checked_add(1).expect("proposal count overflow");
        env.storage().instance().set(&DataKey::ProposalCount, &new_id);
        let expiry = env.ledger().timestamp().checked_add(EXPIRY_SECONDS).expect("expiry overflow");
        let proposal = ProposalRecord {
            id: new_id,
            proposer: proposer.clone(),
            target,
            function,
            args,
            approval_count: 1,
            expiry,
            executed: false,
            cancelled: false,
        };
        env.storage().persistent().set(&DataKey::Proposal(new_id), &proposal);
        env.storage().persistent().set(&DataKey::Approval(new_id, proposer.clone()), &true);
        env.events().publish(
            (symbol_short!("multisig"), symbol_short!("proposed"), new_id),
            (proposer, expiry),
        );
        Ok(new_id)
    }

    pub fn sign_action(
        env: Env,
        signer: Address,
        action_id: u32,
    ) -> Result<(), Error> {
        if !env.storage().instance().has(&DataKey::Threshold) {
            return Err(Error::NotInitialized);
        }
        if !env.storage().persistent().get::<_, bool>(&DataKey::Signer(signer.clone())).unwrap_or(false) {
            return Err(Error::NotSigner);
        }
        let mut proposal: ProposalRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Proposal(action_id))
            .ok_or(Error::ProposalNotFound)?;
        if proposal.executed {
            return Err(Error::AlreadyExecuted);
        }
        if proposal.cancelled {
            return Err(Error::Cancelled);
        }
        if env.ledger().timestamp() > proposal.expiry {
            return Err(Error::Expired);
        }
        if env.storage().persistent().get::<_, bool>(&DataKey::Approval(action_id, signer.clone())).unwrap_or(false) {
            return Err(Error::AlreadySigned);
        }
        signer.require_auth();
        env.storage().persistent().set(&DataKey::Approval(action_id, signer.clone()), &true);
        proposal.approval_count = proposal.approval_count.checked_add(1).expect("approval count overflow");
        env.storage().persistent().set(&DataKey::Proposal(action_id), &proposal);
        env.events().publish(
            (symbol_short!("multisig"), symbol_short!("signed"), action_id),
            (signer, proposal.approval_count),
        );
        Ok(())
    }

    pub fn execute_action(
        env: Env,
        action_id: u32,
    ) -> Result<(), Error> {
        if !env.storage().instance().has(&DataKey::Threshold) {
            return Err(Error::NotInitialized);
        }
        let mut proposal: ProposalRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Proposal(action_id))
            .ok_or(Error::ProposalNotFound)?;
        if proposal.executed {
            return Err(Error::AlreadyExecuted);
        }
        if proposal.cancelled {
            return Err(Error::Cancelled);
        }
        if env.ledger().timestamp() > proposal.expiry {
            return Err(Error::Expired);
        }
        let threshold: u32 = env.storage().instance().get(&DataKey::Threshold).unwrap();
        if proposal.approval_count < threshold {
            return Err(Error::BelowThreshold);
        }
        // Mark executed before dispatch (prevents re-entrancy)
        proposal.executed = true;
        env.storage().persistent().set(&DataKey::Proposal(action_id), &proposal);
        // Capture for event before values are moved into dispatch
        let event_target = proposal.target.clone();
        let event_function = proposal.function.clone();
        // Dispatch: self-targeted vs external
        if proposal.target == env.current_contract_address() {
            let add_fn = Symbol::new(&env, "add_signer");
            let rem_fn = Symbol::new(&env, "remove_signer");
            if proposal.function == add_fn {
                let new_signer: Address = proposal.args.get(0)
                    .ok_or(Error::ProposalNotFound)?
                    .try_into_val(&env)
                    .map_err(|_| Error::ProposalNotFound)?;
                apply_add_signer(&env, new_signer)?;
            } else if proposal.function == rem_fn {
                let target_signer: Address = proposal.args.get(0)
                    .ok_or(Error::ProposalNotFound)?
                    .try_into_val(&env)
                    .map_err(|_| Error::ProposalNotFound)?;
                apply_remove_signer(&env, target_signer)?;
            } else {
                return Err(Error::ProposalNotFound);
            }
        } else {
            env.invoke_contract::<()>(&proposal.target, &proposal.function, proposal.args.clone());
        }
        env.events().publish(
            (symbol_short!("multisig"), symbol_short!("executed"), action_id),
            (action_id, event_target, event_function),
        );
        Ok(())
    }

    pub fn cancel_action(
        env: Env,
        caller: Address,
        action_id: u32,
    ) -> Result<(), Error> {
        if !env.storage().instance().has(&DataKey::Threshold) {
            return Err(Error::NotInitialized);
        }
        let mut proposal: ProposalRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Proposal(action_id))
            .ok_or(Error::ProposalNotFound)?;
        // Caller must be the original proposer or a current signer (Req 7.1)
        let is_proposer = proposal.proposer == caller;
        let is_signer = env.storage().persistent()
            .get::<_, bool>(&DataKey::Signer(caller.clone()))
            .unwrap_or(false);
        if !is_proposer && !is_signer {
            return Err(Error::NotSigner);
        }
        if proposal.executed {
            return Err(Error::AlreadyExecuted);
        }
        if proposal.cancelled {
            return Err(Error::Cancelled);
        }
        if env.ledger().timestamp() > proposal.expiry {
            return Err(Error::Expired);
        }
        caller.require_auth();
        proposal.cancelled = true;
        env.storage().persistent().set(&DataKey::Proposal(action_id), &proposal);
        env.events().publish(
            (symbol_short!("multisig"), symbol_short!("cancelled"), action_id),
            caller,
        );
        Ok(())
    }

    pub fn get_proposal(env: Env, action_id: u32) -> Result<ProposalRecord, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Proposal(action_id))
            .ok_or(Error::ProposalNotFound)
    }

    pub fn is_signer(env: Env, address: Address) -> Result<bool, Error> {
        if !env.storage().instance().has(&DataKey::Threshold) {
            return Err(Error::NotInitialized);
        }
        Ok(env.storage().persistent()
            .get::<_, bool>(&DataKey::Signer(address))
            .unwrap_or(false))
    }

    pub fn get_threshold(env: Env) -> Result<u32, Error> {
        env.storage().instance()
            .get(&DataKey::Threshold)
            .ok_or(Error::NotInitialized)
    }

    pub fn get_signer_count(env: Env) -> Result<u32, Error> {
        env.storage().instance()
            .get(&DataKey::SignerCount)
            .ok_or(Error::NotInitialized)
    }
}

// ---------------------------------------------------------------------------
// Internal signer-management helpers (not public entry points)
// ---------------------------------------------------------------------------

fn apply_add_signer(env: &Env, new_signer: Address) -> Result<(), Error> {
    if env.storage().persistent().get::<_, bool>(&DataKey::Signer(new_signer.clone())).unwrap_or(false) {
        return Err(Error::AlreadySigner);
    }
    env.storage().persistent().set(&DataKey::Signer(new_signer.clone()), &true);
    let count: u32 = env.storage().instance().get(&DataKey::SignerCount).unwrap_or(0);
    env.storage().instance().set(&DataKey::SignerCount, &(count + 1));
    env.events().publish(
        (symbol_short!("multisig"), symbol_short!("sgn_added"), new_signer),
        count + 1,
    );
    Ok(())
}

fn apply_remove_signer(env: &Env, signer: Address) -> Result<(), Error> {
    if !env.storage().persistent().get::<_, bool>(&DataKey::Signer(signer.clone())).unwrap_or(false) {
        return Err(Error::NotSigner);
    }
    let count: u32 = env.storage().instance().get(&DataKey::SignerCount).unwrap_or(0);
    let threshold: u32 = env.storage().instance().get(&DataKey::Threshold).unwrap_or(0);
    if count.saturating_sub(1) < threshold {
        return Err(Error::InvalidThreshold);
    }
    env.storage().persistent().remove(&DataKey::Signer(signer.clone()));
    env.storage().instance().set(&DataKey::SignerCount, &(count - 1));
    env.events().publish(
        (symbol_short!("multisig"), symbol_short!("sgn_rmvd"), signer),
        count - 1,
    );
    Ok(())
}

#[cfg(test)]
mod test {
    extern crate std;

    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{vec, Env, IntoVal, Symbol};

    // -----------------------------------------------------------------------
    // Fixture
    // -----------------------------------------------------------------------

    struct Fixture {
        env:      Env,
        contract: Address,
        signers:  [Address; 5],
    }

    impl Fixture {
        fn setup() -> Self {
            let env = Env::default();
            env.mock_all_auths();
            env.ledger().set_timestamp(0);

            let s = core::array::from_fn::<Address, 5, _>(|_| Address::generate(&env));
            let contract = env.register_contract(None, MultisigAdminContract);
            MultisigAdminContractClient::new(&env, &contract)
                .initialize(
                    &vec![&env, s[0].clone(), s[1].clone(), s[2].clone(), s[3].clone(), s[4].clone()],
                    &3u32,
                );

            Fixture { env, contract, signers: s }
        }

        fn client(&self) -> MultisigAdminContractClient {
            MultisigAdminContractClient::new(&self.env, &self.contract)
        }

        /// Propose an external action as signers[0]; returns proposal id.
        fn propose(&self, target: &Address, function: &str) -> u32 {
            self.client()
                .propose_action(
                    &self.signers[0],
                    target,
                    &Symbol::new(&self.env, function),
                    &vec![&self.env],
                )
        }

        /// Sign a proposal with signers[1..n] (signers[0] already auto-approved).
        fn sign_n(&self, pid: u32, n: usize) {
            for i in 1..n {
                self.client().sign_action(&self.signers[i], &pid);
            }
        }

        /// Propose a self-targeted action (add_signer / remove_signer) with one Address arg.
        fn self_proposal(&self, function: &str, arg: &Address) -> u32 {
            self.client()
                .propose_action(
                    &self.signers[0],
                    &self.contract,
                    &Symbol::new(&self.env, function),
                    &vec![&self.env, arg.clone().into_val(&self.env)],
                )
        }
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_initialize() {
        let env = Env::default();
        env.mock_all_auths();
        let s0 = Address::generate(&env);
        let s1 = Address::generate(&env);
        let id = env.register_contract(None, MultisigAdminContract);
        let client = MultisigAdminContractClient::new(&env, &id);

        // threshold == 0
        assert_eq!(
            client.try_initialize(&vec![&env, s0.clone()], &0u32),
            Err(Ok(Error::InvalidThreshold))
        );
        // threshold > signer count
        assert_eq!(
            client.try_initialize(&vec![&env, s0.clone()], &2u32),
            Err(Ok(Error::InvalidThreshold))
        );
        // duplicate signer
        assert_eq!(
            client.try_initialize(&vec![&env, s0.clone(), s0.clone()], &1u32),
            Err(Ok(Error::AlreadySigner))
        );
        // happy path
        client.initialize(&vec![&env, s0.clone(), s1.clone()], &1u32);
        assert_eq!(client.get_threshold(), 1u32);
        assert_eq!(client.get_signer_count(), 2u32);
        assert_eq!(client.is_signer(&s0), true);
        assert_eq!(client.is_signer(&s1), true);
        // double initialize
        assert_eq!(
            client.try_initialize(&vec![&env, s0.clone()], &1u32),
            Err(Ok(Error::AlreadyInitialized))
        );
    }

    #[test]
    fn test_propose_action() {
        let f = Fixture::setup();
        let target = Address::generate(&f.env);
        let outsider = Address::generate(&f.env);

        // non-signer rejected
        assert_eq!(
            f.client().try_propose_action(
                &outsider,
                &target,
                &Symbol::new(&f.env, "do_thing"),
                &vec![&f.env],
            ),
            Err(Ok(Error::NotSigner))
        );

        // signer can propose; id starts at 1
        let pid1 = f.propose(&target, "do_thing");
        assert_eq!(pid1, 1u32);

        // ids increment
        let pid2 = f.propose(&target, "do_thing");
        assert_eq!(pid2, 2u32);

        // proposer auto-approval is reflected
        let proposal = f.client().get_proposal(&pid1);
        assert_eq!(proposal.approval_count, 1u32);
        assert_eq!(proposal.proposer, f.signers[0]);
        assert_eq!(proposal.executed, false);
        assert_eq!(proposal.cancelled, false);
        assert_eq!(proposal.expiry, EXPIRY_SECONDS); // timestamp was 0 at setup
    }

    #[test]
    fn test_sign_action() {
        let f = Fixture::setup();
        let target = Address::generate(&f.env);
        let outsider = Address::generate(&f.env);
        let pid = f.propose(&target, "do_thing");

        // non-signer rejected
        assert_eq!(
            f.client().try_sign_action(&outsider, &pid),
            Err(Ok(Error::NotSigner))
        );

        // signer can sign; approval_count increments
        f.client().sign_action(&f.signers[1], &pid);
        assert_eq!(f.client().get_proposal(&pid).approval_count, 2u32);

        // duplicate signing rejected
        assert_eq!(
            f.client().try_sign_action(&f.signers[1], &pid),
            Err(Ok(Error::AlreadySigned))
        );

        // expired proposal rejected
        f.env.ledger().set_timestamp(EXPIRY_SECONDS + 1);
        assert_eq!(
            f.client().try_sign_action(&f.signers[2], &pid),
            Err(Ok(Error::Expired))
        );
    }

    #[test]
    fn test_execute_below_threshold() {
        let f = Fixture::setup();
        let target = Address::generate(&f.env);
        let pid = f.propose(&target, "do_thing");

        // 1 approval (proposer only) — below threshold of 3
        assert_eq!(
            f.client().try_execute_action(&pid),
            Err(Ok(Error::BelowThreshold))
        );

        // 2 approvals — still below threshold
        f.client().sign_action(&f.signers[1], &pid);
        assert_eq!(
            f.client().try_execute_action(&pid),
            Err(Ok(Error::BelowThreshold))
        );
    }

    #[test]
    fn test_signer_management_via_proposal() {
        let f = Fixture::setup();
        let new_signer = Address::generate(&f.env);

        // --- add_signer ---
        let pid_add = f.self_proposal("add_signer", &new_signer);
        // need 3 approvals total; proposer = 1, sign 2 more
        f.sign_n(pid_add, 3); // signs signers[1] and signers[2]
        f.client().execute_action(&pid_add);
        assert_eq!(f.client().get_signer_count(), 6u32);
        assert_eq!(f.client().is_signer(&new_signer), true);

        // --- remove_signer ---
        let pid_rem = f.self_proposal("remove_signer", &new_signer);
        f.sign_n(pid_rem, 3);
        f.client().execute_action(&pid_rem);
        assert_eq!(f.client().get_signer_count(), 5u32);
        assert_eq!(f.client().is_signer(&new_signer), false);

        // --- removal that would violate threshold is rejected ---
        // current: 5 signers, threshold 3 — remove down to 3 is fine, but 2 is not
        let pid_r1 = f.self_proposal("remove_signer", &f.signers[3]);
        f.sign_n(pid_r1, 3);
        f.client().execute_action(&pid_r1); // 4 signers remain

        let pid_r2 = f.self_proposal("remove_signer", &f.signers[4]);
        f.sign_n(pid_r2, 3);
        f.client().execute_action(&pid_r2); // 3 signers remain == threshold

        // now removing one more would leave 2 < threshold(3) — must be rejected
        let pid_r3 = f.self_proposal("remove_signer", &f.signers[2]);
        f.sign_n(pid_r3, 3);
        assert_eq!(
            f.client().try_execute_action(&pid_r3),
            Err(Ok(Error::InvalidThreshold))
        );
    }

    #[test]
    fn test_cancel_action() {
        let f = Fixture::setup();
        let target = Address::generate(&f.env);
        let outsider = Address::generate(&f.env);
        let pid = f.propose(&target, "do_thing");

        // outsider (not proposer, not signer) rejected
        assert_eq!(
            f.client().try_cancel_action(&outsider, &pid),
            Err(Ok(Error::NotSigner))
        );

        // a current signer (not the proposer) can cancel
        f.client().cancel_action(&f.signers[1], &pid);
        assert_eq!(f.client().get_proposal(&pid).cancelled, true);

        // double cancel rejected
        assert_eq!(
            f.client().try_cancel_action(&f.signers[0], &pid),
            Err(Ok(Error::Cancelled))
        );
    }
}
