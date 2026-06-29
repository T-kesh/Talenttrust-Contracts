//! Dispute module — single canonical implementation.
//!
//! This module owns:
//! - [`DisputeResolution`] re-exported from `types` (the `Split(DisputeSplit)` form).
//! - [`resolution_payouts`] — pure payout arithmetic.
//! - [`final_status_after_resolution`] — status transition helper.
//!
//! The single `#[contractimpl] impl Escrow` block for `raise_dispute` and
//! `resolve_dispute` lives in `lib.rs`. All dispute entrypoints delegate
//! payout logic here, but the Soroban contract implementation must appear
//! exactly once in the crate — in `lib.rs`.
//!
//! Security assumptions preserved on the single path:
//! - Pause/emergency gate fires before any state mutation.
//! - Only the assigned client or freelancer may raise a dispute.
//! - An arbiter must be set on the contract before a dispute can be raised.
//! - A dispute can only be raised on a `Funded` or `PartiallyFunded` contract.
//! - Only the assigned arbiter may resolve a dispute.
//! - Resolution can only happen on a `Disputed` contract.
//! - Conservation invariant: `released_amount + refunded_amount == funded_amount`
//!   is asserted after every resolution.

use crate::{
    safe_add_amounts, Contract, ContractStatus, DisputeResolution, DisputeSplit,
    Error as EscrowError,
};

/// Compute the `(client_payout, freelancer_payout)` pair for a given resolution.
///
/// Uses `contract.funded_amount` as the total deposited amount and subtracts
/// already-released and already-refunded amounts to arrive at the available
/// balance. All arithmetic is checked; overflows and corrupted accounting
/// states are surfaced as errors rather than panics so callers can map them
/// to on-chain errors.
///
/// # Errors
/// - [`EscrowError::AccountingInvariantViolated`] when the computed available
///   balance is negative (indicates corrupted on-chain state).
/// - [`EscrowError::PotentialOverflow`] on checked-arithmetic overflow in
///   `PartialRefund` payout calculation.
/// - [`EscrowError::InvalidDisputeSplit`] when a `Split` variant has negative
///   legs or legs that do not sum to the available balance.
pub fn resolution_payouts(
    contract: &Contract,
    resolution: &DisputeResolution,
) -> Result<(i128, i128), EscrowError> {
    let available = contract
        .funded_amount
        .checked_sub(contract.released_amount)
        .and_then(|v| v.checked_sub(contract.refunded_amount))
        .ok_or(EscrowError::AccountingInvariantViolated)?;

    if available < 0 {
        return Err(EscrowError::AccountingInvariantViolated);
    }

    match resolution {
        DisputeResolution::FullRefund => Ok((available, 0)),
        DisputeResolution::PartialRefund => {
            // 30 % to freelancer (floor-rounded); remainder to client.
            let freelancer_payout = available
                .checked_mul(30)
                .and_then(|v| v.checked_div(100))
                .ok_or(EscrowError::PotentialOverflow)?;
            Ok((available - freelancer_payout, freelancer_payout))
        }
        DisputeResolution::FullPayout => Ok((0, available)),
        DisputeResolution::Split(DisputeSplit {
            client_amount,
            freelancer_amount,
        }) => {
            if *client_amount < 0 || *freelancer_amount < 0 {
                return Err(EscrowError::InvalidDisputeSplit);
            }
            let total = safe_add_amounts(*client_amount, *freelancer_amount)
                .ok_or(EscrowError::PotentialOverflow)?;
            if total != available {
                return Err(EscrowError::InvalidDisputeSplit);
            }
            Ok((*client_amount, *freelancer_amount))
        }
    }
}

/// Determine the terminal [`ContractStatus`] after a dispute has been resolved.
///
/// Returns [`ContractStatus::Refunded`] when every funded stroop has been
/// refunded to the client; otherwise returns [`ContractStatus::Completed`].
pub fn final_status_after_resolution(contract: &Contract) -> ContractStatus {
    if contract.refunded_amount == contract.funded_amount {
        ContractStatus::Refunded
    } else {
        ContractStatus::Completed
    }
}
