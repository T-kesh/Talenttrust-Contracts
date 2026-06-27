use super::{
    assert_contract_error, create_contract, register_client, total_milestone_amount, MILESTONE_ONE,
    MILESTONE_THREE, MILESTONE_TWO,
};
use crate::{Contract, ContractStatus, DataKey, Error, Milestone, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, vec, Address, Env, Symbol, Vec};

fn milestone_funding(env: &Env, client: &crate::EscrowClient<'_>, contract_id: u32) -> Vec<i128> {
    let milestones = client.get_milestones(&contract_id);
    let mut funding = Vec::new(env);
    for milestone in milestones.iter() {
        funding.push_back(milestone.funded_amount);
    }
    funding
}

#[test]
fn partial_deposit_allocates_only_to_first_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, contract_id) = create_contract(&env, &client);
    let deposit = MILESTONE_ONE / 2;

    assert!(client.deposit_funds(&contract_id, &client_addr, &deposit));

    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.status, ContractStatus::Created);
    assert_eq!(contract.funded_amount, deposit);
    assert_eq!(
        milestone_funding(&env, &client, contract_id),
        vec![&env, deposit, 0, 0]
    );
}

#[test]
fn spanning_deposit_fills_milestones_in_order() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, contract_id) = create_contract(&env, &client);
    let deposit = MILESTONE_ONE + (MILESTONE_TWO / 2);

    assert!(client.deposit_funds(&contract_id, &client_addr, &deposit));

    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.status, ContractStatus::Created);
    assert_eq!(contract.funded_amount, deposit);
    assert_eq!(
        milestone_funding(&env, &client, contract_id),
        vec![&env, MILESTONE_ONE, MILESTONE_TWO / 2, 0]
    );
}

#[test]
fn incremental_deposits_resume_at_next_unfunded_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, contract_id) = create_contract(&env, &client);
    let first_deposit = MILESTONE_ONE + (MILESTONE_TWO / 2);
    let second_deposit = (MILESTONE_TWO / 2) + (MILESTONE_THREE / 3);

    assert!(client.deposit_funds(&contract_id, &client_addr, &first_deposit));
    assert!(client.deposit_funds(&contract_id, &client_addr, &second_deposit));

    assert_eq!(
        milestone_funding(&env, &client, contract_id),
        vec![&env, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE / 3]
    );
    assert_eq!(
        client.get_contract(&contract_id).funded_amount,
        first_deposit + second_deposit
    );
}

#[test]
fn exact_total_deposit_funds_all_milestones_and_preserves_aggregate() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, contract_id) = create_contract(&env, &client);

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestone_amount()));

    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.status, ContractStatus::Funded);
    assert_eq!(contract.funded_amount, total_milestone_amount());
    assert_eq!(
        milestone_funding(&env, &client, contract_id),
        vec![&env, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE]
    );
}

#[test]
fn overfunding_is_rejected_without_allocating_to_milestones() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, contract_id) = create_contract(&env, &client);

    assert_contract_error(
        client.try_deposit_funds(&contract_id, &client_addr, &(total_milestone_amount() + 1)),
        Error::FundingExceedsRequired,
    );

    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.status, ContractStatus::Created);
    assert_eq!(contract.funded_amount, 0);
    assert_eq!(
        milestone_funding(&env, &client, contract_id),
        vec![&env, 0, 0, 0]
    );
}

#[test]
fn release_rejects_legacy_aggregate_funding_without_milestone_allocation() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    env.as_contract(&client.address, || {
        let contract_key = DataKey::Contract(contract_id);
        let mut contract: Contract = env.storage().persistent().get(&contract_key).unwrap();
        contract.status = ContractStatus::Funded;
        contract.funded_amount = total_milestone_amount();
        env.storage().persistent().set(&contract_key, &contract);

        let milestone_key = Symbol::new(&env, "milestones");
        let mut stored: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key.clone()))
            .unwrap();
        let mut first = stored.get(0).unwrap();
        first.funded_amount = MILESTONE_ONE - 1;
        stored.set(0, first);
        env.storage()
            .persistent()
            .set(&(DataKey::Contract(contract_id), milestone_key), &stored);
    });

    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert_contract_error(
        client.try_release_milestone(&contract_id, &client_addr, &0),
        Error::InsufficientFunds,
    );
}

/// Tests that deposit_funds panics with UnauthorizedRole when caller is not the depositor.
///
/// Asserts the exact error code when an unauthorized address attempts to deposit.
/// 
/// # Security
/// - Prevents unauthorized fund deposits
/// - Enforces client-only deposit authorization
#[test]
fn test_deposit_unauthorized_role() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = create_contract(&env, &client);

    // Attempt deposit from wrong caller (freelancer instead of client)
    let wrong_caller = Address::generate(&env);
    let result = client.try_deposit_funds(&contract_id, &wrong_caller, &100_0000000_i128);
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

/// Tests that deposit_funds panics with InvalidState when contract is not in Created state.
///
/// Asserts the exact error code when attempting to deposit after contract has been funded.
/// 
/// # Security
/// - Prevents state machine violations
/// - Ensures deposits only occur during contract setup phase
#[test]
fn test_deposit_invalid_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = create_contract(&env, &client);

    // Fully fund the contract first (transitions to Funded state)
    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestone_amount()));

    // Try to deposit again (contract is now Funded, not Created)
    let result = client.try_deposit_funds(&contract_id, &client_addr, &100_0000000_i128);
    assert_contract_error(result, EscrowError::InvalidState);
}

/// Tests that deposit_funds panics with InsufficientFunds when caller token balance is too low.
///
/// Note: In Soroban test environment with mocked auth, balance checks are typically bypassed.
/// This test documents the error branch but may not be directly testable without token contract integration.
/// 
/// # UNREACHABLE
/// InsufficientFunds in deposit_funds is currently unreachable because:
/// - The contract does not perform balance verification in the current implementation
/// - Token transfer is mocked in test environment
/// - Real balance checks occur only at the token contract level during actual transfers
///
/// Documented per Issue #405 requirements for completeness.
#[test]
#[ignore]
fn test_deposit_insufficient_funds() {
    // UNREACHABLE: deposit_funds does not check caller's token balance
    // in the current implementation. Balance validation occurs at token contract level.
    // This test is documented for completeness but cannot be triggered in unit tests.
}

/// Tests that per-milestone funded_amount is distributed correctly across milestones.
///
/// When a deposit is made, the amount is distributed in milestone order, filling
/// each milestone's `funded_amount` up to its `amount` before moving to the next.
#[test]
fn deposit_distributes_funds_per_milestone() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    // Deposit 250: fills milestone[0] (200) + 50 toward milestone[1]
    assert!(client.deposit_funds(&contract_id, &client_addr, &250_0000000_i128));
    let milestones = client.get_milestones(&contract_id);
    assert_eq!(milestones.get(0).unwrap().funded_amount, 200_0000000_i128);
    assert_eq!(milestones.get(1).unwrap().funded_amount, 50_0000000_i128);
    assert_eq!(milestones.get(2).unwrap().funded_amount, 0_i128);

    // Deposit remaining 950: fills milestone[1] (remaining 350) + milestone[2] (600)
    assert!(client.deposit_funds(&contract_id, &client_addr, &950_0000000_i128));
    let milestones = client.get_milestones(&contract_id);
    assert_eq!(milestones.get(0).unwrap().funded_amount, 200_0000000_i128);
    assert_eq!(milestones.get(1).unwrap().funded_amount, 400_0000000_i128);
    assert_eq!(milestones.get(2).unwrap().funded_amount, 600_0000000_i128);

    // Verify contract-level sum matches per-milestone sum
    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.funded_amount, 1_200_0000000_i128);
}

/// Tests that per-milestone funded_amount remains consistent after partial deposits.
#[test]
fn deposit_partial_fills_first_milestones_only() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    // Deposit 50: only milestone[0] gets partial funding
    assert!(client.deposit_funds(&contract_id, &client_addr, &50_0000000_i128));
    let milestones = client.get_milestones(&contract_id);
    assert_eq!(milestones.get(0).unwrap().funded_amount, 50_0000000_i128);
    assert_eq!(milestones.get(1).unwrap().funded_amount, 0_i128);
    assert_eq!(milestones.get(2).unwrap().funded_amount, 0_i128);

    // Invariant: per-milestone sum == contract funded_amount
    let contract = client.get_contract(&contract_id);
    let total: i128 = milestones.iter().map(|m| m.funded_amount).sum();
    assert_eq!(total, contract.funded_amount);
}

