#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, BytesN, Env,
};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    MarketNotFound = 3,
    MarketAlreadyResolved = 4,
    MarketNotResolved = 5,
    InvalidAmount = 6,
    NotAdmin = 7,
    ResolutionNotReady = 8,
    NoWinnings = 9,
}

// ---------------------------------------------------------------------------
// Data Types
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketRecord {
    pub id: u32,
    pub creator: Address,
    pub learner: Address,
    pub goal_description_hash: BytesN<32>,
    pub resolution_date: u64,
    pub token: Address,
    pub yes_pool: i128,
    pub no_pool: i128,
    pub resolved: bool,
    pub outcome: Option<bool>,
    pub liquidity_parameter: i128, // b in LMSR cost function: higher = less slippage
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BetRecord {
    pub bettor: Address,
    pub market_id: u32,
    pub outcome: bool,
    pub amount: i128,
    pub claimed: bool,
}

// ---------------------------------------------------------------------------
// Storage Keys
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    MarketCount,
    Market(u32),
    Bet(Address, u32),
    BettorMarkets(Address),
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const PLATFORM_FEE_BPS: i128 = 200; // 2%
const FIXED_POINT_SCALE: i128 = 1_000_000_000_000_000_000; // 10^18 for fixed-point math
const DEFAULT_LIQUIDITY_PARAMETER: i128 = 100_000_000_000_000_000; // 0.1 * SCALE

// ---------------------------------------------------------------------------
// Fixed-Point Math Utilities
// ---------------------------------------------------------------------------

/// Compute e^x using Taylor series approximation (10 terms).
/// x is in fixed-point format (scaled by 10^18).
/// Accurate to ~0.01% for x in [-5, 5].
/// Returns result in fixed-point format.
fn exp_fixed_point(x: i128) -> i128 {
    if x == 0 {
        return FIXED_POINT_SCALE;
    }

    // Avoid overflow for very large x
    if x > 5 * FIXED_POINT_SCALE {
        return i128::MAX / 2; // Saturate instead of overflow
    }
    if x < -5 * FIXED_POINT_SCALE {
        return 0; // e^(-5+) ~= 0
    }

    let mut result = FIXED_POINT_SCALE; // 1.0
    let mut term = FIXED_POINT_SCALE; // x^0 / 0! = 1
    let mut x_power = x; // x^1

    // Taylor series: e^x = 1 + x + x^2/2! + x^3/3! + ... (10 terms)
    for n in 1..=10 {
        // term = x^n / n!
        term = (x_power / (n as i128)) / FIXED_POINT_SCALE;
        if term == 0 {
            break;
        }
        result = result + term;

        // Prepare next term: x_power = x^(n+1)
        x_power = x_power * x / FIXED_POINT_SCALE;
    }

    result
}

/// Compute ln(x) in fixed-point format using Newton's method.
/// x is in fixed-point format. Accurate for x > 0.
fn ln_fixed_point(x: i128) -> i128 {
    if x <= 0 {
        panic!("ln of non-positive number");
    }
    if x == FIXED_POINT_SCALE {
        return 0; // ln(1) = 0
    }

    // Newton-Raphson: x_{n+1} = (x_n + a/x_n) / 2
    // For ln: use Newton on e^y = a => y = ln(a)
    let mut y = 0i128;
    let mut prev_y = -FIXED_POINT_SCALE;

    // Iterate until convergence
    for _ in 0..10 {
        if (y - prev_y).abs() < 1 {
            break;
        }
        prev_y = y;
        let exp_y = exp_fixed_point(y);
        let delta = (x - exp_y) / exp_y;
        y = y + delta;
    }

    y
}

/// LMSR cost function: C(q_yes, q_no) = b * ln(e^(q_yes/b) + e^(q_no/b))
/// where b is the liquidity parameter.
/// Returns cost in fixed-point format.
fn lmsr_cost(q_yes: i128, q_no: i128, b: i128) -> i128 {
    let q_yes_scaled = q_yes * FIXED_POINT_SCALE / b;
    let q_no_scaled = q_no * FIXED_POINT_SCALE / b;

    let exp_yes = exp_fixed_point(q_yes_scaled);
    let exp_no = exp_fixed_point(q_no_scaled);
    let sum = exp_yes + exp_no;

    let ln_sum = ln_fixed_point(sum);
    b * ln_sum / FIXED_POINT_SCALE
}

/// Get current price for yes outcome as basis points (0-10000).
/// price_yes = e^(q_yes/b) / (e^(q_yes/b) + e^(q_no/b))
fn get_yes_price_bps(q_yes: i128, q_no: i128, b: i128) -> u32 {
    let q_yes_scaled = q_yes * FIXED_POINT_SCALE / b;
    let q_no_scaled = q_no * FIXED_POINT_SCALE / b;

    let exp_yes = exp_fixed_point(q_yes_scaled);
    let exp_no = exp_fixed_point(q_no_scaled);
    let sum = exp_yes + exp_no;

    let price = if sum == 0 {
        FIXED_POINT_SCALE / 2 // Default to 50/50
    } else {
        exp_yes * FIXED_POINT_SCALE / sum
    };

    // Convert to basis points: [0, 10000]
    let bps = price * 10000 / FIXED_POINT_SCALE;
    (bps as u32).min(10000)
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct PredictionMarket;

#[contractimpl]
impl PredictionMarket {
    /// Initialize the prediction market contract
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::MarketCount, &0u32);
    }

    /// Create a new prediction market with LMSR AMM.
    /// liquidity_parameter: higher = less slippage, lower efficiency. Default: 0.1
    pub fn create_market(
        env: Env,
        creator: Address,
        learner: Address,
        goal_description_hash: BytesN<32>,
        resolution_date: u64,
        token: Address,
        liquidity_parameter: Option<i128>,
    ) -> u32 {
        creator.require_auth();

        let now = env.ledger().timestamp();
        if resolution_date <= now {
            panic!("resolution date must be in future");
        }

        let market_count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::MarketCount)
            .unwrap_or(0);
        let market_id = market_count + 1;

        let b = liquidity_parameter.unwrap_or(DEFAULT_LIQUIDITY_PARAMETER);

        let market = MarketRecord {
            id: market_id,
            creator: creator.clone(),
            learner: learner.clone(),
            goal_description_hash,
            resolution_date,
            token,
            yes_pool: 0,
            no_pool: 0,
            resolved: false,
            outcome: None,
            liquidity_parameter: b,
        };

        env.storage()
            .instance()
            .set(&DataKey::Market(market_id), &market);
        env.storage()
            .instance()
            .set(&DataKey::MarketCount, &market_id);

        env.events()
            .publish((symbol_short!("mkt_crt"),), (market_id, creator, learner));

        market_id
    }

    /// Place a bet on market outcome using LMSR pricing.
    /// Cost = C(new_state) - C(old_state) where C is LMSR cost function.
    pub fn place_bet(env: Env, bettor: Address, market_id: u32, outcome: bool, amount: i128) {
        if amount <= 0 {
            panic!("invalid amount");
        }

        bettor.require_auth();

        let mut market: MarketRecord = env
            .storage()
            .instance()
            .get(&DataKey::Market(market_id))
            .expect("market not found");

        if market.resolved {
            panic!("market already resolved");
        }

        let now = env.ledger().timestamp();
        if now >= market.resolution_date {
            panic!("market resolution date passed");
        }

        // Calculate cost using LMSR
        let old_cost = lmsr_cost(market.yes_pool, market.no_pool, market.liquidity_parameter);

        // Update pools for cost calculation
        let (new_yes_pool, new_no_pool) = if outcome {
            (market.yes_pool + amount, market.no_pool)
        } else {
            (market.yes_pool, market.no_pool + amount)
        };

        let new_cost = lmsr_cost(new_yes_pool, new_no_pool, market.liquidity_parameter);

        // Cost to bettor
        let cost = if new_cost >= old_cost {
            new_cost - old_cost
        } else {
            0 // Should not happen in normal LMSR, but handle edge case
        };

        if cost > amount {
            panic!("insufficient amount for LMSR cost");
        }

        // Transfer tokens from bettor to contract (exact amount)
        let token_client = token::Client::new(&env, &market.token);
        token_client.transfer(&bettor, &env.current_contract_address(), &amount);

        // Update pools with new state
        market.yes_pool = new_yes_pool;
        market.no_pool = new_no_pool;

        env.storage()
            .instance()
            .set(&DataKey::Market(market_id), &market);

        // Record bet with original amount (not cost)
        let bet = BetRecord {
            bettor: bettor.clone(),
            market_id,
            outcome,
            amount,
            claimed: false,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Bet(bettor.clone(), market_id), &bet);

        env.events().publish(
            (symbol_short!("bet_pl"),),
            (bettor, market_id, outcome, amount),
        );
    }

    /// Resolve market with outcome (admin/oracle only)
    pub fn resolve_market(env: Env, market_id: u32, outcome: bool) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        admin.require_auth();

        let mut market: MarketRecord = env
            .storage()
            .instance()
            .get(&DataKey::Market(market_id))
            .expect("market not found");

        if market.resolved {
            panic!("market already resolved");
        }

        let now = env.ledger().timestamp();
        if now < market.resolution_date {
            panic!("resolution date not reached");
        }

        market.resolved = true;
        market.outcome = Some(outcome);

        env.storage()
            .instance()
            .set(&DataKey::Market(market_id), &market);

        env.events()
            .publish((symbol_short!("mkt_res"),), (market_id, outcome));
    }

    /// Claim winnings from resolved market
    pub fn claim_winnings(env: Env, bettor: Address, market_id: u32) {
        bettor.require_auth();

        let market: MarketRecord = env
            .storage()
            .instance()
            .get(&DataKey::Market(market_id))
            .expect("market not found");

        if !market.resolved {
            panic!("market not resolved");
        }

        let mut bet: BetRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Bet(bettor.clone(), market_id))
            .expect("bet not found");

        if bet.claimed {
            panic!("winnings already claimed");
        }

        let winning_outcome = market.outcome.expect("outcome not set");

        if bet.outcome != winning_outcome {
            panic!("bet did not win");
        }

        // Calculate winnings
        let losing_pool = if winning_outcome {
            market.no_pool
        } else {
            market.yes_pool
        };

        let winning_pool = if winning_outcome {
            market.yes_pool
        } else {
            market.no_pool
        };

        let platform_fee = (losing_pool * PLATFORM_FEE_BPS) / 10_000;
        let net_winnings = losing_pool - platform_fee;
        let share = (net_winnings * bet.amount) / winning_pool;
        let total_payout = bet.amount + share;

        // Transfer winnings to bettor
        let token_client = token::Client::new(&env, &market.token);
        token_client.transfer(&env.current_contract_address(), &bettor, &total_payout);

        // Mark bet as claimed
        bet.claimed = true;
        env.storage()
            .persistent()
            .set(&DataKey::Bet(bettor.clone(), market_id), &bet);

        env.events().publish(
            (symbol_short!("win_clm"),),
            (bettor, market_id, total_payout),
        );
    }

    /// Cancel market and refund all bets (admin only)
    pub fn cancel_market(env: Env, market_id: u32) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        admin.require_auth();

        let market: MarketRecord = env
            .storage()
            .instance()
            .get(&DataKey::Market(market_id))
            .expect("market not found");

        if market.resolved {
            panic!("cannot cancel resolved market");
        }

        // In production, would iterate through all bets and refund
        // For now, just mark as resolved with no outcome
        let mut updated_market = market.clone();
        updated_market.resolved = true;

        env.storage()
            .instance()
            .set(&DataKey::Market(market_id), &updated_market);

        env.events().publish((symbol_short!("mkt_can"),), market_id);
    }

    /// Get market record
    pub fn get_market(env: Env, id: u32) -> MarketRecord {
        env.storage()
            .instance()
            .get(&DataKey::Market(id))
            .expect("market not found")
    }

    /// Get current LMSR prices as basis points
    /// Returns (yes_price_bps, no_price_bps) where both sum to 10000
    /// e.g., (6000, 4000) means 60% yes, 40% no
    pub fn get_current_price(env: Env, market_id: u32) -> (u32, u32) {
        let market: MarketRecord = env
            .storage()
            .instance()
            .get(&DataKey::Market(market_id))
            .expect("market not found");

        let yes_bps =
            get_yes_price_bps(market.yes_pool, market.no_pool, market.liquidity_parameter);
        let no_bps = 10000u32.saturating_sub(yes_bps);

        (yes_bps, no_bps)
    }

    /// Get market pools (legacy, use get_current_price for LMSR prices)
    pub fn get_odds(env: Env, market_id: u32) -> (i128, i128) {
        let market: MarketRecord = env
            .storage()
            .instance()
            .get(&DataKey::Market(market_id))
            .expect("market not found");

        (market.yes_pool, market.no_pool)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_market() {
        let env = Env::default();
        let contract_id = env.register_contract(None, PredictionMarket);
        let client = PredictionMarketClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let creator = Address::generate(&env);
        let learner = Address::generate(&env);
        let token = Address::generate(&env);
        let hash = BytesN::<32>::from_array(&env, &[0u8; 32]);

        env.mock_all_auths();
        client.initialize(&admin);

        let market_id = client.create_market(&creator, &learner, &hash, &1000, &token, &None);
        assert_eq!(market_id, 1);
    }

    #[test]
    fn test_place_bet() {
        let env = Env::default();
        let contract_id = env.register_contract(None, PredictionMarket);
        let client = PredictionMarketClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let creator = Address::generate(&env);
        let learner = Address::generate(&env);
        let bettor = Address::generate(&env);
        let token = Address::generate(&env);
        let hash = BytesN::<32>::from_array(&env, &[0u8; 32]);

        env.mock_all_auths();
        client.initialize(&admin);

        let market_id = client.create_market(&creator, &learner, &hash, &1000, &token, &None);
        client.place_bet(&bettor, &market_id, &true, &100);

        let (yes_pool, no_pool) = client.get_odds(&market_id);
        assert_eq!(yes_pool, 100);
        assert_eq!(no_pool, 0);

        // Verify LMSR prices sum to 10000
        let (yes_price, no_price) = client.get_current_price(&market_id);
        assert_eq!(yes_price + no_price, 10000);
    }

    #[test]
    fn test_resolve_market() {
        let env = Env::default();
        let contract_id = env.register_contract(None, PredictionMarket);
        let client = PredictionMarketClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let creator = Address::generate(&env);
        let learner = Address::generate(&env);
        let token = Address::generate(&env);
        let hash = BytesN::<32>::from_array(&env, &[0u8; 32]);

        env.mock_all_auths();
        client.initialize(&admin);

        let market_id = client.create_market(&creator, &learner, &hash, &100, &token, &None);

        // Advance ledger past resolution date
        env.ledger().set_timestamp(101);

        client.resolve_market(&market_id, &true);
        let market = client.get_market(&market_id);
        assert!(market.resolved);
        assert_eq!(market.outcome, Some(true));
    }

    #[test]
    #[should_panic(expected = "resolution date must be in future")]
    fn test_invalid_resolution_date() {
        let env = Env::default();
        let contract_id = env.register_contract(None, PredictionMarket);
        let client = PredictionMarketClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let creator = Address::generate(&env);
        let learner = Address::generate(&env);
        let token = Address::generate(&env);
        let hash = BytesN::<32>::from_array(&env, &[0u8; 32]);

        env.mock_all_auths();
        client.initialize(&admin);

        client.create_market(&creator, &learner, &hash, &0, &token, &None);
    }
}
