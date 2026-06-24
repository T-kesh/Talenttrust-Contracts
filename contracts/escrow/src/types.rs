use soroban_sdk::{contracterror, contracttype, Address, String, Vec};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    InvalidParticipants = 1,
    MissingArbiter = 2,
    InvalidArbiter = 3,
    EmptyMilestones = 4,
    InvalidMilestoneAmount = 5,
    ContractIdCollision = 6,
    ContractIdOverflow = 7,
    AmountMustBePositive = 8,
    ContractNotFound = 9,
    UnauthorizedRole = 10,
    InvalidState = 11,
    IndexOutOfBounds = 12,
    MilestoneAlreadyReleased = 13,
    AlreadyRefunded = 14,
    InsufficientFunds = 15,
    EmptyRefundRequest = 16,
    DuplicateMilestoneInRefund = 17,
    AlreadyApproved = 18,
    InsufficientApprovals = 19,
    FreelancerMismatch = 20,
    InvalidRating = 21,
    ReputationAlreadyIssued = 22,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Contract {
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub status: ContractStatus,
    pub funded_amount: i128,
    pub released_amount: i128,
    pub refunded_amount: i128,
    pub release_authorization: ReleaseAuthorization,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneApprovals {
    pub client_approved: bool,
    pub freelancer_approved: bool,
    pub arbiter_approved: bool,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseAuthorization {
    ClientOnly,
    ArbiterOnly,
    ClientAndArbiter,
    MultiSig,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    // Admin / pause / emergency
    Initialized,
    Admin,
    Paused,
    Emergency,
    // Contract storage
    Contract(u32),
    NextContractId,
    MilestoneReleased(u32, u32),
    MilestoneApprovals(u32, u32),
    // Reputation
    ReputationIssued(u32),
    PendingReputationCredits(Address),
    Reputation(Address),
    // Client migration
    PendingClientMigration(u32),
    // Protocol / governance
    GovernanceAdmin,
    PendingGovernanceAdmin,
    ProtocolParameters,
    ProtocolFeeBps,
    // Two-step admin transfer: pending admin stored here while proposal awaits acceptance
    PendingAdmin,
    AccumulatedProtocolFees,
    GovernedParameters,
    ReadinessChecklist,
    // Finalization
    Finalization(u32),
}

/// Canonical contract error type for all entrypoint-facing errors.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    InvalidParticipant = 1,
    EmptyMilestones = 2,
    InvalidMilestoneAmount = 3,
    InvalidDepositAmount = 4,
    InvalidMilestone = 5,
    UnauthorizedRole = 6,
    InvalidStatusTransition = 7,
    AlreadyCancelled = 8,
    ContractNotFound = 9,
    MilestoneAlreadyReleased = 10,
    TooManyMilestones = 11,
    NotCompleted = 12,
    InvalidRating = 13,
    AlreadyFinalized = 14,
    AlreadyReleased = 15,
    InsufficientFunds = 16,
    SelfRating = 17,
    AmountMustBePositive = 18,
    InvalidState = 19,
    AlreadyApproved = 20,
    ReputationAlreadyIssued = 21,
    ContractPaused = 22,
    EmergencyActive = 23,
    NotInitialized = 24,
    AlreadyInitialized = 25,
    FreelancerMismatch = 26,
    EmptyRefundRequest = 27,
    DuplicateMilestoneInRefund = 28,
    MissingArbiter = 29,
    InvalidArbiter = 30,
    ContractIdOverflow = 31,
    ContractIdCollision = 32,
    IndexOutOfBounds = 33,
    AlreadyRefunded = 34,
    InsufficientApprovals = 35,
    ApprovalExpired = 36,
    Refunded = 37,
    InsufficientAccumulatedFees = 38,
}

/// Alias kept for external test / migration code that references
/// `EscrowError`.  Will be removed once all callers migrate to `Error`.
pub type EscrowError = Error;

#[contracttype]
#[derive(Clone, Debug)]
pub struct Contract {
    pub client: soroban_sdk::Address,
    pub freelancer: soroban_sdk::Address,
    pub arbiter: Option<soroban_sdk::Address>,
    pub status: ContractStatus,
    pub funded_amount: i128,
    pub released_amount: i128,
    pub refunded_amount: i128,
    pub release_authorization: ReleaseAuthorization,
}

/// Defines who can approve milestone releases
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseAuthorization {
    /// Only client can approve
    ClientOnly = 0,
    /// Either client or arbiter can approve
    ClientAndArbiter = 1,
    /// Only arbiter can approve
    ArbiterOnly = 2,
    /// Both client and freelancer must approve (multi-signature)
    MultiSig = 3,
}

/// Tracks approval status for a milestone
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneApprovals {
    pub client_approved: bool,
    pub freelancer_approved: bool,
    pub arbiter_approved: bool,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    Created = 0,
    Accepted = 1,
    Funded = 2,
    Completed = 3,
    Disputed = 4,
    Cancelled = 5,
    Refunded = 6,
    PartiallyFunded = 7,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    pub amount: i128,
    pub funded_amount: i128,
    pub released: bool,
    pub refunded: bool,
    pub work_evidence: Option<String>,
    pub refunded_amount: i128,
}

/// Readiness checklist stored under [`DataKey::ReadinessChecklist`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadinessChecklist {
    /// `true` after `initialize` has been called successfully.
    pub initialized: bool,
    /// `true` after protocol governance parameters have been set.
    pub governed_params_set: bool,
    /// `true` after an emergency control operation has been invoked.
    pub emergency_controls_enabled: bool,
}

impl Default for ReadinessChecklist {
    fn default() -> Self {
        ReadinessChecklist {
            initialized: false,
            governed_params_set: false,
            emergency_controls_enabled: false,
        }
    }
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GovernedParameters {
    pub protocol_fee_bps: u32,
    pub max_escrow_total_stroops: i128,
}

// ─── Indexer summary types ────────────────────────────────────────────────────

#[allow(dead_code)]
pub const CONTRACT_SUMMARY_SCHEMA_VERSION: u32 = 1;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneSummary {
    pub index: u32,
    pub amount: i128,
    pub released: bool,
    pub refunded: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractSummary {
    pub schema_version: u32,
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub status: ContractStatus,
    pub reputation_issued: bool,
    pub total_amount: i128,
    pub funded_amount: i128,
    pub released_amount: i128,
    pub refundable_balance: i128,
    pub released_milestone_count: u32,
    pub milestones: Vec<MilestoneSummary>,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DepositMode {
    ExactTotal = 0,
    Incremental = 1,
}
