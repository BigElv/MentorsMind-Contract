# Requirements Document: Migration Telemetry Event

## Introduction

This feature implements structured event emission for V1 to V2 stream migrations in the Soroban smart contract. The migration event enables backend indexers to reliably link V1 and V2 stream records, track migration history, and maintain analytics continuity across contract upgrades.

## Glossary

- **Stream**: A payment stream contract managing recurring or time-locked payments
- **V1 Stream**: Original version of the stream contract
- **V2 Stream**: Upgraded version of the stream contract with enhanced features
- **Migration**: The process of upgrading a V1 stream to V2, transferring state and funds
- **MigrationEvent**: A structured event emitted during successful V1 to V2 migration
- **Indexer**: Backend service that listens to contract events and indexes them for querying
- **Atomic Execution**: Guarantee that all operations within a transaction succeed or all fail together
- **Sender**: The address initiating the migration (typically the stream owner)
- **Remaining Balance**: The amount of funds still available in the stream after migration

## Requirements

### Requirement 1: Define Migration Event Type

**User Story:** As a backend indexer, I want a structured MigrationEvent type, so that I can reliably parse and link V1 and V2 stream records.

#### Acceptance Criteria

1. THE MigrationEvent struct SHALL be defined with #[contracttype] and #[derive(Clone, Debug, Eq, PartialEq)] attributes
2. THE MigrationEvent struct SHALL contain the following fields:
   - v1_id: u64 (identifier of the V1 stream)
   - v2_id: u64 (identifier of the V2 stream)
   - sender: Address (address initiating the migration)
   - remaining_balance: i128 (amount of funds in the V2 stream after migration)
3. THE MigrationEvent struct SHALL be serializable for event emission
4. THE MigrationEvent struct SHALL be defined in the contract types module

### Requirement 2: Emit Migration Event During V1 to V2 Migration

**User Story:** As a backend indexer, I want migration events to be emitted during stream upgrades, so that I can track when streams are migrated and link the records.

#### Acceptance Criteria

1. WHEN a V1 stream is successfully migrated to V2, THE contract SHALL emit a MigrationEvent
2. THE emitted event SHALL have topic "migrate"
3. THE event data SHALL include v1_id, v2_id, sender, and remaining_balance
4. THE event emission SHALL occur after V1 state is finalized and V2 stream is created
5. THE event emission SHALL occur within the same transaction as the migration
6. IF migration fails at any point, THEN NO event SHALL be emitted

### Requirement 3: Ensure Atomic Migration Execution

**User Story:** As a contract user, I want migration to be atomic, so that the stream state remains consistent even if migration partially fails.

#### Acceptance Criteria

1. WHEN a migration is initiated, THE V1 state finalization, V2 stream creation, and event emission SHALL all occur within the same transaction
2. IF any step of the migration fails, THEN the entire transaction SHALL be rolled back
3. IF the transaction is rolled back, THEN NO event SHALL be emitted
4. THE event emission SHALL only occur after successful completion of all migration steps

### Requirement 4: Validate Migration Prerequisites

**User Story:** As a contract owner, I want the migration to validate all prerequisites, so that invalid migrations are prevented.

#### Acceptance Criteria

1. WHEN a migration is initiated, THE sender SHALL be authenticated via require_auth()
2. WHEN a migration is initiated, THE contract SHALL verify that the V1 stream exists
3. WHEN a migration is initiated, THE contract SHALL verify that the V2 stream is successfully created
4. WHEN a migration is initiated, THE contract SHALL verify that the remaining balance is correctly calculated
5. IF any validation fails, THEN the migration SHALL be rejected and no event SHALL be emitted

### Requirement 5: Event Structure for Indexer Compatibility

**User Story:** As a backend indexer, I want events in a specific format, so that I can reliably parse and process migration records.

#### Acceptance Criteria

1. THE event topic SHALL be the Symbol "migrate"
2. THE event data payload SHALL contain (v1_id, v2_id, sender, remaining_balance) in that order
3. THE event format SHALL allow indexers to link V1 and V2 streams using v1_id and v2_id
4. THE event format SHALL allow indexers to track migration history using sender and timestamp
5. THE event format SHALL allow indexers to maintain analytics continuity using remaining_balance

### Requirement 6: Test Migration Event Emission

**User Story:** As a developer, I want comprehensive tests for migration events, so that I can verify correct behavior.

#### Acceptance Criteria

1. WHEN a migration succeeds, THEN a test SHALL verify that the event is emitted
2. WHEN a migration succeeds, THEN a test SHALL verify that the event contains correct v1_id
3. WHEN a migration succeeds, THEN a test SHALL verify that the event contains correct v2_id
4. WHEN a migration succeeds, THEN a test SHALL verify that the event contains correct sender
5. WHEN a migration succeeds, THEN a test SHALL verify that the event contains correct remaining_balance
6. WHEN a migration fails, THEN a test SHALL verify that no event is emitted
7. WHEN a migration is executed, THEN a test SHALL verify atomic behavior (all-or-nothing)

### Requirement 7: Maintain Backward Compatibility

**User Story:** As a platform maintainer, I want the migration feature to not break existing functionality, so that current streams continue to work.

#### Acceptance Criteria

1. THE migration feature SHALL not modify existing V1 stream behavior
2. THE migration feature SHALL not modify existing V2 stream behavior
3. THE migration feature SHALL be additive and not remove any existing functionality
4. EXISTING tests for V1 and V2 streams SHALL continue to pass
