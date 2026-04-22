#![cfg(test)]

mod cancel_contract;

use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env};

use crate::{ContractStatus, Escrow, EscrowClient};

#[test]
fn test_hello() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let result = client.hello(&symbol_short!("World"));
    assert_eq!(result, symbol_short!("World"));
}

#[test]
fn test_create_contract() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];

    let id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones);
    assert_eq!(id, 0);

    // Verify contract was created with correct status
    let contract = client.get_contract(&id);
    assert_eq!(contract.status, ContractStatus::Created);
}

#[test]
fn test_deposit_funds() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    // Create a contract first
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];
    let id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones);

    // Now deposit
    let result = client.deposit_funds(&id, &1_000_0000000);
    assert!(result);
}

#[test]
fn test_release_milestone() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    // Create and fund a contract first
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];
    let id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones);
    client.deposit_funds(&id, &1_000_0000000);

    // Now release milestone
    let result = client.release_milestone(&id, &0);
    assert!(result);
}
