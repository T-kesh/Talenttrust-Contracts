#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Symbol, Vec};

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
    InsufficientMilestoneFunding = 6,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractData {
    pub client: Address,
    pub freelancer: Address,
    pub milestones: Vec<i128>,
    pub total_funded: i128,
    pub total_released: i128,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    NextId,
    Contract(u32),
    MilestoneFunded(u32, u32),
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

        let id = env
            .storage()
            .persistent()
            .get::<_, u32>(&DataKey::NextId)
            .unwrap_or(0);

        let data = ContractData {
            client,
            freelancer,
            milestones,
            total_funded: 0,
            total_released: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Contract(id), &data);
        env.storage().persistent().set(&DataKey::NextId, &(id + 1));

        id
    }

    pub fn deposit_funds(env: Env, contract_id: u32, amount: i128) -> bool {
        if amount <= 0 {
            env.panic_with_error(EscrowError::InvalidDepositAmount);
        }

        let mut contract: ContractData = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .expect("Contract not found");

        let total_amount: i128 = contract.milestones.iter().sum();
        if contract.total_funded + amount > total_amount {
            env.panic_with_error(EscrowError::InvalidDepositAmount);
        }

        contract.total_funded += amount;
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        true
    }

    pub fn release_milestone(env: Env, contract_id: u32, milestone_index: u32) -> bool {
        let mut contract: ContractData = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .expect("Contract not found");

        if milestone_index >= contract.milestones.len() {
            env.panic_with_error(EscrowError::InvalidMilestone);
        }

        let milestone_amount = contract.milestones.get(milestone_index).unwrap();
        let funded_key = DataKey::MilestoneFunded(contract_id, milestone_index);
        let funded_amount: i128 = env
            .storage()
            .persistent()
            .get(&funded_key)
            .unwrap_or(0);

        if funded_amount < milestone_amount {
            env.panic_with_error(EscrowError::InsufficientMilestoneFunding);
        }

        contract.total_released += milestone_amount;
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        true
    }

    pub fn get_milestone_funded(env: Env, contract_id: u32, milestone_index: u32) -> i128 {
        let funded_key = DataKey::MilestoneFunded(contract_id, milestone_index);
        env.storage()
            .persistent()
            .get(&funded_key)
            .unwrap_or(0)
    }

    pub fn set_milestone_funded(
        env: Env,
        contract_id: u32,
        milestone_index: u32,
        amount: i128,
    ) -> bool {
        if amount < 0 {
            env.panic_with_error(EscrowError::InvalidDepositAmount);
        }

        let contract: ContractData = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .expect("Contract not found");

        if milestone_index >= contract.milestones.len() {
            env.panic_with_error(EscrowError::InvalidMilestone);
        }

        let funded_key = DataKey::MilestoneFunded(contract_id, milestone_index);
        env.storage()
            .persistent()
            .set(&funded_key, &amount);

        true
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_per_milestone_funding;
