#![no_std]

use soroban_sdk::{contract, contractimpl, symbol_short, Env, Symbol};

const PAUSED: Symbol = symbol_short!("PAUSED");

#[contract]
pub struct PauseGuardian;

#[contractimpl]
impl PauseGuardian {
    pub fn set_paused(env: Env, value: bool) {
        env.storage().instance().set(&PAUSED, &value);
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage().instance().get(&PAUSED).unwrap_or(false)
    }
}
