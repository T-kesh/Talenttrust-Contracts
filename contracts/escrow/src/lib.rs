#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Bytes, BytesN,
    Env, Symbol, Vec,
};

mod ttl;

pub use ttl::{
    LEDGERS_PER_DAY, PENDING_APPROVAL_BUMP_THRESHOLD, PENDING_APPROVAL_TTL_LEDGERS,
    PENDING_MIGRATION_BUMP_THRESHOLD, PENDING_MIGRATION_TTL_LEDGERS,
};

use crate::types::ContractStatus;
#![allow(clippy::derivable_impls)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::redundant_field_names)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::useless_vec)]
#![allow(clippy::let_and_return)]
#![allow(clippy::inconsistent_digit_grouping)]
#![allow(clippy::int_plus_one)]
#![allow(clippy::duplicated_attributes)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::bool_assert_comparison)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::clone_on_copy)]
#![allow(clippy::module_inception)]
#![allow(clippy::single_match)]
#![allow(clippy::useless_conversion)]

mod types;
mod ttl;
mod approvals;

pub use types::{Contract, ContractStatus, DataKey, Error, Milestone, MilestoneApprovals, ReleaseAuthorization};

pub use crate::types::{MainnetReadinessInfo, ReadinessChecklist};
use crate::types::DataKey as ReadinessDataKey;
use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Symbol, Vec};

#[contract]
pub struct Escrow;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EscrowError {
    InvalidParticipant = 1,
    EmptyMilestones = 2,
    TooManyMilestones = 3,
    InvalidMilestoneAmount = 4,
    InvalidDepositAmount = 5,
    InvalidMilestone = 6,
    UnauthorizedRole = 7,
    InvalidStatusTransition = 8,
    AlreadyCancelled = 9,
    ContractPaused = 10,
    ArbiterAlreadyAssigned = 11,
    ContractNotFound = 12,
    MilestonesAlreadyReleased = 13,
    NoLeftoverFunds = 14,
    InvalidMilestoneAmount = 3,
    InvalidDepositAmount = 4,
    InvalidMilestone = 5,
    ContractNotFound = 6,
    EmptyRefundRequest = 7,
    DuplicateMilestoneInRefund = 8,
    AlreadyReleased = 9,
    AlreadyRefunded = 10,
    InsufficientFunds = 11,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractData {
    pub client: Address,
    pub freelancer: Address,
    pub milestones: Vec<i128>,
    pub status: ContractStatus,
    pub total_deposited: i128,
    pub released_amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowBounds {
    pub max_milestones: u32,
    pub max_total_escrow_stroops: i128,
}

type ContractData = EscrowContractData;

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
    Milestones(u32),
    MilestoneApprovalTime(u32, u32),
    MilestoneReleased(u32, u32),
    RefundableBalance(u32),
    ContractCount,
    Paused,
}

fn update_readiness_checklist<F>(env: &Env, f: F)
where
    F: FnOnce(&mut ReadinessChecklist),
{
    let mut checklist: ReadinessChecklist = env
        .storage()
        .instance()
        .get(&ReadinessDataKey::ReadinessChecklist)
        .unwrap_or_default();
    f(&mut checklist);
    env.storage()
        .instance()
        .set(&ReadinessDataKey::ReadinessChecklist, &checklist);
}

#[contractimpl]
impl Escrow {
    /// Hello-world style function for testing and CI.
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }

    /// Creates a new escrow contract with the specified client, freelancer, and milestone amounts.
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `client` - The address of the client funding the contract
    /// * `freelancer` - The address of the freelancer performing the work
    /// * `arbiter` - Optional arbiter address for dispute resolution
    /// * `milestones` - Vector of milestone amounts (in stroops)
    /// * `release_authorization` - Authorization mode for milestone releases
    /// 
    /// # Returns
    /// The unique contract ID
    /// 
    /// # Errors
    /// * `InvalidParticipants` - If client and freelancer are the same address
    /// * `EmptyMilestones` - If no milestones are provided
    /// * `InvalidMilestoneAmount` - If any milestone amount is <= 0
    /// * `MissingArbiter` - If arbiter is required but not provided
    /// * `InvalidArbiter` - If arbiter is same as client or freelancer
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        arbiter: Option<Address>,
        milestone_amounts: Vec<i128>,
        milestones: Vec<i128>,
        release_authorization: ReleaseAuthorization,
    ) -> u32 {
        client.require_auth();

        if client == freelancer {
            env.panic_with_error(Error::InvalidParticipants);
        }
        
        // Validate arbiter requirements
        match release_authorization {
            ReleaseAuthorization::ArbiterOnly | ReleaseAuthorization::ClientAndArbiter => {
                if arbiter.is_none() {
                    env.panic_with_error(Error::MissingArbiter);
                }
            }
            _ => {}
        }

        
        // Validate arbiter is not client or freelancer
        if let Some(ref arb) = arbiter {
            if arb == &client || arb == &freelancer {
                env.panic_with_error(Error::InvalidArbiter);
            }
        }
        
        if milestones.is_empty() {
            env.panic_with_error(Error::EmptyMilestones);
        }

        for amount in milestones.iter() {
            if amount <= 0 {
                env.panic_with_error(EscrowError::InvalidMilestoneAmount);
            }
        }
        if milestone_amounts.is_empty() {
            env.panic_with_error(EscrowError::EmptyMilestones);
        }
        if milestone_amounts.len() > MAX_MILESTONES {
            env.panic_with_error(EscrowError::TooManyMilestones);
        }

        let mut total_amount: i128 = 0;
        for amount in milestone_amounts.iter() {
            if amount <= 0 {
                env.panic_with_error(EscrowError::InvalidMilestoneAmount);
            }
            total_amount += amount;
        let mut total: i128 = 0;
        for i in 0..milestone_amounts.len() {
            let amt = milestone_amounts.get(i).unwrap();
            if amt <= 0 {
                env.panic_with_error(EscrowError::InvalidMilestoneAmount);
            }
            total = safe_add_amounts(total, amt)
                .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));
        }
        if total > MAX_TOTAL_ESCROW_STROOPS {
            env.panic_with_error(EscrowError::InvalidMilestoneAmount);
        }

        let id: u32 = env
            .storage()
            .persistent()
            .get::<_, u32>(&DataKey::NextContractId)
            .unwrap_or(1);

        // Store contract metadata
        let contract = Contract {
            client: client.clone(),
            freelancer,
            arbiter,
            milestones: milestone_amounts,
            status: ContractStatus::Created,
            funded_amount: 0,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Contract(id), &contract);

        // Store milestones
        let mut milestone_vec: Vec<Milestone> = Vec::new(&env);
        for amount in milestones.iter() {
            milestone_vec.push_back(Milestone {
                amount,
                released: false,
                refunded: false,
                work_evidence: None,
            });
        }
        let milestone_key = Symbol::new(&env, "milestones");
        env.storage()
            .persistent()
            .set(&(DataKey::Contract(id), milestone_key), &milestone_vec);

        env.storage().persistent().set(&DataKey::Contract(id), &data);
        env.storage().persistent().set(&DataKey::ContractCount, &(id + 1));
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &(id + 1));

        Self::emit_audit_event(
            env,
            id,
            ContractStatus::Created,
            ContractStatus::Created,
            &client,
        );

        env.events().publish(
            (symbol_short!("created"), id),
            (client, freelancer, env.ledger().timestamp()),
        );
        id
    }

    /// Assign an arbiter on a contract that was created without one.
    ///
    /// Only the client or freelancer can assign the arbiter. The arbiter must be
    /// distinct from both contract parties, can only be assigned once, and may
    /// only be assigned while the contract is in `Created` or `Funded` state.
    pub fn assign_arbiter(
        env: Env,
        contract_id: u32,
        caller: Address,
        arbiter: Address,
    ) -> bool {
        caller.require_auth();

        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            env.panic_with_error(EscrowError::ContractPaused);
        }

        let contract_key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, ContractData>(&contract_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        let is_client = caller == contract.client;
        let is_freelancer = caller == contract.freelancer;

        if !is_client && !is_freelancer {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }

        if contract.arbiter.is_some() {
            env.panic_with_error(EscrowError::ArbiterAlreadyAssigned);
        }

        if arbiter == contract.client || arbiter == contract.freelancer {
            env.panic_with_error(EscrowError::InvalidParticipant);
        }

        match contract.status {
            ContractStatus::Created | ContractStatus::Funded => {}
            _ => env.panic_with_error(EscrowError::InvalidStatusTransition),
        }

        contract.arbiter = Some(arbiter);
        env.storage().persistent().set(&contract_key, &contract);

        true
    }

    pub fn deposit_funds(env: Env, contract_id: u32, amount: i128, caller: Address) -> bool {
        caller.require_auth();

    /// Deposits funds into the contract. Transitions to Funded status when fully funded.
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// * `amount` - The amount to deposit (in stroops)
    /// 
    /// # Returns
    /// `true` if deposit was successful
    /// 
    /// # Errors
    /// * `AmountMustBePositive` - If amount is <= 0
    /// * `ContractNotFound` - If contract doesn't exist
    /// * `InvalidState` - If contract is not in Created state
    /// * `UnauthorizedRole` - If caller is not the client
    pub fn deposit_funds(env: Env, contract_id: u32, caller: Address, amount: i128) -> bool {
        if amount <= 0 {
            env.panic_with_error(Error::AmountMustBePositive);
        }

        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        // Only client can deposit
        if caller != contract.client {
            env.panic_with_error(Error::UnauthorizedRole);
        }
        
        caller.require_auth();
        
        // Can only deposit in Created state
        if contract.status != ContractStatus::Created {
            env.panic_with_error(Error::InvalidState);
        }

        contract.funded_amount += amount;

        // Calculate total milestone amount
        let milestone_key = Symbol::new(&env, "milestones");
        let milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key))
            .unwrap();

        let total_amount: i128 = milestones.iter().map(|m| m.amount).sum();

        // Transition to Funded if fully funded
        if contract.funded_amount >= total_amount && contract.status == ContractStatus::Created {
            contract.status = ContractStatus::Funded;
        }

        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        true
    }

    /// Approves a milestone for release.
    /// 
    /// Records the approval in temporary storage with TTL expiry.
    /// Approvals automatically expire after PENDING_APPROVAL_TTL_LEDGERS.
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// * `caller` - The address of the caller (must be authorized)
    /// * `milestone_index` - The index of the milestone to approve
    /// 
    /// # Returns
    /// `true` if approval was recorded successfully
    /// 
    /// # Errors
    /// * `ContractNotFound` - If contract doesn't exist
    /// * `InvalidState` - If contract is not in Funded state
    /// * `IndexOutOfBounds` - If milestone index is invalid
    /// * `MilestoneAlreadyReleased` - If milestone was already released
    /// * `UnauthorizedRole` - If caller is not authorized to approve
    /// * `AlreadyApproved` - If caller has already approved this milestone
    /// 
    /// # Security
    /// - Caller must be authenticated
    /// - Only authorized parties can approve based on ReleaseAuthorization mode
    /// - Approvals expire via TTL and are auto-evicted
    /// - Duplicate approvals are rejected
    pub fn approve_milestone_release(
        env: Env,
        contract_id: u32,
        caller: Address,
        milestone_index: u32,
    ) -> bool {
        approvals::approve_milestone(&env, contract_id, milestone_index, &caller)
            .unwrap_or_else(|e| env.panic_with_error(e))
    }

    pub fn release_milestone(
        env: Env,
        contract_id: u32,
        milestone_index: u32,
        caller: Address,
    ) -> bool {
        caller.require_auth();

        let contract_key = DataKey::Contract(contract_id);
        let mut contract = env
    /// Releases a specific milestone, transferring funds to the freelancer.
    /// 
    /// Requires valid, non-expired approvals based on the contract's ReleaseAuthorization mode.
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// * `caller` - The address of the caller (must be authorized)
    /// * `milestone_index` - The index of the milestone to release
    /// 
    /// # Returns
    /// `true` if release was successful
    /// 
    /// # Errors
    /// * `ContractNotFound` - If contract doesn't exist
    /// * `InvalidState` - If contract is not in Funded state
    /// * `InvalidMilestone` - If milestone index is out of bounds
    /// * `AlreadyReleased` - If milestone was already released
    /// * `AlreadyRefunded` - If milestone was already refunded
    /// * `InsufficientFunds` - If contract doesn't have enough funded balance
    /// * `InsufficientApprovals` - If required approvals are missing
    /// * `ApprovalExpired` - If approvals have expired
    /// * `UnauthorizedRole` - If caller is not authorized to release
    /// 
    /// # Security
    /// - Requires valid approvals that haven't expired
    /// - Approvals are cleared after successful release
    /// - Fail-closed: missing or expired approvals prevent release
    pub fn release_milestone(
        env: Env,
        contract_id: u32,
        caller: Address,
        milestone_index: u32,
    ) -> bool {
        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        let is_client = caller == contract.client;
        let is_freelancer = caller == contract.freelancer;
        if !is_client && !is_freelancer {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }

        // Mark this milestone as released
        let milestone_key = DataKey::MilestoneReleased(contract_id, milestone_index);
        env.storage().persistent().set(&milestone_key, &true);
        // Verify contract is in Funded state
        if contract.status != ContractStatus::Funded {
            env.panic_with_error(Error::InvalidState);
        }

        // Check authorization for release
        let is_client = caller == contract.client;
        let is_arbiter = contract.arbiter.as_ref().map_or(false, |a| &caller == a);

        match contract.release_authorization {
            ReleaseAuthorization::ClientOnly => {
                if !is_client {
                    env.panic_with_error(Error::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::ArbiterOnly => {
                if !is_arbiter {
                    env.panic_with_error(Error::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::ClientAndArbiter => {
                if !is_client && !is_arbiter {
                    env.panic_with_error(Error::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::MultiSig => {
                if !is_client && !is_arbiter {
                    env.panic_with_error(Error::UnauthorizedRole);
                }
            }
        }

        caller.require_auth();

        // Check for valid approvals
        approvals::check_approvals(&env, &contract, contract_id, milestone_index)
            .unwrap_or_else(|e| env.panic_with_error(e));

        let milestone_key = Symbol::new(&env, "milestones");
        let mut milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key.clone()))
            .unwrap();

        if milestone_index >= milestones.len() {
            env.panic_with_error(Error::IndexOutOfBounds);
        }

        let mut milestone = milestones.get(milestone_index).unwrap();

        if milestone.released {
            env.panic_with_error(Error::MilestoneAlreadyReleased);
        }

        if milestone.refunded {
            env.panic_with_error(Error::AlreadyRefunded);
        }

        // Check if there's enough balance
        let available_balance =
            contract.funded_amount - contract.released_amount - contract.refunded_amount;
        if available_balance < milestone.amount {
            env.panic_with_error(Error::InsufficientFunds);
        }

        milestone.released = true;
        milestones.set(milestone_index, milestone);
        contract.released_amount += milestone.amount;

        // Clear approvals after successful release
        approvals::clear_approvals(&env, contract_id, milestone_index);

        // Check if all milestones are released
        let all_released = milestones.iter().all(|m| m.released || m.refunded);
        if all_released {
            contract.status = ContractStatus::Completed;
        }

    pub fn finalize_contract(env: Env, contract_id: u32, caller: Address) -> bool {
        caller.require_auth();

        let contract_key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, ContractData>(&contract_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        if caller != contract.client {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }

        if contract.status != ContractStatus::Created && contract.status != ContractStatus::Funded {
            env.panic_with_error(EscrowError::InvalidStatusTransition);
        }

        contract.status = ContractStatus::Completed;
        env.storage().persistent().set(&contract_key, &contract);

        true
    }

    pub fn withdraw_leftover(
        env: Env,
        contract_id: u32,
        caller: Address,
    ) -> i128 {
        caller.require_auth();

        let contract_key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, ContractData>(&contract_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        if caller != contract.client {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }

        if contract.status != ContractStatus::Completed {
            env.panic_with_error(EscrowError::InvalidStatusTransition);
        }

        let leftover = contract.total_deposited - contract.released_amount;
        if leftover <= 0 {
            env.panic_with_error(EscrowError::NoLeftoverFunds);
        }

        contract.total_deposited = contract.released_amount;
        env.storage().persistent().set(&contract_key, &contract);

        leftover
    }

    /// Get contract details
    pub fn get_contract(env: Env, contract_id: u32) -> ContractData {
        env.storage()
            .persistent()
            .set(&(DataKey::Contract(contract_id), milestone_key), &milestones);
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        true
    }

    /// Refunds unreleased milestones back to the client.
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// * `milestone_indices` - Vector of milestone indices to refund
    /// 
    /// # Returns
    /// The total amount refunded
    /// 
    /// # Errors
    /// * `ContractNotFound` - If contract doesn't exist
    /// * `EmptyRefundRequest` - If milestone_indices is empty
    /// * `DuplicateMilestoneInRefund` - If the same milestone appears multiple times
    /// * `IndexOutOfBounds` - If any milestone index is out of bounds
    /// * `AlreadyReleased` - If any milestone was already released
    /// * `AlreadyRefunded` - If any milestone was already refunded
    /// * `InsufficientFunds` - If contract doesn't have enough balance to refund
    pub fn refund_unreleased_milestones(
        env: Env,
        contract_id: u32,
        milestone_indices: Vec<u32>,
    ) -> i128 {
        // Validate non-empty request
        if milestone_indices.is_empty() {
            env.panic_with_error(Error::EmptyRefundRequest);
        }

        // Check for duplicates
        for i in 0..milestone_indices.len() {
            for j in (i + 1)..milestone_indices.len() {
                if milestone_indices.get(i).unwrap() == milestone_indices.get(j).unwrap() {
                    env.panic_with_error(Error::DuplicateMilestoneInRefund);
                }
            }
        }

        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        contract.client.require_auth();

        let milestone_key = Symbol::new(&env, "milestones");
        let mut milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key.clone()))
            .unwrap();

        let mut total_refund_amount: i128 = 0;

        // Validate all milestones first
        for idx in milestone_indices.iter() {
            if idx >= milestones.len() {
                env.panic_with_error(Error::IndexOutOfBounds);
            }

            let milestone = milestones.get(idx).unwrap();

            if milestone.released {
                env.panic_with_error(Error::AlreadyReleased);
            }

            if milestone.refunded {
                env.panic_with_error(Error::AlreadyRefunded);
            }

            total_refund_amount += milestone.amount;
        }

        // Check if there's enough balance
        let available_balance =
            contract.funded_amount - contract.released_amount - contract.refunded_amount;
        if available_balance < total_refund_amount {
            env.panic_with_error(Error::InsufficientFunds);
        }

        // Mark milestones as refunded
        for idx in milestone_indices.iter() {
            let mut milestone = milestones.get(idx).unwrap();
            milestone.refunded = true;
            milestones.set(idx, milestone);
        }

        contract.refunded_amount += total_refund_amount;

        // Check if all unreleased milestones are refunded
        let all_refunded_or_released = milestones.iter().all(|m| m.released || m.refunded);
        if all_refunded_or_released {
            let all_refunded = milestones.iter().all(|m| m.refunded);
            if all_refunded {
                contract.status = ContractStatus::Refunded;
            } else {
                // Some released, some refunded
                contract.status = ContractStatus::Completed;
            }
        }

        env.storage()
            .persistent()
            .set(&(DataKey::Contract(contract_id), milestone_key), &milestones);
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        total_refund_amount
    }

    /// Retrieves contract information.
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// 
    /// # Returns
    /// The contract data
    /// 
    /// # Errors
    /// * `ContractNotFound` - If contract doesn't exist
    pub fn get_contract(env: Env, contract_id: u32) -> Contract {
        env.storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound))
    }

    /// Retrieves all milestones for a contract.
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// 
    /// # Returns
    /// Vector of milestones
    /// 
    /// # Errors
    /// * `ContractNotFound` - If contract doesn't exist
    pub fn get_milestones(env: Env, contract_id: u32) -> Vec<Milestone> {
        let milestone_key = Symbol::new(&env, "milestones");
        env.storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound))
    }

    /// Calculates the refundable balance (funded but not released or refunded).
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// 
    /// # Returns
    /// The refundable balance amount
    /// 
    /// # Errors
    /// * `ContractNotFound` - If contract doesn't exist
    pub fn get_refundable_balance(env: Env, contract_id: u32) -> i128 {
        let contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        contract.funded_amount - contract.released_amount - contract.refunded_amount
    }
    
    /// Retrieves approval status for a milestone.
    /// 
    /// Returns None if approvals have expired or don't exist.
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// * `milestone_index` - The milestone index
    /// 
    /// # Returns
    /// Optional MilestoneApprovals struct
    pub fn get_milestone_approvals(
        env: Env,
        contract_id: u32,
        milestone_index: u32,
    ) -> Option<MilestoneApprovals> {
        let approval_key = DataKey::MilestoneApprovals(contract_id, milestone_index);
        env.storage().temporary().get(&approval_key)
    }
}

#[cfg(test)]
mod proptest;
#[cfg(test)]
mod simple_amount_test;

#[cfg(test)]
mod test;
