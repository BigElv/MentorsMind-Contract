# Escrow Contract Invariants

This document defines the formal invariants that must hold for the escrow contract at all times. These properties are critical for security and correctness.

## Invariant 1: Token Balance Consistency

**Statement:** The sum of all active escrow amounts must be less than or equal to the contract's token balance.

```
sum(all active escrow amounts) <= contract.token_balance
```

**Rationale:** Ensures the contract never promises more funds than it holds. Prevents double-spending and insolvency.

**Verification:** After every state change that affects escrow amounts or token transfers, verify that the total active escrow amount does not exceed the contract's current token balance.

## Invariant 2: State Transition Validity

**Statement:** Escrow status transitions are one-way and follow a strict state machine. Valid transitions are:
- `Active` → `Released`
- `Active` → `Disputed`
- `Disputed` → `Released`
- `Disputed` → `Active` (dispute resolution)

Invalid transitions (e.g., `Released` → `Active`) must never occur.

**Rationale:** Prevents invalid state changes that could allow funds to be re-locked or double-released.

**Verification:** Before any state transition, validate that the transition is in the allowed set. Reject invalid transitions with a panic.

## Invariant 3: Session Completion Bounds

**Statement:** The number of sessions completed must never exceed the total number of sessions.

```
sessions_completed <= total_sessions
```

**Rationale:** Prevents logical inconsistencies where more sessions are marked complete than exist.

**Verification:** After updating `sessions_completed`, verify that it does not exceed `total_sessions`.

## Invariant 4: Fund Conservation on Release

**Statement:** When an escrow is released, the sum of funds distributed must equal the original amount:

```
platform_fee + net_amount_to_learner == original_amount
```

**Rationale:** Ensures no funds are lost or created during release. All funds are accounted for.

**Verification:** After calculating fees and distributions, verify that the sum equals the original escrow amount before any transfers.

## Invariant 5: Exclusive Fund Distribution

**Statement:** On any release path, exactly one of the following receives funds:
- Mentor (if learner defaults)
- Learner (if mentor completes)
- Treasury (if dispute resolution)

No two parties receive funds from the same escrow release.

**Rationale:** Prevents double-payment and ensures clear fund ownership.

**Verification:** After determining the release path, verify that only one recipient is selected. Panic if multiple recipients are identified.

## Invariant 6: Escrow Amount Non-Negativity

**Statement:** All escrow amounts must be non-negative.

```
escrow.amount >= 0
```

**Rationale:** Prevents negative balances and logical inconsistencies.

**Verification:** On escrow creation and any amount modification, verify that the amount is >= 0.

## Invariant 7: Timestamp Consistency

**Statement:** Timestamps must be monotonically increasing:
- `created_at <= current_time`
- `created_at <= release_time`
- `release_time >= created_at`

**Rationale:** Prevents time-based logic errors and ensures temporal consistency.

**Verification:** When recording timestamps, verify they are consistent with the ledger timestamp.

## Testing Strategy

### Unit Tests
Each invariant is tested with dedicated unit tests that:
1. Create valid escrow states
2. Perform operations that could violate the invariant
3. Assert that the invariant still holds

### Property-Based Tests
Using `proptest`, generate random sequences of operations and verify all invariants hold after each operation.

### Snapshot Tests
Capture full contract state before and after critical operations to verify invariants in realistic scenarios.

## Implementation

Invariant checks are implemented in `contracts/escrow/src/invariants.rs` and called after every state-changing operation in the main contract.

```rust
#[cfg(test)]
mod invariants {
    pub fn check_all_invariants(env: &Env, contract: &EscrowContract) {
        check_token_balance_consistency(env, contract);
        check_state_transition_validity(env, contract);
        check_session_completion_bounds(env, contract);
        check_fund_conservation(env, contract);
        check_exclusive_distribution(env, contract);
        check_amount_non_negativity(env, contract);
        check_timestamp_consistency(env, contract);
    }
}
```

## Failure Modes

If any invariant is violated:
1. The contract panics with a descriptive error message
2. The transaction is rolled back
3. No state changes are persisted
4. The violation is logged for investigation

## Future Enhancements

- Formal verification using Coq or TLA+
- Automated invariant discovery using symbolic execution
- Runtime monitoring in production with alerts
