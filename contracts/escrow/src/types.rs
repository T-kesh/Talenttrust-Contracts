use soroban_sdk::{contracterror, contracttype, Address, Bytes, String, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Contract(u32),
    ContractCount,
    Milestones(u32),
    MilestoneReleased(u32, u32),
    MilestoneApprovalTime(u32, u32),
    RefundableBalance(u32),
    Reputation(u32, Address),
    PendingReputation(Address),
    ReputationRecord(Address),
    ReadinessChecklist,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EscrowError {
    InvalidParticipant = 1,
    EmptyMilestones = 2,
    InvalidMilestoneAmount = 3,
    InvalidDepositAmount = 4,
    InvalidMilestone = 5,
    UnauthorizedRole = 6,
    InvalidStatusTransition = 7,
    AlreadyCancelled = 8,
    ContractNotFound = 9,
    MilestonesAlreadyReleased = 10,
    TooManyMilestones = 11,
    NotCompleted = 12,
    InvalidRating = 13,
    DuplicateRating = 14,
    AlreadyFinalized = 15,
    NotReadyForFinalization = 16,
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    pub amount: i128,
    pub released: bool,
    pub refunded: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowContractData {
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub milestones: Vec<Milestone>,
    pub status: ContractStatus,
    pub total_amount: i128,
    pub funded_amount: i128,
    pub released_amount: i128,
    pub refunded_amount: i128,
    pub finalized: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationRecord {
    pub completed_contracts: u32,
    pub total_rating: u32,
    pub last_rating: u32,
    pub ratings_count: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowBounds {
    pub max_milestones: u32,
    pub max_total_escrow_stroops: i128,
}

#[contracttype]
#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub struct ReadinessChecklist {
    pub storage_optimized: bool,
    pub events_indexed: bool,
    pub security_audited: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MainnetReadinessInfo {
    pub protocol_version: u32,
    pub checklist: ReadinessChecklist,
}
