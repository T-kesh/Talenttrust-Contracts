use soroban_sdk::{contractimpl, contracttype, Address, Env, Symbol};

use crate::{
    safe_add_amounts, ttl, Contract, ContractStatus, Error as EscrowError, Escrow, DataKey,
    EscrowClient, EscrowArgs,
};

/// Resolution selected by the assigned arbiter for a disputed escrow.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeResolution {
    /// Refund all remaining escrowed funds to the client.
    FullRefund,
    /// Refund 70% of the remaining balance to the client and release 30% to the freelancer.
    PartialRefund,
    /// Release all remaining escrowed funds to the freelancer.
    FullPayout,
    /// Apply a custom split of the remaining balance.
    Split(i128, i128),
}

impl DisputeResolution {
    pub fn code(&self) -> u32 {
        match self {
            Self::FullRefund => 0,
            Self::PartialRefund => 1,
            Self::FullPayout => 2,
            Self::Split(_, _) => 3,
        }
    }
}

pub fn resolution_payouts(
    contract: &Contract,
    resolution: &DisputeResolution,
) -> Result<(i128, i128), EscrowError> {
    let available = contract
        .funded_amount
        .checked_sub(contract.released_amount)
        .and_then(|value| value.checked_sub(contract.refunded_amount))
        .ok_or(EscrowError::AccountingInvariantViolated)?;
    if available < 0 {
        return Err(EscrowError::AccountingInvariantViolated);
    }

    match resolution {
        DisputeResolution::FullRefund => Ok((available, 0)),
        DisputeResolution::PartialRefund => {
            let freelancer_payout = available
                .checked_mul(30)
                .and_then(|value| value.checked_div(100))
                .ok_or(EscrowError::PotentialOverflow)?;
            Ok((available - freelancer_payout, freelancer_payout))
        }
        DisputeResolution::FullPayout => Ok((0, available)),
        DisputeResolution::Split(client_amount, freelancer_amount) => {
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

pub fn final_status_after_resolution(contract: &Contract) -> ContractStatus {
    if contract.refunded_amount == contract.funded_amount {
        ContractStatus::Refunded
    } else {
        ContractStatus::Completed
    }
}

#[contractimpl]
impl Escrow {
    /// Raise a dispute on a contract. Blocked when paused.
    pub fn raise_dispute(env: Env, contract_id: u32, caller: Address) -> bool {
        Self::require_not_paused(&env);
        caller.require_auth();

        let key = DataKey::Contract(contract_id);
        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        Self::require_not_finalized(&env, contract_id);

        if caller != contract.client && caller != contract.freelancer {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }
        if contract.arbiter.is_none() {
            env.panic_with_error(EscrowError::ArbiterRequired);
        }
        if contract.status != ContractStatus::Funded
            && contract.status != ContractStatus::PartiallyFunded
        {
            env.panic_with_error(EscrowError::InvalidStatusTransition);
        }

        contract.status = ContractStatus::Disputed;

        env.storage().persistent().set(&key, &contract);
        ttl::extend_contract_ttl(&env, contract_id);

        env.events().publish(
            (Symbol::new(&env, "dispute"), contract_id),
            (caller, env.ledger().timestamp()),
        );
        true
    }

    /// Resolve a dispute on a contract. Blocked when paused.
    pub fn resolve_dispute(
        env: Env,
        contract_id: u32,
        arbiter: Address,
        resolution: DisputeResolution,
    ) -> bool {
        Self::require_not_paused(&env);
        arbiter.require_auth();

        let key = DataKey::Contract(contract_id);
        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        Self::require_not_finalized(&env, contract_id);

        if contract.status != ContractStatus::Disputed {
            env.panic_with_error(EscrowError::InvalidStatusTransition);
        }
        if contract.arbiter.as_ref() != Some(&arbiter) {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }

        let (client_payout, freelancer_payout) =
            resolution_payouts(&contract, &resolution)
                .unwrap_or_else(|err| env.panic_with_error(err));

        contract.refunded_amount = safe_add_amounts(contract.refunded_amount, client_payout)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));
        contract.released_amount = safe_add_amounts(contract.released_amount, freelancer_payout)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));

        if safe_add_amounts(contract.released_amount, contract.refunded_amount)
            != Some(contract.funded_amount)
        {
            env.panic_with_error(EscrowError::AccountingInvariantViolated);
        }

        contract.status = final_status_after_resolution(&contract);

        env.storage().persistent().set(&key, &contract);
        ttl::extend_contract_ttl(&env, contract_id);

        env.events().publish(
            (Symbol::new(&env, "dsp_res"), contract_id),
            (
                arbiter,
                resolution.code(),
                client_payout,
                freelancer_payout,
                env.ledger().timestamp(),
            ),
        );
        true
    }
}
