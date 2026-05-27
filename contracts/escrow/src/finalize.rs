use soroban_sdk::{contracttype, symbol_short, Address, Env, Vec};

use crate::{
    safe_add_amounts, safe_subtract_amounts, ContractStatus, ContractSummary, DataKey, Escrow,
    EscrowArgs, EscrowClient, EscrowContractData, EscrowError, MilestoneSummary,
    CONTRACT_SUMMARY_SCHEMA_VERSION,
};

/// Immutable metadata written when an escrow contract is closed.
///
/// The record is stored once under `DataKey::Finalization(contract_id)`.
/// After it exists, all contract-specific mutating entrypoints reject with
/// `EscrowError::AlreadyFinalized`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinalizationRecord {
    /// Authorized client, freelancer, or assigned arbiter that finalized.
    pub finalizer: Address,
    /// Ledger timestamp at finalization time.
    pub timestamp: u64,
    /// Snapshot of participant, milestone, status, and accounting state.
    pub summary: ContractSummary,
}

impl Escrow {
    fn finalization_key(contract_id: u32) -> DataKey {
        DataKey::Finalization(contract_id)
    }

    fn load_contract_for_finalization(env: &Env, contract_id: u32) -> EscrowContractData {
        env.storage()
            .persistent()
            .get::<_, EscrowContractData>(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound))
    }

    fn require_finalizer_role(env: &Env, contract: &EscrowContractData, finalizer: &Address) {
        let is_client = *finalizer == contract.client;
        let is_freelancer = *finalizer == contract.freelancer;
        let is_arbiter = contract.arbiter.clone().is_some_and(|a| a == *finalizer);
        if !is_client && !is_freelancer && !is_arbiter {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }
    }

    fn summarize_contract(
        env: &Env,
        contract_id: u32,
        contract: &EscrowContractData,
    ) -> ContractSummary {
        let mut total_amount: i128 = 0;
        let mut released_milestone_count: u32 = 0;
        let mut milestones = Vec::new(env);

        for index in 0..contract.milestones.len() {
            let amount = contract.milestones.get(index).unwrap();
            total_amount = safe_add_amounts(total_amount, amount)
                .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));

            let released = env
                .storage()
                .persistent()
                .get::<_, bool>(&DataKey::MilestoneReleased(contract_id, index))
                .unwrap_or(false);
            if released {
                released_milestone_count = released_milestone_count
                    .checked_add(1)
                    .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));
            }

            milestones.push_back(MilestoneSummary {
                index,
                amount,
                released,
                refunded: false,
            });
        }

        let after_releases =
            safe_subtract_amounts(contract.total_deposited, contract.released_amount)
                .unwrap_or_else(|| env.panic_with_error(EscrowError::AccountingInvariantViolated));
        let refundable_balance = safe_subtract_amounts(after_releases, contract.refunded_amount)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::AccountingInvariantViolated));

        ContractSummary {
            schema_version: CONTRACT_SUMMARY_SCHEMA_VERSION,
            client: contract.client.clone(),
            freelancer: contract.freelancer.clone(),
            arbiter: contract.arbiter.clone(),
            status: contract.status,
            reputation_issued: contract.reputation_issued,
            total_amount,
            funded_amount: contract.total_deposited,
            released_amount: contract.released_amount,
            refundable_balance,
            released_milestone_count,
            milestones,
        }
    }
}

#[soroban_sdk::contractimpl]
impl Escrow {
    /// Finalize an escrow contract by writing immutable close metadata.
    ///
    /// `finalizer` must authorize the call and must be the stored client,
    /// freelancer, or assigned arbiter. Finalization is allowed only while the
    /// contract is `Completed` or `Disputed`. Once finalized, future
    /// contract-specific mutations fail with `AlreadyFinalized`.
    ///
    /// # Errors
    /// - `ContractPaused` when pause or emergency controls are active.
    /// - `ContractNotFound` when `contract_id` is unknown.
    /// - `AlreadyFinalized` when a close record already exists.
    /// - `UnauthorizedRole` when `finalizer` is not a contract participant.
    /// - `InvalidStatusTransition` unless status is `Completed` or `Disputed`.
    pub fn finalize_contract(env: Env, contract_id: u32, finalizer: Address) -> bool {
        Self::require_not_paused(&env);
        finalizer.require_auth();

        let contract = Self::load_contract_for_finalization(&env, contract_id);
        Self::require_not_finalized(&env, contract_id);
        Self::require_finalizer_role(&env, &contract, &finalizer);

        if contract.status != ContractStatus::Completed
            && contract.status != ContractStatus::Disputed
        {
            env.panic_with_error(EscrowError::InvalidStatusTransition);
        }

        let record = FinalizationRecord {
            finalizer: finalizer.clone(),
            timestamp: env.ledger().timestamp(),
            summary: Self::summarize_contract(&env, contract_id, &contract),
        };

        env.storage()
            .persistent()
            .set(&Self::finalization_key(contract_id), &record);

        env.events().publish(
            (symbol_short!("finalized"), contract_id),
            (finalizer, record.timestamp),
        );

        true
    }

    /// Return immutable close metadata for `contract_id`, if it has been finalized.
    pub fn get_finalization_record(env: Env, contract_id: u32) -> Option<FinalizationRecord> {
        env.storage()
            .persistent()
            .get(&Self::finalization_key(contract_id))
    }
}
