use soroban_sdk::{contracterror, contracttype, Address, String, Vec};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
/// Unified error enum for the escrow contract.
pub enum Error {
    // ── Participant / identity ─────────────────────────────────────────────
    /// `client` and `freelancer` must be distinct addresses.
    InvalidParticipant = 1,
    /// `arbiter` address overlaps with `client` or `freelancer`.
    InvalidArbiter = 2,
    /// An arbiter-requiring `ReleaseAuthorization` mode was selected but no arbiter was provided.
    MissingArbiter = 3,
    /// A contract participant address failed a role check.
    UnauthorizedRole = 4,

    // ── Milestone amount validation ────────────────────────────────────────
    /// Milestone list is empty.
    EmptyMilestones = 5,
    /// Too many milestones (exceeds [`MAX_MILESTONES`]).
    TooManyMilestones = 6,
    /// A milestone amount is zero or negative.
    InvalidMilestoneAmount = 7,
    /// The sum of all milestone amounts exceeds [`MAX_TOTAL_ESCROW_STROOPS`].
    TotalCapExceeded = 8,
    /// Checked arithmetic detected a potential i128 overflow.
    PotentialOverflow = 9,

    // ── Deposit validation ────────────────────────────────────────────────
    /// The deposit amount is zero or negative.
    InvalidDepositAmount = 10,
    /// Depositing this amount would push `total_deposited` above the contract total.
    DepositWouldExceedTotal = 11,

    // ── State machine ─────────────────────────────────────────────────────
    /// The referenced contract ID does not exist.
    ContractNotFound = 12,
    /// The contract is not in the required state for this operation.
    InvalidState = 13,

    // ── Milestone lifecycle ───────────────────────────────────────────────
    /// The milestone index is out of bounds.
    InvalidMilestone = 14,
    /// The milestone was already released.
    AlreadyReleased = 15,
    /// The milestone was already refunded.
    AlreadyRefunded = 16,
    /// The contract does not have enough funded balance.
    InsufficientFunds = 17,

    // ── Refund ───────────────────────────────────────────────────────────
    /// Refund request contains no milestone indices.
    EmptyRefundRequest = 18,
    /// The same milestone index appears more than once in a single refund request.
    DuplicateMilestoneInRefund = 19,

    // ── Approvals ─────────────────────────────────────────────────────────
    /// The required approval(s) are missing or were never submitted.
    InsufficientApprovals = 20,
    /// The approval record in temporary storage has expired (TTL elapsed).
    ApprovalExpired = 21,
    /// The caller already submitted an approval for this milestone.
    AlreadyApproved = 22,
    /// The milestone was already released (approval-time check).
    MilestoneAlreadyReleased = 23,

    // ── Misc ──────────────────────────────────────────────────────────────
    /// The amount supplied must be a positive value (> 0 stroops).
    AmountMustBePositive = 24,
    /// Accounting invariant violated (internal consistency check).
    AccountingInvariantViolated = 25,

    // ── Reputation ───────────────────────────────────────────────────────
    /// Rating value is outside the allowed range.
    InvalidRating = 26,
    /// Reputation token was already issued for this contract.
    ReputationAlreadyIssued = 27,
    /// The supplied freelancer address does not match the stored one.
    FreelancerMismatch = 28,

    // ── Additional error codes ───────────────────────────────────────────
    ContractIdCollision = 29,
    ContractIdOverflow = 30,
    IndexOutOfBounds = 31,
    AlreadyInitialized = 32,
    InsufficientAccumulatedFees = 33,
    AlreadyFinalized = 34,
    InvalidDisputeSplit = 35,
    NotCompleted = 36,
    SelfRating = 37,
    ContractPaused = 38,
    EmergencyActive = 39,
    InvalidStatusTransition = 40,
    NotInitialized = 41,
    TotalExceedsMaxEscrow = 42,
    FundingExceedsRequired = 43,
    InvalidParticipants = 44,
    InsufficientEscrowBalance = 45,
    MilestoneNotFound = 46,
    ExactDepositRequired = 47,
    InvalidProtocolParameters = 48,
    ArbiterRequired = 49,
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
    // Removed duplicate canonical error enum; using unified definition from errors.rs

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
    pub deadline: Option<u64>,
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

// ── Core contract state ──────────────────────────────────────────────────────

// ─── Storage keys ──────────────────────────────────────────────────────────────

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
    ReputationComment(u32),
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
    // Settlement token
    SettlementToken,
}

/// Canonical contract error type for all entrypoint-facing errors.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    /// The specified milestone index is out of bounds.
    IndexOutOfBounds = 3,
    /// The milestone has already been released.
    AlreadyReleased = 4,
    /// The refund request is empty.
    EmptyRefundRequest = 6,
    /// Duplicate milestone indices specified in the refund request.
    DuplicateMilestoneInRefund = 7,
    /// The milestone has already been refunded.
    AlreadyRefunded = 8,
    /// Insufficient funds available to perform the operation.
    InsufficientFunds = 9,
    /// The requested contract was not found.
    ContractNotFound = 10,
    /// The caller is not authorized for this operation.
    UnauthorizedRole = 11,
    /// The contract requires an arbiter address but none was provided.
    MissingArbiter = 12,
    /// The provided arbiter address is invalid (e.g. same as client or freelancer).
    InvalidArbiter = 13,
    /// The client and freelancer addresses are identical or invalid.
    InvalidParticipants = 14,
    /// The amount must be strictly greater than zero.
    AmountMustBePositive = 15,
    /// The contract is in an invalid state for this operation.
    InvalidState = 16,
    /// The milestone has already been released.
    MilestoneAlreadyReleased = 17,
    /// The milestone has already been approved.
    AlreadyApproved = 18,
    /// The milestone has not received sufficient approvals to release.
    InsufficientApprovals = 20,
    /// The freelancer address does not match the stored freelancer.
    FreelancerMismatch = 21,
    /// The rating value is outside the allowed range (1 to 5).
    InvalidRating = 22,
    /// Reputation has already been issued for this contract.
    ReputationAlreadyIssued = 23,
    /// The milestone list cannot be empty.
    EmptyMilestones = 25,
    /// The milestone amount is invalid.
    InvalidMilestoneAmount = 26,
    /// A contract with the specified ID already exists.
    ContractIdCollision = 27,
    /// The contract ID has overflowed the maximum limit.
    ContractIdOverflow = 28,
    /// The comment string is empty.
    EmptyComment = 29,
    /// The comment string exceeds the maximum length limit.
    CommentTooLong = 30,
    /// The participant address is invalid.
    InvalidParticipant = 31,
    /// The deposit amount is invalid.
    InvalidDepositAmount = 32,
    /// The milestone configuration is invalid.
    InvalidMilestone = 33,
    /// The contract has already been initialized.
    AlreadyInitialized = 34,
    /// Insufficient accumulated fees available for extraction.
    InsufficientAccumulatedFees = 35,
    /// The contract has not been initialized.
    NotInitialized = 36,
    /// The contract is currently paused.
    ContractPaused = 37,
    /// Emergency mode is currently active.
    EmergencyActive = 38,
    /// Self-rating is not allowed.
    SelfRating = 39,
    /// The contract has not been completed.
    NotCompleted = 40,
    /// The requested contract status transition is invalid.
    InvalidStatusTransition = 41,
    /// An arbiter is required for this operation.
    ArbiterRequired = 42,
    /// The dispute split percentage is invalid.
    InvalidDisputeSplit = 43,
    /// The operation would violate the core accounting invariant.
    AccountingInvariantViolated = 44,
    /// Checked arithmetic operation resulted in an overflow.
    PotentialOverflow = 45,
    /// The contract has already been finalized.
    AlreadyFinalized = 46,
    /// The work evidence string exceeds the maximum length limit.
    EvidenceTooLong = 47,
    /// The governance admin rotation timelock has not elapsed.
    TimelockNotElapsed = 48,
    /// The provided protocol parameters are invalid.
    InvalidProtocolParameters = 49,
    /// The contract's maximum escrowed amount cap would be exceeded.
    EscrowCapExceeded = 50,
    /// The milestone deadline has not yet elapsed (used by timeout refund paths).
    MilestoneNotOverdue = 51,
    /// The settlement token has already been bound and cannot be rebound.
    ///
    /// Set via `bind_settlement_token` / `set_settlement_token`. A second bind
    /// attempt panics with this error so the token identity is immutable for
    /// the lifetime of the contract — preventing an admin from migrating
    /// escrow balances to a different SAC mid-flight.
    SettlementTokenAlreadyBound = 52,
    /// A path that requires the settlement token to be bound was invoked
    /// before `bind_settlement_token` succeeded. Use `get_settlement_token`
    /// to verify bind state before mutating operations.
    SettlementTokenNotConfigured = 53,
}


/// Contract lifecycle states
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

/// Main escrow contract state
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Contract {
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub status: ContractStatus,
    pub total_deposited: i128,
    pub funded_amount: i128,
    pub released_amount: i128,
    pub refunded_amount: i128,
    pub release_authorization: ReleaseAuthorization,
    pub reputation_issued: bool,
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
    /// Optional Unix timestamp (seconds) after which the client may claim
    /// a timeout refund for this milestone without arbiter involvement.
    /// None means no deadline — the milestone never expires.
    pub deadline: Option<u64>,
}

/// Defines who can approve milestone releases.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseAuthorization {
    /// Only client can approve.
    ClientOnly = 0,
    /// Either client or arbiter can approve.
    ClientAndArbiter = 1,
    /// Only arbiter can approve.
    ArbiterOnly = 2,
    /// Both client and freelancer must approve; only either of them may release
    /// after both approvals are present.
    MultiSig = 3,
}

/// Tracks approval status for a milestone.
/// Stored in temporary storage with TTL for expiry grace period.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneApprovals {
    pub client_approved: bool,
    pub freelancer_approved: bool,
    pub arbiter_approved: bool,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DepositMode {
    ExactTotal = 0,
    Incremental = 1,
}

// ── Governance / readiness ───────────────────────────────────────────────────

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

/// Stores a pending governance admin proposal with the proposed address
/// and the ledger sequence when it was proposed.
/// Used for the admin rotation timelock mechanism.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingAdminProposal {
    pub proposed: Address,
    pub proposed_at_ledger: u32,
}

// ── Reputation ───────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct Reputation {
    pub completed_contracts: i128,
    pub total_rating: i128,
    pub last_rating: i128,
}

// ── Dispute Resolution ───────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeSplit {
    pub client_amount: i128,
    pub freelancer_amount: i128,
}

pub type SplitAmounts = DisputeSplit;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeResolution {
    FullRefund,
    PartialRefund,
    FullPayout,
    Split(DisputeSplit),
}

// ── Canonical contract error type ────────────────────────────────────────────
//
// The single authoritative `Error` enum lives in the first half of this file
// (see the top of the file). Earlier revisions accidentally re-declared it
// below; that duplicate was removed because it shadowed the canonical
// definition and prevented the contract from compiling.


