# Design Document: Migration Telemetry Event

## Overview

This design implements structured event emission for V1 to V2 stream migrations in Soroban smart contracts. The feature enables backend indexers to reliably link V1 and V2 stream records by emitting a `MigrationEvent` during the upgrade process. The event contains the V1 stream ID, V2 stream ID, sender address, and remaining balance, allowing indexers to track migration history and maintain analytics continuity.

## Architecture

### Event-Driven Migration Pattern

The migration process follows an event-driven architecture:

```
┌─────────────────────────────────────────────────────────────┐
│                    Migration Transaction                     │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  1. Authenticate Sender                                      │
│     └─> sender.require_auth()                               │
│                                                               │
│  2. Validate V1 Stream Exists                               │
│     └─> Retrieve V1 stream from storage                     │
│                                                               │
│  3. Finalize V1 State                                       │
│     └─> Mark V1 as migrated                                 │
│     └─> Preserve V1 data for audit trail                    │
│                                                               │
│  4. Create V2 Stream                                        │
│     └─> Initialize V2 with migrated state                  │
│     └─> Transfer remaining balance to V2                   │
│                                                               │
│  5. Emit MigrationEvent                                     │
│     └─> Topic: "migrate"                                    │
│     └─> Data: (v1_id, v2_id, sender, remaining_balance)   │
│                                                               │
│  6. Return Success                                          │
│     └─> Return V2 stream ID                                │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

### Atomicity Guarantee

All steps execute within a single Soroban transaction. If any step fails:
- The entire transaction is rolled back
- No event is emitted
- V1 stream remains unchanged
- V2 stream is not created

## Components and Interfaces

### 1. MigrationEvent Type

```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationEvent {
    pub v1_id: u64,
    pub v2_id: u64,
    pub sender: Address,
    pub remaining_balance: i128,
}
```

**Purpose**: Structured representation of a migration event for serialization and emission.

**Fields**:
- `v1_id`: Unique identifier of the V1 stream being migrated
- `v2_id`: Unique identifier of the newly created V2 stream
- `sender`: Address of the account initiating the migration
- `remaining_balance`: Amount of funds transferred to V2 stream

### 2. Migration Function Interface

```rust
pub fn migrate_stream(
    env: Env,
    sender: Address,
    v1_id: u64,
) -> u64  // Returns v2_id
```

**Purpose**: Execute V1 to V2 stream migration with event emission.

**Parameters**:
- `env`: Soroban environment for storage and event access
- `sender`: Address initiating the migration (must be authenticated)
- `v1_id`: Identifier of the V1 stream to migrate

**Returns**: Identifier of the newly created V2 stream

**Behavior**:
1. Authenticate sender via `sender.require_auth()`
2. Retrieve V1 stream from persistent storage
3. Validate V1 stream exists and is in migratable state
4. Calculate remaining balance
5. Finalize V1 state (mark as migrated)
6. Create V2 stream with migrated state
7. Emit MigrationEvent with all required data
8. Return V2 stream ID

### 3. Event Emission

```rust
let topic = Symbol::new(&env, "migrate");
let event = MigrationEvent {
    v1_id,
    v2_id,
    sender: sender.clone(),
    remaining_balance,
};
env.events().publish((topic,), event);
```

**Event Topic**: `"migrate"` (Symbol)

**Event Data**: Tuple containing MigrationEvent struct

**Emission Point**: After successful V2 stream creation, before function return

## Data Models

### V1 Stream State

```rust
#[contracttype]
#[derive(Clone, Debug)]
pub struct StreamV1 {
    pub id: u64,
    pub owner: Address,
    pub recipient: Address,
    pub amount: i128,
    pub start_time: u64,
    pub end_time: u64,
    pub claimed_amount: i128,
    pub status: StreamStatus,
}
```

### V2 Stream State

```rust
#[contracttype]
#[derive(Clone, Debug)]
pub struct StreamV2 {
    pub id: u64,
    pub owner: Address,
    pub recipient: Address,
    pub amount: i128,
    pub start_time: u64,
    pub end_time: u64,
    pub claimed_amount: i128,
    pub status: StreamStatus,
    pub v1_id: u64,  // Reference to original V1 stream
    pub migrated_at: u64,  // Timestamp of migration
}
```

### Migration State Transition

```
V1 Stream (Active)
    ↓
    ├─ Validate prerequisites
    ├─ Calculate remaining balance
    ├─ Finalize V1 (mark as Migrated)
    ├─ Create V2 (with v1_id reference)
    ├─ Emit MigrationEvent
    ↓
V1 Stream (Migrated) + V2 Stream (Active)
```

## Correctness Properties

A property is a characteristic or behavior that should hold true across all valid executions of a system—essentially, a formal statement about what the system should do. Properties serve as the bridge between human-readable specifications and machine-verifiable correctness guarantees.

### Property 1: Migration Event Emission

*For any* V1 stream that is successfully migrated, a MigrationEvent SHALL be emitted with the correct v1_id, v2_id, sender, and remaining_balance.

**Validates: Requirements 2.1, 2.2, 2.3, 2.4, 2.5**

### Property 2: Event Contains Correct V1 ID

*For any* migration of a V1 stream with ID `v1_id`, the emitted MigrationEvent SHALL contain that exact v1_id.

**Validates: Requirements 2.2, 5.2**

### Property 3: Event Contains Correct V2 ID

*For any* migration that creates a V2 stream with ID `v2_id`, the emitted MigrationEvent SHALL contain that exact v2_id.

**Validates: Requirements 2.2, 5.2**

### Property 4: Event Contains Correct Sender

*For any* migration initiated by a sender address, the emitted MigrationEvent SHALL contain that exact sender address.

**Validates: Requirements 2.2, 4.1, 5.2**

### Property 5: Event Contains Correct Remaining Balance

*For any* V1 stream with remaining balance `B`, after successful migration, the emitted MigrationEvent SHALL contain that exact remaining_balance value.

**Validates: Requirements 2.2, 4.4, 5.2**

### Property 6: No Event on Failed Migration

*For any* migration that fails (due to validation error, insufficient balance, or other failure), NO MigrationEvent SHALL be emitted.

**Validates: Requirements 2.6, 3.2, 3.3**

### Property 7: Atomic Migration Behavior

*For any* migration transaction, either all steps succeed and an event is emitted, OR the entire transaction is rolled back and no event is emitted. There is no partial success state.

**Validates: Requirements 3.1, 3.2, 3.3, 3.4**

### Property 8: V1 Stream Validation

*For any* migration attempt with a non-existent V1 stream ID, the migration SHALL fail and no event SHALL be emitted.

**Validates: Requirements 4.2, 4.5**

### Property 9: Sender Authentication

*For any* migration, the sender address SHALL be authenticated via `require_auth()` before any state changes occur.

**Validates: Requirements 4.1, 4.5**

### Property 10: V2 Stream Creation Success

*For any* successful migration, a V2 stream SHALL be created with the migrated state and the same remaining balance as the V1 stream.

**Validates: Requirements 4.3, 4.4**

## Error Handling

### Migration Failures

The following conditions cause migration to fail (transaction rollback, no event emission):

1. **Sender Not Authenticated**: `require_auth()` fails
   - Error: "Sender not authorized"
   - Action: Reject migration

2. **V1 Stream Not Found**: V1 stream ID does not exist
   - Error: "V1 stream not found"
   - Action: Reject migration

3. **V1 Stream Already Migrated**: V1 stream status is already Migrated
   - Error: "V1 stream already migrated"
   - Action: Reject migration

4. **Insufficient Balance**: Remaining balance calculation fails
   - Error: "Insufficient balance for migration"
   - Action: Reject migration

5. **V2 Creation Failed**: V2 stream creation fails
   - Error: "Failed to create V2 stream"
   - Action: Reject migration (transaction rollback)

6. **Event Emission Failed**: Event emission fails (rare)
   - Error: "Failed to emit migration event"
   - Action: Reject migration (transaction rollback)

### Error Recovery

All errors result in transaction rollback. No partial state changes occur. Users can retry migration after addressing the underlying issue.

## Testing Strategy

### Unit Tests

Unit tests verify specific examples and edge cases:

1. **Successful Migration**: Verify event is emitted with correct data
2. **V1 Stream Not Found**: Verify migration fails gracefully
3. **Already Migrated**: Verify cannot migrate twice
4. **Sender Not Authenticated**: Verify authorization check
5. **Balance Calculation**: Verify remaining_balance is correct
6. **Event Data Integrity**: Verify all event fields are correct

### Property-Based Tests

Property-based tests verify universal properties across many generated inputs:

1. **Property 1: Migration Event Emission** - For all valid V1 streams, migration emits event
2. **Property 2: Event Contains Correct V1 ID** - For all migrations, event contains correct v1_id
3. **Property 3: Event Contains Correct V2 ID** - For all migrations, event contains correct v2_id
4. **Property 4: Event Contains Correct Sender** - For all migrations, event contains correct sender
5. **Property 5: Event Contains Correct Balance** - For all migrations, event contains correct remaining_balance
6. **Property 6: No Event on Failed Migration** - For all failed migrations, no event is emitted
7. **Property 7: Atomic Behavior** - For all migrations, atomicity is maintained
8. **Property 8: V1 Validation** - For all invalid V1 IDs, migration fails
9. **Property 9: Sender Authentication** - For all migrations, sender is authenticated
10. **Property 10: V2 Creation Success** - For all successful migrations, V2 is created correctly

### Test Configuration

- **Minimum iterations per property test**: 100
- **Test framework**: Soroban SDK test utilities
- **Coverage target**: 100% of migration code paths

## Implementation Notes

### Storage Considerations

- V1 streams remain in storage after migration (for audit trail)
- V2 streams reference their V1 counterpart via `v1_id` field
- Migration timestamp is recorded in V2 stream

### Performance Considerations

- Single transaction ensures atomicity
- No additional network calls required
- Event emission is O(1) operation
- Storage operations are minimal

### Security Considerations

- Sender authentication via `require_auth()` prevents unauthorized migrations
- Validation checks prevent invalid state transitions
- Atomic execution prevents partial failures
- Event immutability ensures audit trail integrity

### Future Enhancements

- Batch migration support for multiple streams
- Migration rollback capability (if needed)
- Migration fee collection
- Migration history queries
