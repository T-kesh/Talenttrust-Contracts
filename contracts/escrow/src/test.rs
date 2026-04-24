#![cfg(test)]

use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env};

use crate::{Escrow, EscrowClient};

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

    let id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    assert_eq!(id, 0);
}

#[test]
fn test_deposit_funds() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];

    let contract_id_val = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    let result = client.deposit_funds(&contract_id_val, &1_000_0000000);
    assert!(result);
}

#[test]
fn test_release_milestone() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];

    let contract_id_val = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    client.deposit_funds(&contract_id_val, &1_200_0000000);
    client.set_milestone_funded(&contract_id_val, &0, &200_0000000);

    let result = client.release_milestone(&contract_id_val, &0);
    assert!(result);
}

#[test]
fn test_per_milestone_funding_tracking() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 100_i128, 200_i128, 300_i128];

    let contract_id_val = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    
    // Deposit funds
    assert!(client.deposit_funds(&contract_id_val, &600_i128));
    
    // Set per-milestone funding
    assert!(client.set_milestone_funded(&contract_id_val, &0, &100_i128));
    assert!(client.set_milestone_funded(&contract_id_val, &1, &200_i128));
    assert!(client.set_milestone_funded(&contract_id_val, &2, &300_i128));
    
    // Verify funding amounts
    assert_eq!(client.get_milestone_funded(&contract_id_val, &0), 100_i128);
    assert_eq!(client.get_milestone_funded(&contract_id_val, &1), 200_i128);
    assert_eq!(client.get_milestone_funded(&contract_id_val, &2), 300_i128);
}
