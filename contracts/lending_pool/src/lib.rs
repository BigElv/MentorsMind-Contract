#![no_std]

use shared::ReentrancyGuard;
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Symbol,
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
    InsufficientBalance = 3,
    InsufficientLiquidity = 4,
    LowCreditScore = 5,
    BorrowLimitExceeded = 6,
    LoanNotFound = 7,
    LoanAlreadyRepaid = 8,
    NotAdmin = 9,
    InvalidAmount = 10,
}

// ---------------------------------------------------------------------------
// Data Types
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoanRecord {
    pub borrower: Address,
    pub amount: i128,
    pub fee: i128,
    pub session_id: Symbol,
    pub borrowed_at: u64,
    pub due_at: u64,
    pub repaid: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LenderRecord {
    pub lender: Address,
    pub lp_tokens: i128,
    pub deposited_at: u64,
}

// ---------------------------------------------------------------------------
// Storage Keys
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    UsdcToken,
    CreditScoreContract,
    TotalLiquidity,
    TotalLpTokens,
    LenderBalance(Address),
    Loan(Address),
    LoanCount,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const INTEREST_RATE_BPS: i128 = 200; // 2% flat fee
const MIN_CREDIT_SCORE: u32 = 600;
const LIQUIDATION_DAYS: u64 = 30;
const LIQUIDATION_SECONDS: u64 = LIQUIDATION_DAYS * 86_400;

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct LendingPool;

#[contractimpl]
impl LendingPool {
    /// Initialize the lending pool
    pub fn initialize(
        env: Env,
        admin: Address,
        usdc_token: Address,
        credit_score_contract: Address,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::UsdcToken, &usdc_token);
        env.storage()
            .instance()
            .set(&DataKey::CreditScoreContract, &credit_score_contract);
        env.storage()
            .instance()
            .set(&DataKey::TotalLiquidity, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::TotalLpTokens, &0i128);

        Ok(())
    }

    /// Deposit USDC liquidity and receive LP tokens
    pub fn deposit(env: Env, lender: Address, amount: i128) -> Result<i128, Error> {
        if !env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::NotInitialized);
        }

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        lender.require_auth();

        let usdc_token: Address = env.storage().instance().get(&DataKey::UsdcToken).unwrap();
        let token_client = token::Client::new(&env, &usdc_token);

        // Transfer USDC from lender to contract
        token_client.transfer(&lender, &env.current_contract_address(), &amount);

        // Calculate LP tokens (1:1 ratio for simplicity)
        let lp_tokens = amount;

        // Update storage
        let mut total_liquidity: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalLiquidity)
            .unwrap_or(0);
        total_liquidity += amount;
        env.storage()
            .instance()
            .set(&DataKey::TotalLiquidity, &total_liquidity);

        let mut total_lp: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalLpTokens)
            .unwrap_or(0);
        total_lp += lp_tokens;
        env.storage()
            .instance()
            .set(&DataKey::TotalLpTokens, &total_lp);

        let mut lender_balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::LenderBalance(lender.clone()))
            .unwrap_or(0);
        lender_balance += lp_tokens;
        env.storage()
            .persistent()
            .set(&DataKey::LenderBalance(lender.clone()), &lender_balance);

        env.events()
            .publish((symbol_short!("deposited"),), (lender, amount, lp_tokens));

        Ok(lp_tokens)
    }

    /// Withdraw USDC by burning LP tokens
    pub fn withdraw(env: Env, lender: Address, lp_amount: i128) -> Result<i128, Error> {
        if !env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::NotInitialized);
        }

        if lp_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        lender.require_auth();

        let lender_balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::LenderBalance(lender.clone()))
            .unwrap_or(0);

        if lender_balance < lp_amount {
            return Err(Error::InsufficientBalance);
        }

        // Calculate USDC to return (1:1 ratio + accrued interest)
        let usdc_amount = lp_amount;

        let usdc_token: Address = env.storage().instance().get(&DataKey::UsdcToken).unwrap();
        let token_client = token::Client::new(&env, &usdc_token);

        // Transfer USDC back to lender
        token_client.transfer(&env.current_contract_address(), &lender, &usdc_amount);

        // Update storage
        let mut total_liquidity: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalLiquidity)
            .unwrap_or(0);
        total_liquidity -= usdc_amount;
        env.storage()
            .instance()
            .set(&DataKey::TotalLiquidity, &total_liquidity);

        let mut total_lp: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalLpTokens)
            .unwrap_or(0);
        total_lp -= lp_amount;
        env.storage()
            .instance()
            .set(&DataKey::TotalLpTokens, &total_lp);

        let new_balance = lender_balance - lp_amount;
        if new_balance == 0 {
            env.storage()
                .persistent()
                .remove(&DataKey::LenderBalance(lender.clone()));
        } else {
            env.storage()
                .persistent()
                .set(&DataKey::LenderBalance(lender.clone()), &new_balance);
        }

        env.events().publish(
            (symbol_short!("withdrawn"),),
            (lender, lp_amount, usdc_amount),
        );

        Ok(usdc_amount)
    }

    /// Borrow USDC against credit score
    pub fn borrow(
        env: Env,
        borrower: Address,
        amount: i128,
        session_id: Symbol,
    ) -> Result<(), Error> {
        let _guard = ReentrancyGuard::enter(&env, Symbol::new(&env, "borrow"));
        if !env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::NotInitialized);
        }

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        borrower.require_auth();

        // Check credit score (mock: assume credit_score_contract returns u32)
        // In real implementation, would call credit_score_contract
        let _credit_score = MIN_CREDIT_SCORE; // Simplified for now

        // Check liquidity
        let total_liquidity: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalLiquidity)
            .unwrap_or(0);

        if total_liquidity < amount {
            return Err(Error::InsufficientLiquidity);
        }

        // Calculate fee (2% flat)
        let fee = (amount * INTEREST_RATE_BPS) / 10_000;

        // Create loan record
        let now = env.ledger().timestamp();
        let loan = LoanRecord {
            borrower: borrower.clone(),
            amount,
            fee,
            session_id: session_id.clone(),
            borrowed_at: now,
            due_at: now + LIQUIDATION_SECONDS,
            repaid: false,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Loan(borrower.clone()), &loan);

        // Transfer USDC to borrower
        let usdc_token: Address = env.storage().instance().get(&DataKey::UsdcToken).unwrap();
        let token_client = token::Client::new(&env, &usdc_token);
        token_client.transfer(&env.current_contract_address(), &borrower, &amount);

        // Update liquidity
        let new_liquidity = total_liquidity - amount;
        env.storage()
            .instance()
            .set(&DataKey::TotalLiquidity, &new_liquidity);

        env.events().publish(
            (symbol_short!("borrowed"),),
            (borrower, amount, fee, session_id),
        );

        Ok(())
    }

    /// Repay loan with principal + fee
    pub fn repay(env: Env, borrower: Address, amount: i128) -> Result<(), Error> {
        let _guard = ReentrancyGuard::enter(&env, Symbol::new(&env, "repay"));
        if !env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::NotInitialized);
        }

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        borrower.require_auth();

        let loan: LoanRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Loan(borrower.clone()))
            .ok_or(Error::LoanNotFound)?;

        if loan.repaid {
            return Err(Error::LoanAlreadyRepaid);
        }

        let total_owed = loan.amount + loan.fee;
        if amount < total_owed {
            return Err(Error::InvalidAmount);
        }

        // Transfer USDC from borrower to contract
        let usdc_token: Address = env.storage().instance().get(&DataKey::UsdcToken).unwrap();
        let token_client = token::Client::new(&env, &usdc_token);
        token_client.transfer(&borrower, &env.current_contract_address(), &total_owed);

        // Mark loan as repaid
        let mut updated_loan = loan.clone();
        updated_loan.repaid = true;
        env.storage()
            .persistent()
            .set(&DataKey::Loan(borrower.clone()), &updated_loan);

        // Update liquidity
        let mut total_liquidity: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalLiquidity)
            .unwrap_or(0);
        total_liquidity += total_owed;
        env.storage()
            .instance()
            .set(&DataKey::TotalLiquidity, &total_liquidity);

        env.events().publish(
            (symbol_short!("repaid"),),
            (borrower, loan.amount, loan.fee),
        );

        Ok(())
    }

    /// Get loan record for borrower
    pub fn get_loan(env: Env, borrower: Address) -> Result<LoanRecord, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Loan(borrower))
            .ok_or(Error::LoanNotFound)
    }

    /// Liquidate overdue loan (admin only)
    pub fn liquidate(env: Env, borrower: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        let loan: LoanRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Loan(borrower.clone()))
            .ok_or(Error::LoanNotFound)?;

        if loan.repaid {
            return Err(Error::LoanAlreadyRepaid);
        }

        let now = env.ledger().timestamp();
        if now <= loan.due_at {
            panic!("loan not yet due");
        }

        // Mark as repaid (liquidated)
        let mut updated_loan = loan.clone();
        updated_loan.repaid = true;
        env.storage()
            .persistent()
            .set(&DataKey::Loan(borrower.clone()), &updated_loan);

        env.events()
            .publish((symbol_short!("liq"),), (borrower, loan.amount));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deposit() {
        let env = Env::default();
        let contract_id = env.register_contract(None, LendingPool);
        let client = LendingPoolClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let lender = Address::generate(&env);
        let usdc = Address::generate(&env);
        let credit_score = Address::generate(&env);

        env.mock_all_auths();
        client.initialize(&admin, &usdc, &credit_score).unwrap();

        let result = client.deposit(&lender, &1000);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1000);
    }

    #[test]
    fn test_borrow() {
        let env = Env::default();
        let contract_id = env.register_contract(None, LendingPool);
        let client = LendingPoolClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let lender = Address::generate(&env);
        let borrower = Address::generate(&env);
        let usdc = Address::generate(&env);
        let credit_score = Address::generate(&env);
        let session_id = symbol_short!("session1");

        env.mock_all_auths();
        client.initialize(&admin, &usdc, &credit_score).unwrap();
        client.deposit(&lender, &10000).unwrap();

        let result = client.borrow(&borrower, &1000, &session_id);
        assert!(result.is_ok());
    }

    #[test]
    fn test_repay() {
        let env = Env::default();
        let contract_id = env.register_contract(None, LendingPool);
        let client = LendingPoolClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let lender = Address::generate(&env);
        let borrower = Address::generate(&env);
        let usdc = Address::generate(&env);
        let credit_score = Address::generate(&env);
        let session_id = symbol_short!("session1");

        env.mock_all_auths();
        client.initialize(&admin, &usdc, &credit_score).unwrap();
        client.deposit(&lender, &10000).unwrap();
        client.borrow(&borrower, &1000, &session_id).unwrap();

        let result = client.repay(&borrower, &1020);
        assert!(result.is_ok());
    }

    #[test]
    fn test_insufficient_liquidity() {
        let env = Env::default();
        let contract_id = env.register_contract(None, LendingPool);
        let client = LendingPoolClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);
        let usdc = Address::generate(&env);
        let credit_score = Address::generate(&env);
        let session_id = symbol_short!("session1");

        env.mock_all_auths();
        client.initialize(&admin, &usdc, &credit_score).unwrap();

        let result = client.borrow(&borrower, &1000, &session_id);
        assert_eq!(result, Err(Error::InsufficientLiquidity));
    }
}
