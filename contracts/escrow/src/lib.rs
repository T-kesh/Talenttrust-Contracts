#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, BytesN, Env,
    Symbol, Vec,
};

mod ttl;

pub use ttl::{
    LEDGERS_PER_DAY, PENDING_APPROVAL_BUMP_THRESHOLD, PENDING_APPROVAL_TTL_LEDGERS,
    PENDING_MIGRATION_BUMP_THRESHOLD, PENDING_MIGRATION_TTL_LEDGERS,
};

mod types;
pub use types::{ContractStatus, Milestone};

// ─── Bounds constants ─────────────────────────────────────────────────────────
//
// Policy decision: bounds are HARD-CODED for the initial release rather than
// governed on-chain. Rationale:
//   • Governance machinery adds upgrade-path complexity and new attack surface.
//   • Hard limits give the strongest security guarantee with zero runtime cost.
//   • A future governance proposal can introduce adjustable parameters if
//     operational experience shows the defaults need revisiting.
//
// MAX_MILESTONES: limits worst-case per-contract storage and loop cost.
//   10 milestones covers the overwhelming majority of real freelance contracts.
//
// MAX_TOTAL_ESCROW_STROOPS: caps the maximum value locked in a single contract
//   to 1 000 000 tokens (7-decimal stroops) to bound worst-case griefing impact.

/// Maximum number of milestones allowed per contract.
pub const MAX_MILESTONES: u32 = 10;

/// Hard cap on the total escrow value per contract, in stroops (7 decimal places).
/// Equals 1 000 000 tokens.
pub const MAX_TOTAL_ESCROW_STROOPS: i128 = 1_000_000_0000000; // 1 M tokens × 10^7 = 10^13

pub const MAINNET_PROTOCOL_VERSION: u32 = 1u32;
pub const MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS: i128 = 1_000_000_000_000_000i128;

#[contract]
pub struct Escrow;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    /// Attempted to release a milestone that has already been released.
    MilestoneAlreadyReleased = 12,
    /// Attempted to refund a milestone that has already been refunded.
    MilestoneAlreadyRefunded = 13,
    /// The refund request list is empty.
    EmptyRefundRequest = 14,
    /// The same milestone index appears more than once in a single refund call.
    DuplicateMilestoneInRefund = 15,
    /// The escrow balance is insufficient to cover the requested refund.
    InsufficientEscrowBalance = 16,
    /// The client address is identical to the freelancer address.
    /// Validation is fail-closed: any overlap between principal roles is rejected.
    ClientEqualsFreelancer = 17,
    /// The arbiter address overlaps with the client or freelancer address.
    /// An arbiter must be a fully independent third party.
    ArbiterRoleOverlap = 18,
}

/// Per-contract storage record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowContractData {
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    /// Milestone list.  Index matches milestone index.
    pub milestones: Vec<Milestone>,
    pub status: ContractStatus,
    /// Cumulative amount deposited into escrow.
    pub total_deposited: i128,
    /// Cumulative amount released to the freelancer.
    pub released_amount: i128,
    /// Cumulative amount refunded to the client.
    /// Invariant: total_deposited == released_amount + refunded_amount + available_balance
    pub refunded_amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingApproval {
    pub approver: Address,
    pub contract_id: u32,
    pub requested_at_ledger: u32,
    pub expires_at_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingMigration {
    pub proposer: Address,
    pub new_wasm_hash: BytesN<32>,
    pub requested_at_ledger: u32,
    pub expires_at_ledger: u32,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Contract(u32),
    ContractCount,
}

// ─── Identity validation ──────────────────────────────────────────────────────

/// Validate that the three principal roles carry distinct identities.
///
/// # Rules (fail-closed — any violation panics)
///
/// 1. `client` ≠ `freelancer`  
///    The two primary counterparties must be different accounts.  Allowing the
///    same address in both roles would let a single party self-approve milestone
///    releases and collect its own escrow funds.
///
/// 2. `arbiter` ≠ `client` and `arbiter` ≠ `freelancer` (when arbiter is `Some`)  
///    An arbiter is a neutral third party.  If the arbiter address overlaps with
///    either primary role the arbiter could unilaterally cancel or resolve
///    disputes in their own favour.
///
/// # When to call
/// Call this helper at the very start of `create_contract` (before any storage
/// writes) so that invalid identity combinations are rejected atomically and no
/// partial state is ever committed.
///
/// The same helper should be called from any future migration path that
/// reconstructs or re-validates contract participants.
pub fn validate_participant_identities(
    env: &Env,
    client: &Address,
    freelancer: &Address,
    arbiter: &Option<Address>,
) {
    // Rule 1: client and freelancer must be distinct.
    if client == freelancer {
        env.panic_with_error(EscrowError::ClientEqualsFreelancer);
    }

    // Rule 2: arbiter (if present) must not overlap with either primary role.
    if let Some(ref a) = arbiter {
        if a == client || a == freelancer {
            env.panic_with_error(EscrowError::ArbiterRoleOverlap);
        }
    }
}

#[contractimpl]
impl Escrow {
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }

    /// Create a new escrow contract.
    ///
    /// * `client`     – the party funding the escrow.
    /// * `freelancer` – the party receiving milestone payments.
    /// * `arbiter`    – optional dispute-resolution address; must be distinct
    ///                  from both `client` and `freelancer`.
    /// * `milestones` – list of milestone amounts (stroops, must be > 0).
    ///
    /// Returns the new contract ID (monotonically increasing u32).
    ///
    /// # Identity constraints (enforced via `validate_participant_identities`)
    /// - `client` ≠ `freelancer`
    /// - `arbiter` ≠ `client` and `arbiter` ≠ `freelancer` (when provided)
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        arbiter: Option<Address>,
        milestone_amounts: Vec<i128>,
    ) -> u32 {
        client.require_auth();

        // Identity sanitization — fail-closed, called before any storage writes.
        validate_participant_identities(&env, &client, &freelancer, &arbiter);

        if milestone_amounts.is_empty() {
            env.panic_with_error(EscrowError::EmptyMilestones);
        }
        if milestone_amounts.len() > MAX_MILESTONES {
            env.panic_with_error(EscrowError::TooManyMilestones);
        }

        let mut milestones: Vec<Milestone> = Vec::new(&env);
        for amount in milestone_amounts.iter() {
            if amount <= 0 {
                env.panic_with_error(EscrowError::InvalidMilestoneAmount);
            }
            milestones.push_back(Milestone {
                amount,
                released: false,
                refunded: false,
                work_evidence: None,
                funded_amount: 0,
                refunded_amount: 0,
            });
        }

        let id: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ContractCount)
            .unwrap_or(0u32);

        let data = EscrowContractData {
            client,
            freelancer,
            arbiter,
            milestones,
            status: ContractStatus::Created,
            total_deposited: 0,
            released_amount: 0,
            refunded_amount: 0,
        };

        env.storage().persistent().set(&DataKey::Contract(id), &data);
        env.storage().persistent().set(&DataKey::ContractCount, &(id + 1));

        id
    }

    /// Deposit funds into the escrow.  Transitions status from Created → Funded
    /// once the deposited amount reaches the sum of all milestone amounts.
    pub fn deposit_funds(env: Env, contract_id: u32, amount: i128) -> bool {
        if amount <= 0 {
            env.panic_with_error(EscrowError::InvalidDepositAmount);
        }

        let contract_key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, EscrowContractData>(&contract_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        contract.total_deposited += amount;

        let total_milestone_amount: i128 = contract.milestones.iter().map(|m| m.amount).sum();

        if contract.status == ContractStatus::Created
            && contract.total_deposited >= total_milestone_amount
        {
            contract.status = ContractStatus::Funded;
        }

        env.storage().persistent().set(&contract_key, &contract);

        true
    }

    /// Release a single milestone to the freelancer.
    pub fn release_milestone(env: Env, contract_id: u32, milestone_index: u32) -> bool {
        let contract_key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, EscrowContractData>(&contract_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        let milestone = contract
            .milestones
            .get(milestone_index)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::InvalidMilestone));

        if milestone.released {
            env.panic_with_error(EscrowError::MilestoneAlreadyReleased);
        }
        if milestone.refunded {
            env.panic_with_error(EscrowError::MilestoneAlreadyRefunded);
        }

        let available = Self::available_balance(&contract);
        if available < milestone.amount {
            env.panic_with_error(EscrowError::InsufficientEscrowBalance);
        }

        let mut updated = milestone.clone();
        updated.released = true;
        contract.milestones.set(milestone_index, updated);
        contract.released_amount += milestone.amount;

        if Self::all_milestones_settled(&contract) {
            contract.status = ContractStatus::Completed;
        }

        env.storage().persistent().set(&contract_key, &contract);

        env.events().publish(
            (Symbol::new(&env, "milestone_released"), contract_id),
            (milestone_index, milestone.amount, env.ledger().timestamp()),
        );

        true
    }

    // ─── Partial-refund API ───────────────────────────────────────────────────

    /// Refund one or more unreleased milestones back to the client.
    pub fn refund_milestone(
        env: Env,
        contract_id: u32,
        milestone_ids: Vec<u32>,
    ) -> i128 {
        if milestone_ids.is_empty() {
            env.panic_with_error(EscrowError::EmptyRefundRequest);
        }

        let len = milestone_ids.len();
        for i in 0..len {
            for j in (i + 1)..len {
                if milestone_ids.get(i).unwrap() == milestone_ids.get(j).unwrap() {
                    env.panic_with_error(EscrowError::DuplicateMilestoneInRefund);
                }
            }
        }

        let contract_key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, EscrowContractData>(&contract_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        let mut total_refund: i128 = 0;
        for idx in milestone_ids.iter() {
            let milestone = contract
                .milestones
                .get(idx)
                .unwrap_or_else(|| env.panic_with_error(EscrowError::InvalidMilestone));

            if milestone.released {
                env.panic_with_error(EscrowError::MilestoneAlreadyReleased);
            }
            if milestone.refunded {
                env.panic_with_error(EscrowError::MilestoneAlreadyRefunded);
            }
            total_refund += milestone.amount;
        }

        let available = Self::available_balance(&contract);
        if available < total_refund {
            env.panic_with_error(EscrowError::InsufficientEscrowBalance);
        }

        for idx in milestone_ids.iter() {
            let mut milestone = contract.milestones.get(idx).unwrap();
            milestone.refunded = true;
            milestone.refunded_amount = milestone.amount;
            contract.milestones.set(idx, milestone.clone());
            contract.refunded_amount += milestone.amount;

            env.events().publish(
                (Symbol::new(&env, "milestone_refunded"), contract_id),
                (idx, milestone.amount, env.ledger().timestamp()),
            );
        }

        if Self::all_milestones_settled(&contract) {
            contract.status = ContractStatus::Refunded;
        }

        env.storage().persistent().set(&contract_key, &contract);

        env.events().publish(
            (Symbol::new(&env, "contract_refunded"), contract_id),
            (total_refund, contract.refunded_amount, env.ledger().timestamp()),
        );

        total_refund
    }

    /// Returns the current available escrow balance for a contract.
    pub fn get_refundable_balance(env: Env, contract_id: u32) -> i128 {
        let contract = env
            .storage()
            .persistent()
            .get::<_, EscrowContractData>(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        Self::available_balance(&contract)
    }

    // ─── Query helpers ────────────────────────────────────────────────────────

    /// Get the full contract record.
    pub fn get_contract(env: Env, contract_id: u32) -> EscrowContractData {
        env.storage()
            .persistent()
            .get::<_, EscrowContractData>(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound))
    }

    /// Get the milestone list for a contract.
    pub fn get_milestones(env: Env, contract_id: u32) -> Vec<Milestone> {
        let contract = Self::get_contract(env, contract_id);
        contract.milestones
    }

    // ─── Cancel ───────────────────────────────────────────────────────────────

    /// Cancel an escrow contract under strict authorization and state constraints.
    pub fn cancel_contract(env: Env, contract_id: u32, caller: Address) -> bool {
        caller.require_auth();

        let contract_key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, EscrowContractData>(&contract_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        if contract.status == ContractStatus::Cancelled {
            env.panic_with_error(EscrowError::AlreadyCancelled);
        }

        if contract.status == ContractStatus::Completed {
            env.panic_with_error(EscrowError::InvalidStatusTransition);
        }

        let is_client = caller == contract.client;
        let is_freelancer = caller == contract.freelancer;
        let is_arbiter = contract.arbiter.as_ref().is_some_and(|a| *a == caller);

        match contract.status {
            ContractStatus::Created => {
                if !is_client && !is_freelancer {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
            ContractStatus::Funded => {
                if is_client {
                    if contract.released_amount > 0 {
                        env.panic_with_error(EscrowError::MilestonesAlreadyReleased);
                    }
                } else if is_freelancer {
                    // Freelancer can cancel (economic deterrent – funds return to client).
                } else if is_arbiter {
                    // Arbiter can cancel in funded state (dispute resolution).
                } else {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
            ContractStatus::Disputed => {
                if !is_arbiter {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
            _ => {
                env.panic_with_error(EscrowError::InvalidStatusTransition);
            }
        }

        contract.status = ContractStatus::Cancelled;
        env.storage().persistent().set(&contract_key, &contract);

        env.events().publish(
            (Symbol::new(&env, "contract_cancelled"), contract_id),
            (caller, contract.status, env.ledger().timestamp()),
        );

        true
    }

    // ─── Private helpers ──────────────────────────────────────────────────────

    fn available_balance(contract: &EscrowContractData) -> i128 {
        contract.total_deposited - contract.released_amount - contract.refunded_amount
    }

    fn all_milestones_settled(contract: &EscrowContractData) -> bool {
        for m in contract.milestones.iter() {
            if !m.released && !m.refunded {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_refund_milestone;
