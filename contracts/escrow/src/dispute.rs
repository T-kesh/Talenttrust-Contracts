//! # Dispute resolution library
//!
//! This module contains the **pure** dispute-resolution helpers used by the
//! guarded entry points (`raise_dispute`, `resolve_dispute`,
//! `resolve_dispute_split`, `get_dispute`) exposed on the `Escrow`
//! contract.
//!
//! The functions here do not perform authentication or storage mutation on
//! their own. They are deliberately pure so that they can be unit-tested
//! without a fully-featured `Env` and so that the only mutable side effects
//! of an arbiter decision live in the entry points, where they can be
//! audited in one place.
//!
//! ## Invariants
//!
//! 1. `split_payouts` is the single source of truth for Split validation:
//!    it panics with [`EscrowError::NonPositiveAmount`] if either
//!    component is negative and with
//!    [`EscrowError::AccountingInvariantViolated`] if the two components
//!    do not sum to the contract's currently available escrow balance
//!    (`total_deposited - released_amount - refunded_amount`).
//! 2. `final_status_after_resolution` and `final_status_after_split` only
//!    ever report a contract status reachable from `Disputed`.
//!    `Cancel` is terminal at `Cancelled`; `Release`/`Refund` that
//!    fully cover the milestone total land in `Completed`/`Refunded`
//!    respectively; everything else stays `Funded` while funds continue
//!    to drive the contract toward an unambiguous terminal state.

use soroban_sdk::{Address, Env};

use crate::{ContractStatus, EscrowContractData, EscrowError};

/// Re-export the dispute-domain types so embedders can `use
/// crate::dispute::DisputeResolution;` without crossing the parent crate.
pub use crate::{DisputeMetadata, DisputeResolution, DisputeSplit};

/// Computes and validates the per-party payouts for a `DisputeSplit`
/// arbitrary split.
///
/// Pure function: panics on invalid input but does not otherwise touch
/// contract state.
///
/// # Errors
/// * `EscrowError::NonPositiveAmount` — either split component is
///   negative.
/// * `EscrowError::AccountingInvariantViolated` — the two split
///   components do not sum to the available escrow balance.
pub fn split_payouts(
    env: &Env,
    contract: &EscrowContractData,
    split: &DisputeSplit,
) -> (i128, i128) {
    let available = contract.total_deposited - contract.released_amount - contract.refunded_amount;
    if split.client_amount < 0 || split.freelancer_amount < 0 {
        env.panic_with_error(EscrowError::NonPositiveAmount);
    }
    if split.client_amount + split.freelancer_amount != available {
        env.panic_with_error(EscrowError::AccountingInvariantViolated);
    }
    (split.client_amount, split.freelancer_amount)
}

/// Returns the sum of every milestone amount on `contract`.
fn milestone_total(contract: &EscrowContractData) -> i128 {
    let mut total: i128 = 0;
    for m in contract.milestones.iter() {
        total = total.saturating_add(m);
    }
    total
}

/// Translates post-resolution accounting into a `ContractStatus` for the
/// simple Release/Refund/Cancel variants and for `Split`. Both share
/// these terminal-state rules:
///
/// - `Completed` when the freelancer-side payout fully covers every
///   milestone and the client has received nothing.
/// - `Refunded` when the client-side payout fully covers every
///   milestone and the freelancer has received nothing.
/// - `Funded` otherwise (intermediate state with leftover funds).
///
/// `resolution` is consulted only to short-circuit `Cancelled`, which
/// always wins regardless of the post-accounting math.
fn status_from_post_accounting(
    resolution: &DisputeResolution,
    new_released: i128,
    new_refunded: i128,
    total: i128,
) -> ContractStatus {
    match resolution {
        DisputeResolution::Cancel => ContractStatus::Cancelled,
        _ => {
            if new_released == total && new_refunded == 0 {
                ContractStatus::Completed
            } else if new_refunded == total && new_released == 0 {
                ContractStatus::Refunded
            } else {
                ContractStatus::Funded
            }
        }
    }
}

/// Returns the post-resolution `ContractStatus` implied by `resolution`.
///
/// Pure function — does not take `&Env` because the simple-variant
/// payouts are derived deterministically from the contract state and
/// cannot violate any invariants (only `Split` can, and `Split` is
/// always validated through `split_payouts` before reaching that
/// decision point).
///
/// It derives the **post**-application state of the contract by adding
/// the per-variant payouts to the existing `released_amount` and
/// `refunded_amount` *before* deciding which terminal state applies.
/// Without this step, a `Release` resolution against a fully-funded but
/// never-released contract would land on `Funded` (because the
/// pre-state `released_amount` is zero) instead of `Completed`. See
/// the module-level invariants for the state machine guarantees.
pub fn final_status_after_resolution(
    contract: &EscrowContractData,
    resolution: &DisputeResolution,
) -> ContractStatus {
    let available = contract.total_deposited - contract.released_amount - contract.refunded_amount;

    // Per-variant payouts, computed inline.
    let (client_payout, freelancer_payout) = match resolution {
        DisputeResolution::Release => (0, available),
        DisputeResolution::Refund => (available, 0),
        DisputeResolution::Cancel => (0, 0),
    };

    let new_released = contract.released_amount + freelancer_payout;
    let new_refunded = contract.refunded_amount + client_payout;
    let total = milestone_total(contract);

    status_from_post_accounting(resolution, new_released, new_refunded, total)
}

/// Returns the post-resolution `ContractStatus` implied by a
/// `DisputeSplit`. Mirrors the logic of [`final_status_after_resolution`]
/// but constrained to a split outcome — `Completed` if the freelancer
/// receives everything, `Refunded` if the client receives everything,
/// otherwise `Funded`.
///
/// `split` must already have been validated by [`split_payouts`]; an
/// unvalidated split will produce a misleading status silently rather
/// than panicking, so callers must ensure validation precedes this
/// call.
pub fn final_status_after_split(
    contract: &EscrowContractData,
    split: &DisputeSplit,
) -> ContractStatus {
    let new_released = contract.released_amount + split.freelancer_amount;
    let new_refunded = contract.refunded_amount + split.client_amount;
    let total = milestone_total(contract);

    // Synthesize a sentinel resolution to reuse
    // `status_from_post_accounting`; only its "not-Cancel" branch is
    // reachable for a real Split outcome.
    let sentinel = DisputeResolution::Refund;
    status_from_post_accounting(&sentinel, new_released, new_refunded, total)
}

/// Confirms that `caller` is the registered arbiter for `contract`.
///
/// Panics with [`EscrowError::DisputeArbiterMissing`] if the contract was
/// created without an arbiter and with [`EscrowError::UnauthorizedRole`]
/// for any caller that is not the configured arbiter address.
pub fn require_arbiter(env: &Env, contract: &EscrowContractData, caller: &Address) {
    match &contract.arbiter {
        None => env.panic_with_error(EscrowError::DisputeArbiterMissing),
        Some(arbiter) if arbiter == caller => {}
        Some(_) => env.panic_with_error(EscrowError::UnauthorizedRole),
    }
}

/// Confirms that `caller` is either the client or the freelancer.
///
/// Used by `raise_dispute` to limit dispute initiation to the
/// contracting parties. Panics with [`EscrowError::UnauthorizedRole`]
/// for any caller that is neither.
pub fn require_party(env: &Env, contract: &EscrowContractData, caller: &Address) {
    if caller != &contract.client && caller != &contract.freelancer {
        env.panic_with_error(EscrowError::UnauthorizedRole);
    }
}
