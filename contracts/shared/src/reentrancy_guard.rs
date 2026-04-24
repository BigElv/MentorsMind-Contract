use soroban_sdk::{symbol_short, Env, Symbol};

const LOCK_PREFIX: Symbol = symbol_short!("RGUARD");

pub struct ReentrancyGuard<'a> {
    env: &'a Env,
    lock_name: Symbol,
}

impl<'a> ReentrancyGuard<'a> {
    pub fn enter(env: &'a Env, lock_name: Symbol) -> Self {
        let key = (LOCK_PREFIX, lock_name.clone());
        let locked = env.storage().instance().get(&key).unwrap_or(false);
        if locked {
            panic!("reentrant call");
        }

        env.storage().instance().set(&key, &true);
        Self { env, lock_name }
    }
}

impl Drop for ReentrancyGuard<'_> {
    fn drop(&mut self) {
        let key = (LOCK_PREFIX, self.lock_name.clone());
        self.env.storage().instance().remove(&key);
    }
}
