# PR: Add Bounty Contract

## Summary

Implements a USDC bounty contract allowing anyone to post a reward for learners who complete a specific skill challenge. The reward is released when a verified mentor confirms completion.

## Changes

- `contracts/bounty/src/lib.rs` — full bounty contract implementation
- `contracts/bounty/Cargo.toml` — package manifest
- `Cargo.toml` — added `contracts/bounty` to workspace members

## Contract: `mentorminds-bounty`

### Entry Points

| Function | Auth | Description |
|---|---|---|
| `initialize(env, admin, verification_contract)` | — | One-time setup |
| `post_bounty(env, poster, skill, description_hash, reward, token, deadline)` | poster | Posts bounty, pulls tokens into contract, returns bounty ID |
| `claim_bounty(env, learner, bounty_id)` | learner | Learner signals completion; multiple learners can claim the same bounty |
| `verify_completion(env, mentor, bounty_id, learner)` | verified mentor | Confirms completion, releases reward to learner; first verified claim wins |
| `dispute_completion(env, bounty_id, learner)` | poster | Disputes a pending claim within 48h of the claim |
| `refund_bounty(env, bounty_id)` | poster | Reclaims reward after deadline with no verified claim |
| `get_bounty(env, id)` | — | Returns `BountyRecord` |
| `get_claim(env, bounty_id, learner)` | — | Returns `ClaimRecord` |
| `get_bounty_count(env)` | — | Returns total bounties posted |

### State Machine

```
Open → Claimed → Verified  (reward released to learner)
     ↘          ↘ Disputed  (poster disputed within 48h)
       → Refunded           (deadline passed, no verified claim)
```

### Events Emitted

| Topic | Data |
|---|---|
| `bounty / posted` | `BountyPostedEvent { id, poster, skill, reward, deadline }` |
| `bounty / claimed` | `BountyClaimedEvent { bounty_id, learner, claimed_at }` |
| `bounty / verified` | `BountyVerifiedEvent { bounty_id, learner, mentor, reward }` |
| `bounty / disputed` | `BountyDisputedEvent { bounty_id, learner, disputed_at }` |
| `bounty / refunded` | `BountyRefundedEvent { bounty_id, poster, reward }` |

### Key Design Decisions

- Mentor verification is enforced via cross-contract call to the existing `VerificationContract` (`is_verified`)
- Multiple learners can claim the same bounty; the first one a verified mentor approves wins
- Dispute window is fixed at 48 hours from claim timestamp
- Reward tokens are held by the contract until `verify_completion` or `refund_bounty`
- TTL bumps applied on all persistent storage writes for ledger entry longevity

## Tests

Unit tests cover all acceptance criteria:

- `test_post_bounty` — tokens transferred to contract, record stored
- `test_claim_bounty` — status transitions to `Claimed`, claim record created
- `test_verify_completion_releases_reward` — reward sent to learner, status `Verified`
- `test_multiple_learners_first_verified_wins` — second learner claim doesn't affect winner
- `test_verify_twice_panics` — double verify rejected
- `test_dispute_completion` — status transitions to `Disputed`
- `test_dispute_after_window_panics` — dispute rejected past 48h
- `test_refund_after_deadline` — poster reclaims after deadline
- `test_refund_before_deadline_panics` — early refund rejected
- `test_claim_after_deadline_panics` — late claim rejected
- `test_double_claim_panics` — duplicate claim rejected

## Linked Issue

Closes #[ISSUE_NUMBER] — Bounty contract for skill challenge incentives

## Checklist

- [x] Contract compiles (`cargo build -p mentorminds-bounty`)
- [x] All unit tests pass
- [x] Events emitted for all state transitions
- [x] Auth enforced on all mutating calls
- [x] No `std` (pure `no_std` Soroban contract)
- [x] TTL bumps on persistent storage
- [x] Added to workspace `Cargo.toml`
