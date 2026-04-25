use soroban_sdk::{contracterror, contracttype, Address, Bytes, String};

#[contracttype]
pub enum DataKey {
    Client,
    Freelancer,
    Milestones,
    Initialized,
    MilestoneFunded(u32),
    ReputationIssued(u32),
    Reputation(Address),
    PendingReputationCredits(Address),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    IndexOutOfBounds = 3,
    AlreadyReleased = 4,
    InvalidStatusTransition = 5,
    InsufficientMilestoneFunding = 6,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    Created = 0,
    Funded = 1,
    Completed = 2,
    Disputed = 3,
    Cancelled = 4,
    Refunded = 5,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Milestone {
    pub amount: i128,
    pub released: bool,
    pub work_evidence: Option<String>,
    pub funded_amount: i128,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct MilestoneFunding {
    pub contract_id: u32,
    pub milestone_idx: u32,
    pub funded_amount: i128,
}

/// Reputation record for a freelancer.
/// Tracks aggregate rating data across all completed contracts.
#[contracttype]
#[derive(Clone, Debug, Default)]
pub struct ReputationRecord {
    /// Total sum of all ratings received.
    pub total_rating: i128,
    /// Number of ratings received.
    pub ratings_count: u32,
    /// Most recent rating given.
    pub last_rating: i128,
    /// Number of completed contracts.
    pub completed_contracts: u32,
}

