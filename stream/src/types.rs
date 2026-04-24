use soroban_sdk::{contracttype, Address};

/// Status of a stream
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StreamStatus {
    Active,
    Paused,
    Completed,
    Cancelled,
    Migrated,
}

/// V1 Stream data structure
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

/// V2 Stream data structure with migration support
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
    pub v1_id: u64,
    pub migrated_at: u64,
}

/// Migration event emitted when a stream is upgraded from V1 to V2
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationEvent {
    pub v1_id: u64,
    pub v2_id: u64,
    pub sender: Address,
    pub remaining_balance: i128,
}
