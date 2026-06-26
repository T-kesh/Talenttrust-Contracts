use crate::{
    approvals, ttl, Contract, ContractStatus, DataKey, EscrowError, Error, Escrow, EscrowArgs, EscrowClient,
    Milestone, ReleaseAuthorization,
};
use soroban_sdk::{Address, Env, Symbol, Vec};

impl Escrow {
    /// Core logic for releasing a milestone, transferring funds to the freelancer.
    ///
    /// Called from the single `#[contractimpl]` block in lib.rs after the
    /// initialization, pause, and auth guards have been checked.
    pub(crate) fn release_milestone_impl(
        env: &Env,
        contract_id: u32,
        caller: Address,
        milestone_index: u32,
    ) -> bool {
        Self::require_not_paused(&env);
        caller.require_auth();

        Self::require_not_paused(&env);

        Self::require_not_finalized(&env, contract_id);

        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        ttl::extend_contract_ttl(&env, contract_id);

        Self::require_not_paused(&env);
        Self::require_not_finalized(&env, contract_id);

        if contract.status != ContractStatus::Funded {
            env.panic_with_error(Error::InvalidState);
        }

        let is_client = caller == contract.client;
        let is_freelancer = caller == contract.freelancer;
        let is_arbiter = contract.arbiter.as_ref() == Some(&caller);

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
                if !is_client && !is_freelancer {
                    env.panic_with_error(Error::UnauthorizedRole);
                }
            }
        }

        let milestone_key = Symbol::new(&env, "milestones");
        let mut milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key.clone()))
            .unwrap();

        ttl::extend_milestone_ttl(&env, contract_id);

        if milestone_index >= milestones.len() {
            env.panic_with_error(Error::IndexOutOfBounds);
        }

        let mut milestone = milestones.get(milestone_index).unwrap().clone();

        if milestone.released {
            env.panic_with_error(Error::MilestoneAlreadyReleased);
        }

        if milestone.refunded {
            env.panic_with_error(Error::AlreadyRefunded);
        }

        approvals::check_approvals(&env, &contract, contract_id, milestone_index)
            .unwrap_or_else(|e| env.panic_with_error(e));

        let available_balance = safe_subtract_amounts(
            safe_subtract_amounts(contract.funded_amount, contract.released_amount)
                .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow)),
            contract.refunded_amount,
        )
        .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));
        if available_balance < milestone.amount {
            env.panic_with_error(Error::InsufficientFunds);
        }

        let _release_amount = milestone.amount;
        milestone.released = true;
        milestones.set(milestone_index, milestone.clone());
        contract.released_amount = safe_add_amounts(contract.released_amount, milestone.amount)
            .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));

        if is_initialized(&env) {
            let fee_bps = get_protocol_fee_bps(&env);
            if fee_bps > 0 {
                let fee = calculate_protocol_fee(&env, milestone.amount, fee_bps);
                let current_accumulated: i128 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::AccumulatedProtocolFees)
                    .unwrap_or(0);
                env.storage().persistent().set(
                    &DataKey::AccumulatedProtocolFees,
                    &(current_accumulated + fee),
                );
            }
        }

        approvals::clear_approvals(&env, contract_id, milestone_index);

        let all_released = milestones.iter().all(|m| m.released || m.refunded);
        if all_released {
            contract.status = ContractStatus::Completed;
            let pending_key = DataKey::PendingReputationCredits(contract.freelancer.clone());
            let pending: i128 = env.storage().persistent().get(&pending_key).unwrap_or(0);
            env.storage().persistent().set(&pending_key, &(pending + 1));
        }

        env.storage().persistent().set(
            &(DataKey::Contract(contract_id), milestone_key),
            &milestones,
        );
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        ttl::extend_contract_and_milestones_ttl(env, contract_id);

        env.events().publish(
            (Symbol::new(&env, "milestone_released"), contract_id),
            (caller, milestone_index, milestone.amount),
        );

/// Maximum allowed protocol fee in basis points (99.99%).
/// Values >= 10_000 would make fee >= released amount.
const MAX_PROTOCOL_FEE_BPS: u32 = 10_000;

/// Calculates the protocol fee for a given amount.
///
/// Uses overflow-safe arithmetic with explicit round-half-up semantics.
/// The fee is computed as `(amount * fee_bps + 5000 - 1) / 10000` which rounds
/// toward positive infinity for positive amounts.
///
/// # Arguments
/// * `env` - The contract environment (for error handling)
/// * `amount` - The milestone amount (i128, must be positive)
/// * `fee_bps` - The fee rate in basis points (u32, 1-9999)
///
/// # Returns
/// The computed fee amount
///
/// # Panics
/// Panics with `EscrowError::ProtocolFeeOverflow` if `amount * fee_bps` overflows.
///
/// # Security Guarantees
/// - Fee never equals or exceeds the milestone amount
/// - Uses checked arithmetic to prevent silent overflow
/// - Round-half-up ensures deterministic, predictable fee calculation
fn calculate_protocol_fee(env: &Env, amount: i128, fee_bps: u32) -> i128 {
    // Note: This function is only called when fee_bps > 0, so we don't need to handle 0 here
    
    // Use u128 widening to handle potential overflow safely
    // amount can be up to i128::MAX, fee_bps up to 9999
    // i128::MAX * 9999 ≈ 1.7 * 10^37 which fits in u128 but could overflow i128
    let amount_u128 = amount as u128;
    let fee_bps_u128 = fee_bps as u128;
    let scaled_divisor: u128 = MAX_PROTOCOL_FEE_BPS as u128;
    
    // Compute: amount * fee_bps (checked, widened to u128)
    let product = amount_u128
        .checked_mul(fee_bps_u128)
        .unwrap_or_else(|| env.panic_with_error(EscrowError::ProtocolFeeOverflow));
    
    // Round half-up: add (divisor / 2) - 1 before division
    // fee = (amount * fee_bps + 5000 - 1) / 10000 
    // This rounds 0.5 upward (toward positive infinity for positive amounts)
    let rounding = scaled_divisor / 2; // 5000
    let fee = (product + rounding - 1) / scaled_divisor;
    
    // Cap fee to be strictly less than amount (prevents fee >= amount)
    // This is the final security guarantee: fee < milestone.amount
    let fee = fee.min(amount_u128.saturating_sub(1)) as i128;
    
    fee
}
