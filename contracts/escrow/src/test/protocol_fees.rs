#![cfg(test)]

use crate::{Escrow, EscrowClient, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

#[test]
fn test_default_fees_are_zero() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    // Default values before initialization or setting must be 0
    assert_eq!(client.get_protocol_fee_bps(), 0);
    assert_eq!(client.get_accumulated_protocol_fees(), 0);
}

#[test]
fn test_fee_rate_view() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin);

    // Initial value is 0
    assert_eq!(client.get_protocol_fee_bps(), 0);

    // Set fee to 500 bps (5%)
    client.set_protocol_fee_bps(&500u32);
    assert_eq!(client.get_protocol_fee_bps(), 500);

    // Update fee to 1250 bps (12.5%)
    client.set_protocol_fee_bps(&1250u32);
    assert_eq!(client.get_protocol_fee_bps(), 1250);
}

#[test]
fn test_fee_accrual_readers() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    // Initialize escrow
    client.initialize(&admin);

    // Set protocol fee rate to 1000 bps (10%)
    client.set_protocol_fee_bps(&1000u32);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_i128, 2500_i128, 3333_i128];

    // Create contract
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Deposit total milestone amount (6833)
    client.deposit_funds(&id, &client_addr, &6833_i128);

    // Before release, accumulated fees should be 0
    assert_eq!(client.get_accumulated_protocol_fees(), 0);

    // Approve and release milestone 0 (1000)
    // Fee: 1000 * 1000 / 10000 = 100
    assert!(client.approve_milestone_release(&id, &client_addr, &0));
    assert!(client.release_milestone(&id, &client_addr, &0));
    assert_eq!(client.get_accumulated_protocol_fees(), 100);

    // Approve and release milestone 1 (2500)
    // Fee: 2500 * 1000 / 10000 = 250
    // Cumulative: 100 + 250 = 350
    assert!(client.approve_milestone_release(&id, &client_addr, &1));
    assert!(client.release_milestone(&id, &client_addr, &1));
    assert_eq!(client.get_accumulated_protocol_fees(), 350);

    // Approve and release milestone 2 (3333)
    // Fee: 3333 * 1000 / 10000 = 333
    // Cumulative: 350 + 333 = 683
    assert!(client.approve_milestone_release(&id, &client_addr, &2));
    assert!(client.release_milestone(&id, &client_addr, &2));
    assert_eq!(client.get_accumulated_protocol_fees(), 683);
}

#[test]
fn test_fee_accrual_zero_rate() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin);

    // Explicitly set fee rate to 0 bps (0%)
    client.set_protocol_fee_bps(&0u32);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 5000_i128];

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    client.deposit_funds(&id, &client_addr, &5000_i128);

    assert_eq!(client.get_accumulated_protocol_fees(), 0);

    // Release milestone with 0% fee rate
    assert!(client.approve_milestone_release(&id, &client_addr, &0));
    assert!(client.release_milestone(&id, &client_addr, &0));

    // Fees should remain 0
    assert_eq!(client.get_accumulated_protocol_fees(), 0);
}
