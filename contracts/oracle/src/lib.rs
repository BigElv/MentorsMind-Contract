#![no_std]
use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype, symbol_short, Address, Env, Symbol, Vec,
};

const ADMIN: Symbol = symbol_short!("ADMIN");
const FEEDERS: Symbol = symbol_short!("FEEDERS");
const RBAC: Symbol = symbol_short!("RBAC");
const MIN_FEEDERS: u32 = 3;
const MAX_POINTS: u32 = 5;
const STALE_SECS: u64 = 300;

#[contracttype]
#[derive(Clone)]
pub struct PricePoint {
    pub price: i128,
    pub timestamp: u64,
}

#[contractclient(name = "RbacContractClient")]
pub trait RbacContractTrait {
    fn has_role(env: Env, role: Symbol, account: Address) -> bool;
}

#[contract]
pub struct OracleContract;

#[contractimpl]
impl OracleContract {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().persistent().has(&ADMIN) {
            panic!("already initialized");
        }
        env.storage().persistent().set(&ADMIN, &admin);
        env.storage()
            .persistent()
            .set(&FEEDERS, &Vec::<Address>::new(&env));
    }

    pub fn set_rbac_contract(env: Env, admin: Address, rbac: Address) {
        Self::require_admin_or_role(&env, &admin, Symbol::new(&env, "ORACLE_ADMIN"));
        env.storage().persistent().set(&RBAC, &rbac);
    }

    pub fn add_feeder(env: Env, admin: Address, feeder: Address) {
        Self::require_admin_or_role(&env, &admin, Symbol::new(&env, "ORACLE_ADMIN"));
        let mut feeders: Vec<Address> = env
            .storage()
            .persistent()
            .get(&FEEDERS)
            .unwrap_or(Vec::new(&env));
        if !feeders.contains(feeder.clone()) {
            feeders.push_back(feeder);
        }
        env.storage().persistent().set(&FEEDERS, &feeders);
    }

    pub fn remove_feeder(env: Env, admin: Address, feeder: Address) {
        Self::require_admin_or_role(&env, &admin, Symbol::new(&env, "ORACLE_ADMIN"));
        let feeders: Vec<Address> = env
            .storage()
            .persistent()
            .get(&FEEDERS)
            .unwrap_or(Vec::new(&env));
        let mut next = Vec::new(&env);
        for f in feeders.iter() {
            if f != feeder {
                next.push_back(f);
            }
        }
        env.storage().persistent().set(&FEEDERS, &next);
    }

    pub fn submit_price(env: Env, feeder: Address, asset: Symbol, price: i128, timestamp: u64) {
        feeder.require_auth();
        if !Self::is_feeder(&env, &feeder)
            && !Self::has_rbac_role(&env, Symbol::new(&env, "ORACLE_FEEDER"), feeder.clone())
        {
            panic!("unauthorized feeder");
        }
        let key = (symbol_short!("PRICES"), asset.clone());
        let mut points: Vec<PricePoint> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(&env));
        points.push_back(PricePoint { price, timestamp });
        while points.len() > MAX_POINTS {
            points.remove(0);
        }
        env.storage().persistent().set(&key, &points);
        env.events().publish(
            (symbol_short!("oracle"), symbol_short!("price_upd"), asset),
            (price, timestamp),
        );
    }

    pub fn get_price(env: Env, asset: Symbol) -> (i128, u64) {
        let feeders: Vec<Address> = env
            .storage()
            .persistent()
            .get(&FEEDERS)
            .unwrap_or(Vec::new(&env));
        if feeders.len() < MIN_FEEDERS {
            panic!("not enough feeders");
        }
        let key = (symbol_short!("PRICES"), asset);
        let points: Vec<PricePoint> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(&env));
        if points.is_empty() {
            panic!("no prices");
        }
        let mut prices = Vec::new(&env);
        let mut last_updated = 0u64;
        for p in points.iter() {
            prices.push_back(p.price);
            if p.timestamp > last_updated {
                last_updated = p.timestamp;
            }
        }
        (Self::median(prices), last_updated)
    }

    pub fn is_price_stale(env: Env, asset: Symbol) -> bool {
        let (_, updated) = Self::get_price(env.clone(), asset);
        env.ledger().timestamp().saturating_sub(updated) > STALE_SECS
    }

    fn median(mut values: Vec<i128>) -> i128 {
        let n = values.len();
        let mut i = 0;
        while i < n {
            let mut j = 0;
            while j + 1 < n - i {
                let a = values.get(j).unwrap();
                let b = values.get(j + 1).unwrap();
                if a > b {
                    values.set(j, b);
                    values.set(j + 1, a);
                }
                j += 1;
            }
            i += 1;
        }
        values.get(values.len() / 2).unwrap()
    }

    fn is_feeder(env: &Env, feeder: &Address) -> bool {
        let feeders: Vec<Address> = env
            .storage()
            .persistent()
            .get(&FEEDERS)
            .unwrap_or(Vec::new(env));
        feeders.contains(feeder.clone())
    }

    fn admin(env: &Env) -> Address {
        env.storage()
            .persistent()
            .get(&ADMIN)
            .expect("not initialized")
    }

    fn require_admin_or_role(env: &Env, caller: &Address, role: Symbol) {
        caller.require_auth();
        if *caller == Self::admin(env) || Self::has_rbac_role(env, role, caller.clone()) {
            return;
        }
        panic!("unauthorized");
    }

    fn has_rbac_role(env: &Env, role: Symbol, account: Address) -> bool {
        match env.storage().persistent().get::<_, Address>(&RBAC) {
            Some(rbac) => RbacContractClient::new(env, &rbac).has_role(&role, &account),
            None => false,
        }
    }
}
