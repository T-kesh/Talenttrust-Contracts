#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Symbol, Vec};

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

#[contract]
pub struct Escrow;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EscrowError {
    InvalidParticipant = 1,
    EmptyMilestones = 2,
    InvalidMilestoneAmount = 3,
    InvalidDepositAmount = 4,
    InvalidMilestone = 5,
    /// Milestone count exceeds MAX_MILESTONES.
    TooManyMilestones = 6,
    /// Sum of milestone amounts exceeds MAX_TOTAL_ESCROW_STROOPS,
    /// or would overflow i128.
    TotalCapExceeded = 7,
}

/// Exposes the hard-coded contract bounds for off-chain tooling and on-chain
/// integrators. Read-only — no state mutation.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowBounds {
    pub max_milestones: u32,
    pub max_total_escrow_stroops: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractData {
    pub client: Address,
    pub freelancer: Address,
    pub milestones: Vec<i128>,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    NextId,
    Contract(u32),
}

#[contractimpl]
impl Escrow {
    /// Hello-world style function for testing and CI.
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }

    /// Returns the hard-coded bounds enforced by this contract.
    /// Useful for client-side pre-validation and monitoring dashboards.
    pub fn get_bounds(_env: Env) -> EscrowBounds {
        EscrowBounds {
            max_milestones: MAX_MILESTONES,
            max_total_escrow_stroops: MAX_TOTAL_ESCROW_STROOPS,
        }
    }

    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        milestones: Vec<i128>,
    ) -> u32 {
        // ── Identity checks ───────────────────────────────────────────────────
        if client == freelancer {
            env.panic_with_error(EscrowError::InvalidParticipant);
        }

        // ── Milestone count checks ────────────────────────────────────────────
        if milestones.is_empty() {
            env.panic_with_error(EscrowError::EmptyMilestones);
        }
        if milestones.len() > MAX_MILESTONES {
            env.panic_with_error(EscrowError::TooManyMilestones);
        }

        // ── Amount + total cap checks ─────────────────────────────────────────
        // Accumulate with checked_add so i128 overflow is also caught.
        let mut total: i128 = 0;
        for amount in milestones.iter() {
            if amount <= 0 {
                env.panic_with_error(EscrowError::InvalidMilestoneAmount);
            }
            // checked_add returns None on overflow — treat both overflow and
            // exceeding the cap identically (TotalCapExceeded).
            total = total
                .checked_add(amount)
                .filter(|&t| t <= MAX_TOTAL_ESCROW_STROOPS)
                .unwrap_or_else(|| {
                    env.panic_with_error(EscrowError::TotalCapExceeded);
                    0 // unreachable — panic_with_error diverges
                });
        }

        // ── Persist ───────────────────────────────────────────────────────────
        let id = env
            .storage()
            .persistent()
            .get::<_, u32>(&DataKey::NextId)
            .unwrap_or(0);

        let data = ContractData {
            client,
            freelancer,
            milestones,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Contract(id), &data);
        env.storage().persistent().set(&DataKey::NextId, &(id + 1));

        id
    }

    pub fn deposit_funds(env: Env, _contract_id: u32, amount: i128) -> bool {
        if amount <= 0 {
            env.panic_with_error(EscrowError::InvalidDepositAmount);
        }
        true
    }

    pub fn release_milestone(env: Env, contract_id: u32, milestone_index: u32) -> bool {
        let _ = (env, contract_id, milestone_index);
        true
    }
}

#[cfg(test)]
mod test;
#[cfg(test)]
mod test_bounds;
