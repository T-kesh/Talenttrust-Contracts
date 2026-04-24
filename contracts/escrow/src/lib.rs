#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Bytes, Env, Symbol, Vec,
};

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
    GracePeriodNotExpired = 6,
    TermsHashAlreadySet = 7,
    InvalidGracePeriod = 8,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractData {
    pub client: Address,
    pub freelancer: Address,
    pub milestones: Vec<i128>,
    pub terms_hash: Option<Bytes>,
    pub grace_period_seconds: Option<u64>,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    NextId,
    Contract(u32),
    TermsHash(u32),
    GracePeriod(u32),
    MilestoneApprovalTime(u32, u32),
}

#[contractimpl]
impl Escrow {
    /// Hello-world style function for testing and CI.
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }

    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        milestones: Vec<i128>,
        terms_hash: Option<Bytes>,
        grace_period_seconds: Option<u64>,
    ) -> u32 {
        if client == freelancer {
            env.panic_with_error(EscrowError::InvalidParticipant);
        }
        if milestones.is_empty() {
            env.panic_with_error(EscrowError::EmptyMilestones);
        }

        for amount in milestones.iter() {
            if amount <= 0 {
                env.panic_with_error(EscrowError::InvalidMilestoneAmount);
            }
        }

        // Validate grace period if provided
        if let Some(grace_period) = grace_period_seconds {
            if grace_period == 0 {
                env.panic_with_error(EscrowError::InvalidGracePeriod);
            }
        }

        let id = env
            .storage()
            .persistent()
            .get::<_, u32>(&DataKey::NextId)
            .unwrap_or(0);

        let data = ContractData {
            client,
            freelancer,
            milestones,
            terms_hash: terms_hash.clone(),
            grace_period_seconds,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Contract(id), &data);

        // Store terms_hash separately if provided (immutable once set)
        if let Some(hash) = terms_hash {
            env.storage()
                .persistent()
                .set(&DataKey::TermsHash(id), &hash);
        }

        // Store grace period separately if provided
        if let Some(grace_period) = grace_period_seconds {
            env.storage()
                .persistent()
                .set(&DataKey::GracePeriod(id), &grace_period);
        }

        env.storage().persistent().set(&DataKey::NextId, &(id + 1));

        id
    }

    pub fn deposit_funds(env: Env, _contract_id: u32, amount: i128) -> bool {
        if amount <= 0 {
            env.panic_with_error(EscrowError::InvalidDepositAmount);
        }
        true
    }

    pub fn approve_milestone(env: Env, contract_id: u32, milestone_index: u32) -> bool {
        // Store approval time using ledger timestamp
        let approval_time = env.ledger().timestamp();
        env.storage().persistent().set(
            &DataKey::MilestoneApprovalTime(contract_id, milestone_index),
            &approval_time,
        );
        true
    }

    pub fn release_milestone(env: Env, contract_id: u32, milestone_index: u32) -> bool {
        // Check if grace period is configured
        if let Some(grace_period) = env
            .storage()
            .persistent()
            .get::<_, u64>(&DataKey::GracePeriod(contract_id))
        {
            // Get approval time
            if let Some(approval_time) =
                env.storage()
                    .persistent()
                    .get::<_, u64>(&DataKey::MilestoneApprovalTime(
                        contract_id,
                        milestone_index,
                    ))
            {
                let current_time = env.ledger().timestamp();
                let elapsed = current_time.saturating_sub(approval_time);

                if elapsed < grace_period {
                    env.panic_with_error(EscrowError::GracePeriodNotExpired);
                }
            }
        }

        true
    }

    pub fn get_terms_hash(env: Env, contract_id: u32) -> Option<Bytes> {
        env.storage()
            .persistent()
            .get::<_, Bytes>(&DataKey::TermsHash(contract_id))
    }

    pub fn get_grace_period(env: Env, contract_id: u32) -> Option<u64> {
        env.storage()
            .persistent()
            .get::<_, u64>(&DataKey::GracePeriod(contract_id))
    }

    pub fn get_milestone_approval_time(
        env: Env,
        contract_id: u32,
        milestone_index: u32,
    ) -> Option<u64> {
        env.storage()
            .persistent()
            .get::<_, u64>(&DataKey::MilestoneApprovalTime(
                contract_id,
                milestone_index,
            ))
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_grace_period;

#[cfg(test)]
mod test_terms_hash;
