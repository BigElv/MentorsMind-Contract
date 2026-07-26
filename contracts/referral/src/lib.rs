#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, vec, Address, Env, Symbol, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RefereeType {
    Mentor,
    Learner,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferralInfo {
    pub referrer: Address,
    pub referee_type: RefereeType,
    pub completed: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferralRegisteredEventData {
    pub referee: Address,
    pub is_mentor: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewardClaimedEventData {
    pub amount: i128,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    MNTToken,
    Referral(Address),                // referee -> ReferralInfo
    ReferrerCount(Address),           // all-time referrer count
    PendingReward(Address),           // referrer -> base reward amount
    EpochReferralCount(u32, Address), // (epoch_id, referrer) -> count
    EpochTopReferrers(u32),           // epoch_id -> Vec<(Address, u32)>
    EpochBonusDistributed(u32),       // epoch_id -> bool
}

const REWARD_MENTOR: i128 = 50 * 10_000_000; // 50 MNT (7 decimals)
const REWARD_LEARNER: i128 = 20 * 10_000_000; // 20 MNT (7 decimals)
const BONUS_MNT: i128 = 100 * 10_000_000; // 100 MNT per top referrer bonus
const LEADERBOARD_EPOCH_SECS: u64 = 30 * 24 * 3600; // 30 days
const MAX_MULTIPLIER: u32 = 10;
const TOP_REFERRER_BONUS_COUNT: u32 = 3;

const LEADERBOARD_MAX_SIZE: u32 = 10;

#[contract]
pub struct ReferralContract;

#[contractimpl]
impl ReferralContract {
    pub fn initialize(env: Env, admin: Address, mnt_token: Address) {
        if env.storage().persistent().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::MNTToken, &mnt_token);
    }

    pub fn register_referral(env: Env, referrer: Address, referee: Address, is_mentor: bool) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        if referrer == referee {
            panic!("Self-referral not allowed");
        }

        if env
            .storage()
            .persistent()
            .has(&DataKey::Referral(referee.clone()))
        {
            panic!("Referee already registered");
        }

        let referee_type = if is_mentor {
            RefereeType::Mentor
        } else {
            RefereeType::Learner
        };

        let info = ReferralInfo {
            referrer: referrer.clone(),
            referee_type,
            completed: false,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Referral(referee.clone()), &info);

        // Update all-time count
        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ReferrerCount(referrer.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::ReferrerCount(referrer.clone()), &(count + 1));

        // Update epoch-level count and leaderboard
        Self::record_epoch_referral(&env, &referrer);

        env.events().publish(
            (
                Symbol::new(&env, "Referral"),
                Symbol::new(&env, "Registered"),
                referrer.clone(),
            ),
            ReferralRegisteredEventData { referee, is_mentor },
        );
    }

    pub fn fulfill_referral(env: Env, referee: Address) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        let mut info: ReferralInfo = env
            .storage()
            .persistent()
            .get(&DataKey::Referral(referee.clone()))
            .expect("Referral not found");
        if info.completed {
            panic!("Already completed");
        }

        info.completed = true;
        env.storage()
            .persistent()
            .set(&DataKey::Referral(referee.clone()), &info);

        let reward = match info.referee_type {
            RefereeType::Mentor => REWARD_MENTOR,
            RefereeType::Learner => REWARD_LEARNER,
        };

        let mut pending: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::PendingReward(info.referrer.clone()))
            .unwrap_or(0);
        pending += reward;
        env.storage()
            .persistent()
            .set(&DataKey::PendingReward(info.referrer), &pending);
    }

    pub fn claim_reward(env: Env, referrer: Address) {
        referrer.require_auth();

        let pending: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::PendingReward(referrer.clone()))
            .unwrap_or(0);
        if pending <= 0 {
            panic!("No rewards to claim");
        }

        let multiplier = Self::get_multiplier_internal(&env, &referrer);
        let total = pending
            .checked_mul(multiplier as i128)
            .expect("reward overflow");

        let mnt_token: Address = env
            .storage()
            .persistent()
            .get(&DataKey::MNTToken)
            .expect("Token not set");

        let client = mentorminds_mnt_token::MNTTokenClient::new(&env, &mnt_token);
        client.mint(&referrer, &total);

        env.storage()
            .persistent()
            .set(&DataKey::PendingReward(referrer.clone()), &0i128);

        env.events().publish(
            (
                Symbol::new(&env, "Referral"),
                Symbol::new(&env, "RewardClaimed"),
                referrer.clone(),
            ),
            RewardClaimedEventData { amount: total },
        );
    }

    // --- Epoch leaderboard ---

    fn current_epoch(env: &Env) -> u32 {
        (env.ledger().timestamp() / LEADERBOARD_EPOCH_SECS) as u32
    }

    fn record_epoch_referral(env: &Env, referrer: &Address) {
        let epoch = Self::current_epoch(env);
        let key = DataKey::EpochReferralCount(epoch, referrer.clone());
        let count: u32 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(count + 1));

        // Update cached leaderboard for current epoch
        let mut lb: Vec<(Address, u32)> = env
            .storage()
            .persistent()
            .get(&DataKey::EpochTopReferrers(epoch))
            .unwrap_or(vec![env]);

        let new_count = count + 1;
        let mut found = false;
        for i in 0..lb.len() {
            let (addr, _) = lb.get(i).unwrap();
            if addr == *referrer {
                lb.set(i, (referrer.clone(), new_count));
                found = true;
                break;
            }
        }
        if !found && lb.len() < LEADERBOARD_MAX_SIZE as u32 {
            lb.push_back((referrer.clone(), new_count));
        } else if !found {
            // Check if new count beats the lowest entry
            let last = lb.get(lb.len() - 1).unwrap();
            if new_count > last.1 {
                lb.set(lb.len() - 1, (referrer.clone(), new_count));
            }
        }

        // Sort descending by count
        Self::sort_leaderboard(&mut lb);

        env.storage()
            .persistent()
            .set(&DataKey::EpochTopReferrers(epoch), &lb);
    }

    fn sort_leaderboard(lb: &mut Vec<(Address, u32)>) {
        let n = lb.len();
        if n <= 1 {
            return;
        }
        for i in 0..n {
            for j in 0..(n - 1 - i) {
                let a = lb.get(j).unwrap();
                let b = lb.get(j + 1).unwrap();
                if b.1 > a.1 {
                    lb.set(j, b);
                    lb.set(j + 1, a);
                }
            }
        }
    }

    fn get_multiplier_internal(env: &Env, referrer: &Address) -> u32 {
        let epoch = Self::current_epoch(env);
        let lb: Vec<(Address, u32)> = env
            .storage()
            .persistent()
            .get(&DataKey::EpochTopReferrers(epoch))
            .unwrap_or(vec![env]);

        for i in 0..lb.len() {
            let (addr, _) = lb.get(i).unwrap();
            if addr == *referrer {
                return match i {
                    0 => MAX_MULTIPLIER, // 10x
                    1 => 5,
                    2 => 3,
                    _ => 2,
                };
            }
        }
        1
    }

    pub fn get_multiplier(env: Env, referrer: Address) -> u32 {
        Self::get_multiplier_internal(&env, &referrer)
    }

    pub fn distribute_epoch_bonus(env: Env) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        let epoch = Self::current_epoch(&env);
        let prev_epoch = epoch.checked_sub(1).expect("no previous epoch");

        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::EpochBonusDistributed(prev_epoch))
            .unwrap_or(false)
        {
            panic!("bonus already distributed");
        }

        let mnt_token: Address = env
            .storage()
            .persistent()
            .get(&DataKey::MNTToken)
            .expect("Token not set");
        let client = mentorminds_mnt_token::MNTTokenClient::new(&env, &mnt_token);

        let lb: Vec<(Address, u32)> = env
            .storage()
            .persistent()
            .get(&DataKey::EpochTopReferrers(prev_epoch))
            .unwrap_or(vec![&env]);

        let count = core::cmp::min(lb.len(), TOP_REFERRER_BONUS_COUNT);
        for i in 0..count {
            let (addr, _) = lb.get(i).unwrap();
            client.mint(&addr, &BONUS_MNT);
        }

        env.storage()
            .persistent()
            .set(&DataKey::EpochBonusDistributed(prev_epoch), &true);
    }

    pub fn get_epoch_referral_count(env: Env, epoch: u32, referrer: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::EpochReferralCount(epoch, referrer))
            .unwrap_or(0)
    }

    pub fn get_epoch_leaderboard(env: Env, epoch: u32) -> Vec<(Address, u32)> {
        env.storage()
            .persistent()
            .get(&DataKey::EpochTopReferrers(epoch))
            .unwrap_or(vec![&env])
    }

    // --- Legacy views ---

    pub fn get_referral_count(env: Env, referrer: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::ReferrerCount(referrer))
            .unwrap_or(0)
    }

    pub fn get_pending_rewards(env: Env, referrer: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::PendingReward(referrer))
            .unwrap_or(0)
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("Not initialized")
    }
}

#[cfg(test)]
mod test {
    extern crate std;
    use super::*;
    use mentorminds_mnt_token::{MNTToken, MNTTokenClient};
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::IntoVal;

    struct TestFixture {
        env: Env,
        mnt_id: Address,
        ref_id: Address,
        admin: Address,
    }

    impl TestFixture {
        fn setup() -> Self {
            let env = Env::default();
            env.mock_all_auths();
            env.ledger().with_mut(|li| li.timestamp = 1_000_000);

            let admin = Address::generate(&env);
            let mnt_id = env.register_contract(None, MNTToken);
            let ref_id = env.register_contract(None, ReferralContract);

            let mnt_client = MNTTokenClient::new(&env, &mnt_id);
            mnt_client.initialize(&ref_id);

            let ref_client = ReferralContractClient::new(&env, &ref_id);
            ref_client.initialize(&admin, &mnt_id);

            TestFixture {
                env,
                mnt_id,
                ref_id,
                admin,
            }
        }

        fn client(&self) -> ReferralContractClient {
            ReferralContractClient::new(&self.env, &self.ref_id)
        }

        fn mnt_client(&self) -> MNTTokenClient {
            MNTTokenClient::new(&self.env, &self.mnt_id)
        }
    }

    #[test]
    fn test_initialization() {
        let f = TestFixture::setup();
        assert_eq!(f.client().get_referral_count(&Address::generate(&f.env)), 0);
    }

    #[test]
    fn test_referral_flow() {
        let f = TestFixture::setup();
        let referrer = Address::generate(&f.env);
        let referee = Address::generate(&f.env);

        f.client().register_referral(&referrer, &referee, &true);
        assert_eq!(f.client().get_referral_count(&referrer), 1);
        assert_eq!(f.client().get_pending_rewards(&referrer), 0);

        let events = f.env.events().all();
        let last_event = events.last().unwrap();
        assert_eq!(last_event.0, f.ref_id.clone());
        assert_eq!(
            last_event.1,
            (
                Symbol::new(&f.env, "Referral"),
                Symbol::new(&f.env, "Registered"),
                referrer.clone()
            )
                .into_val(&f.env)
        );
        assert_eq!(
            last_event.2,
            ReferralRegisteredEventData {
                referee: referee.clone(),
                is_mentor: true
            }
            .into_val(&f.env)
        );

        f.client().fulfill_referral(&referee);
        assert_eq!(f.client().get_pending_rewards(&referrer), REWARD_MENTOR);

        f.client().claim_reward(&referrer);
        assert_eq!(f.client().get_pending_rewards(&referrer), 0);
        // With multiplier = 1x (only referrer), amount = REWARD_MENTOR * 1
        assert_eq!(f.mnt_client().balance(&referrer), REWARD_MENTOR);

        let events2 = f.env.events().all();
        let last_event2 = events2.last().unwrap();
        assert_eq!(last_event2.0, f.ref_id.clone());
        assert_eq!(
            last_event2.1,
            (
                Symbol::new(&f.env, "Referral"),
                Symbol::new(&f.env, "RewardClaimed"),
                referrer.clone()
            )
                .into_val(&f.env)
        );
        assert_eq!(
            last_event2.2,
            RewardClaimedEventData {
                amount: REWARD_MENTOR
            }
            .into_val(&f.env)
        );
    }

    #[test]
    #[should_panic(expected = "Self-referral not allowed")]
    fn test_self_referral_rejection() {
        let f = TestFixture::setup();
        let user = Address::generate(&f.env);
        f.client().register_referral(&user, &user, &true);
    }

    #[test]
    #[should_panic(expected = "Referee already registered")]
    fn test_duplicate_referral_rejection() {
        let f = TestFixture::setup();
        let referrer1 = Address::generate(&f.env);
        let referrer2 = Address::generate(&f.env);
        let referee = Address::generate(&f.env);

        f.client().register_referral(&referrer1, &referee, &true);
        f.client().register_referral(&referrer2, &referee, &false);
    }

    // --- Epoch leaderboard tests ---

    fn register_n_referrals(
        env: &Env,
        client: &ReferralContractClient,
        referrer: &Address,
        n: u32,
    ) {
        for i in 0..n {
            let r = Address::generate(env);
            client.register_referral(referrer, &r, &(i % 2 == 0));
        }
    }

    #[test]
    fn test_epoch_referral_count() {
        let f = TestFixture::setup();
        let referrer = Address::generate(&f.env);

        register_n_referrals(&f.env, &f.client(), &referrer, 5);

        let epoch = (f.env.ledger().timestamp() / LEADERBOARD_EPOCH_SECS) as u32;
        assert_eq!(f.client().get_epoch_referral_count(&epoch, &referrer), 5);
    }

    #[test]
    fn test_multiplier_bounded() {
        let f = TestFixture::setup();
        let top = Address::generate(&f.env);
        let second = Address::generate(&f.env);
        let third = Address::generate(&f.env);
        let fourth = Address::generate(&f.env);

        // top gets 10 referrals — rank 1
        register_n_referrals(&f.env, &f.client(), &top, 10);
        // second gets 5 — rank 2
        register_n_referrals(&f.env, &f.client(), &second, 5);
        // third gets 3 — rank 3
        register_n_referrals(&f.env, &f.client(), &third, 3);
        // fourth gets 1 — rank 4 (not in top 3 but in top 10)
        register_n_referrals(&f.env, &f.client(), &fourth, 1);

        assert_eq!(f.client().get_multiplier(&top), MAX_MULTIPLIER); // 10x
        assert_eq!(f.client().get_multiplier(&second), 5);
        assert_eq!(f.client().get_multiplier(&third), 3);
        assert_eq!(f.client().get_multiplier(&fourth), 2);

        // Unranked referrer gets 1x
        let nobody = Address::generate(&f.env);
        assert_eq!(f.client().get_multiplier(&nobody), 1);
    }

    #[test]
    fn test_epoch_boundary_resets_rankings() {
        let f = TestFixture::setup();
        let early = Address::generate(&f.env);

        // Epoch 0: early gets many referrals
        register_n_referrals(&f.env, &f.client(), &early, 10);
        let epoch0 = (f.env.ledger().timestamp() / LEADERBOARD_EPOCH_SECS) as u32;
        assert_eq!(f.client().get_epoch_referral_count(&epoch0, &early), 10);
        assert_eq!(f.client().get_multiplier(&early), MAX_MULTIPLIER);

        // Advance to next epoch
        let epoch_secs = LEADERBOARD_EPOCH_SECS;
        f.env.ledger().with_mut(|li| li.timestamp += epoch_secs);

        let late = Address::generate(&f.env);
        // Epoch 1: late gets referrals, early has none
        register_n_referrals(&f.env, &f.client(), &late, 3);

        let epoch1 = (f.env.ledger().timestamp() / LEADERBOARD_EPOCH_SECS) as u32;
        assert_eq!(f.client().get_epoch_referral_count(&epoch1, &late), 3);
        assert_eq!(f.client().get_epoch_referral_count(&epoch1, &early), 0);

        // Early referrer has 0 count in epoch 1, so multiplier = 1x
        assert_eq!(f.client().get_multiplier(&early), 1);
        // Late referrer is top in epoch 1
        assert_eq!(f.client().get_multiplier(&late), MAX_MULTIPLIER);
    }

    #[test]
    fn test_distribute_epoch_bonus() {
        let f = TestFixture::setup();
        let top = Address::generate(&f.env);
        let second = Address::generate(&f.env);
        let third = Address::generate(&f.env);

        register_n_referrals(&f.env, &f.client(), &top, 10);
        register_n_referrals(&f.env, &f.client(), &second, 5);
        register_n_referrals(&f.env, &f.client(), &third, 3);

        let epoch0 = (f.env.ledger().timestamp() / LEADERBOARD_EPOCH_SECS) as u32;

        // Advance to next epoch so we can distribute for epoch 0
        let epoch_secs = LEADERBOARD_EPOCH_SECS;
        f.env.ledger().with_mut(|li| li.timestamp += epoch_secs);

        f.client().distribute_epoch_bonus();

        // Top 3 should each get BONUS_MNT
        assert_eq!(f.mnt_client().balance(&top), BONUS_MNT);
        assert_eq!(f.mnt_client().balance(&second), BONUS_MNT);
        assert_eq!(f.mnt_client().balance(&third), BONUS_MNT);

        // Fourth (non-existent) gets nothing
        let nobody = Address::generate(&f.env);
        assert_eq!(f.mnt_client().balance(&nobody), 0);
    }

    #[test]
    #[should_panic(expected = "bonus already distributed")]
    fn test_double_bonus_distribution_rejected() {
        let f = TestFixture::setup();
        let top = Address::generate(&f.env);
        register_n_referrals(&f.env, &f.client(), &top, 10);

        let epoch_secs = LEADERBOARD_EPOCH_SECS;
        f.env.ledger().with_mut(|li| li.timestamp += epoch_secs);

        f.client().distribute_epoch_bonus();
        f.client().distribute_epoch_bonus();
    }

    #[test]
    fn test_multiplier_bounded_range() {
        // Property test: multiplier is always in [1, MAX_MULTIPLIER]
        let f = TestFixture::setup();
        let referrers: Vec<Address> = (0..15).map(|_| Address::generate(&f.env)).collect();

        for (i, r) in referrers.iter().enumerate() {
            let count = (15 - i) as u32;
            register_n_referrals(&f.env, &f.client(), &r, count);
        }

        for r in referrers.iter() {
            let m = f.client().get_multiplier(&r);
            assert!(
                m >= 1 && m <= MAX_MULTIPLIER,
                "multiplier {} out of range [1, {}]",
                m,
                MAX_MULTIPLIER
            );
        }

        // Unknown referrer gets 1x
        let unknown = Address::generate(&f.env);
        assert_eq!(f.client().get_multiplier(&unknown), 1);
    }

    #[test]
    fn test_claim_with_multiplier() {
        let f = TestFixture::setup();
        let top = Address::generate(&f.env);
        let referee = Address::generate(&f.env);

        // top gets 10 referrals so multiplier = MAX_MULTIPLIER
        for i in 0..9 {
            let r = Address::generate(&f.env);
            f.client().register_referral(&top, &r, &true);
        }
        f.client().register_referral(&top, &referee, &true);
        f.client().fulfill_referral(&referee);

        assert_eq!(f.client().get_multiplier(&top), MAX_MULTIPLIER);
        assert_eq!(f.client().get_pending_rewards(&top), REWARD_MENTOR);

        f.client().claim_reward(&top);

        let expected = REWARD_MENTOR * (MAX_MULTIPLIER as i128);
        assert_eq!(f.mnt_client().balance(&top), expected);
        assert_eq!(f.client().get_pending_rewards(&top), 0);
    }
}
