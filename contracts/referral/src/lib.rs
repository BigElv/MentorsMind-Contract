#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, IntoVal, Symbol};

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
    LeaderboardContract,
    Referral(Address), // referee -> ReferralInfo
    ReferrerCount(Address),
    PendingReward(Address), // referrer -> amount
}

const REWARD_MENTOR: i128 = 50 * 10_000_000; // 50 MNT (7 decimals)
const REWARD_LEARNER: i128 = 20 * 10_000_000; // 20 MNT (7 decimals)

#[contract]
pub struct ReferralContract;

#[contractimpl]
impl ReferralContract {
    pub fn initialize(env: Env, admin: Address, mnt_token: Address, leaderboard: Address) {
        if env.storage().persistent().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::MNTToken, &mnt_token);
        env.storage()
            .persistent()
            .set(&DataKey::LeaderboardContract, &leaderboard);
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

        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ReferrerCount(referrer.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::ReferrerCount(referrer.clone()), &(count + 1));

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

        // Update leaderboard
        let leaderboard: Address = env
            .storage()
            .persistent()
            .get(&DataKey::LeaderboardContract)
            .expect("Leaderboard not set");
        let count = Self::get_referral_count(env, info.referrer.clone());
        env.invoke_contract::<()>(
            &leaderboard,
            &Symbol::new(&env, "record_referral"),
            (info.referrer, count).into_val(&env),
        );
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

        let leaderboard: Address = env
            .storage()
            .persistent()
            .get(&DataKey::LeaderboardContract)
            .expect("Leaderboard not set");
        let multiplier: u32 = env.invoke_contract(
            &leaderboard,
            &Symbol::new(&env, "get_multiplier"),
            (referrer.clone(),).into_val(&env),
        );

        let actual_amount = (pending * multiplier as i128) / 10000;

        let mnt_token: Address = env
            .storage()
            .persistent()
            .get(&DataKey::MNTToken)
            .expect("Token not set");

        // Mint the actual amount
        env.invoke_contract::<()>(
            &mnt_token,
            &Symbol::new(&env, "mint"),
            (referrer.clone(), actual_amount).into_val(&env),
        );

        env.storage()
            .persistent()
            .set(&DataKey::PendingReward(referrer.clone()), &0i128);

        env.events().publish(
            (
                Symbol::new(&env, "Referral"),
                Symbol::new(&env, "RewardClaimed"),
                referrer.clone(),
            ),
            RewardClaimedEventData { amount: actual_amount },
        );
    }

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
    use mentorminds_referral_leaderboard::{ReferralLeaderboardContract, ReferralLeaderboardContractClient};
    use soroban_sdk::testutils::{Address as _, Events};
    use soroban_sdk::{IntoVal, Symbol, TryFromVal};

    struct TestFixture {
        env: Env,
        mnt_id: Address,
        ref_id: Address,
        leaderboard_id: Address,
        admin: Address,
    }

    impl TestFixture {
        fn setup() -> Self {
            let env = Env::default();
            env.mock_all_auths();

            let admin = Address::generate(&env);
            let mnt_id = env.register_contract(None, MNTToken);
            let leaderboard_id = env.register_contract(None, ReferralLeaderboardContract);
            let ref_id = env.register_contract(None, ReferralContract);

            let mnt_client = MNTTokenClient::new(&env, &mnt_id);
            // Make the referral contract the admin of the MNT token so it can mint!
            mnt_client.initialize(&ref_id);

            let leaderboard_client = ReferralLeaderboardContractClient::new(&env, &leaderboard_id);
            leaderboard_client.initialize(&ref_id);

            let ref_client = ReferralContractClient::new(&env, &ref_id);
            ref_client.initialize(&admin, &mnt_id, &leaderboard_id);

            TestFixture {
                env,
                mnt_id,
                ref_id,
                leaderboard_id,
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

        // Register referral as admin
        f.client().register_referral(&referrer, &referee, &true); // true = mentor
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
        let payload = ReferralRegisteredEventData::try_from_val(&f.env, &last_event.2)
            .expect("registered payload should decode");
        assert_eq!(
            payload,
            ReferralRegisteredEventData {
                referee: referee.clone(),
                is_mentor: true,
            }
        );

        // Fulfill referral as admin
        f.client().fulfill_referral(&referee);
        assert_eq!(f.client().get_pending_rewards(&referrer), REWARD_MENTOR);

        // Claim reward as referrer
        f.client().claim_reward(&referrer);
        assert_eq!(f.client().get_pending_rewards(&referrer), 0);
        // With multiplier 2x for rank 1
        assert_eq!(f.mnt_client().balance(&referrer), REWARD_MENTOR * 2);

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
        let payload2 = RewardClaimedEventData::try_from_val(&f.env, &last_event2.2)
            .expect("reward payload should decode");
        assert_eq!(
            payload2,
            RewardClaimedEventData {
                amount: REWARD_MENTOR * 2,
            }
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
}
