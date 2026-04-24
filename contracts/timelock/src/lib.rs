#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, BytesN, Env, Symbol, Val, Vec,
};

const ADMIN: Symbol = symbol_short!("ADMIN");
const OP_COUNT: Symbol = symbol_short!("OP_CNT");
const MIN_DELAY: u64 = 48 * 60 * 60;
const MAX_DELAY: u64 = 30 * 24 * 60 * 60;

#[contracttype]
#[derive(Clone)]
pub struct Operation {
    pub proposer: Address,
    pub target: Address,
    pub function: Symbol,
    pub args: Vec<Val>,
    pub ready_at: u64,
    pub done: bool,
}

#[contract]
pub struct TimelockController;

#[contractimpl]
impl TimelockController {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().persistent().has(&ADMIN) {
            panic!("already initialized");
        }
        env.storage().persistent().set(&ADMIN, &admin);
    }

    pub fn schedule(
        env: Env,
        caller: Address,
        target: Address,
        function: Symbol,
        args: Vec<Val>,
        delay: u64,
    ) -> BytesN<32> {
        caller.require_auth();
        if !(MIN_DELAY..=MAX_DELAY).contains(&delay) {
            panic!("invalid delay");
        }
        let mut count: u64 = env.storage().persistent().get(&OP_COUNT).unwrap_or(0);
        count += 1;
        env.storage().persistent().set(&OP_COUNT, &count);
        let mut raw = [0u8; 32];
        raw[24] = ((count >> 56) & 0xff) as u8;
        raw[25] = ((count >> 48) & 0xff) as u8;
        raw[26] = ((count >> 40) & 0xff) as u8;
        raw[27] = ((count >> 32) & 0xff) as u8;
        raw[28] = ((count >> 24) & 0xff) as u8;
        raw[29] = ((count >> 16) & 0xff) as u8;
        raw[30] = ((count >> 8) & 0xff) as u8;
        raw[31] = (count & 0xff) as u8;
        let op_id: BytesN<32> = BytesN::from_array(&env, &raw);
        let op = Operation {
            proposer: caller.clone(),
            target: target.clone(),
            function: function.clone(),
            args,
            ready_at: env.ledger().timestamp() + delay,
            done: false,
        };
        let key = (symbol_short!("OP"), op_id.clone());
        env.storage().persistent().set(&key, &op);
        env.events().publish(
            (
                symbol_short!("timelock"),
                symbol_short!("scheduled"),
                op_id.clone(),
            ),
            (caller, target, function),
        );
        op_id
    }

    pub fn execute(env: Env, operation_id: BytesN<32>) {
        let key = (symbol_short!("OP"), operation_id.clone());
        let mut op: Operation = env
            .storage()
            .persistent()
            .get(&key)
            .expect("operation not found");
        if op.done {
            panic!("operation already done");
        }
        if env.ledger().timestamp() < op.ready_at {
            panic!("operation not ready");
        }
        env.invoke_contract::<Val>(&op.target, &op.function, op.args.clone());
        op.done = true;
        env.storage().persistent().set(&key, &op);
        env.events().publish(
            (
                symbol_short!("timelock"),
                symbol_short!("executed"),
                operation_id,
            ),
            true,
        );
    }

    pub fn cancel(env: Env, operation_id: BytesN<32>) {
        let key = (symbol_short!("OP"), operation_id.clone());
        let op: Operation = env
            .storage()
            .persistent()
            .get(&key)
            .expect("operation not found");
        if op.done {
            panic!("operation already done");
        }
        let admin: Address = env
            .storage()
            .persistent()
            .get(&ADMIN)
            .expect("not initialized");
        if admin != op.proposer {
            admin.require_auth();
        } else {
            op.proposer.require_auth();
        }
        env.storage().persistent().remove(&key);
        env.events().publish(
            (
                symbol_short!("timelock"),
                symbol_short!("cancelled"),
                operation_id,
            ),
            true,
        );
    }

    pub fn is_operation_ready(env: Env, operation_id: BytesN<32>) -> bool {
        let key = (symbol_short!("OP"), operation_id);
        let op: Operation = env
            .storage()
            .persistent()
            .get(&key)
            .expect("operation not found");
        !op.done && env.ledger().timestamp() >= op.ready_at
    }

    pub fn is_operation_done(env: Env, operation_id: BytesN<32>) -> bool {
        let key = (symbol_short!("OP"), operation_id);
        let op: Operation = env
            .storage()
            .persistent()
            .get(&key)
            .expect("operation not found");
        op.done
    }
}
