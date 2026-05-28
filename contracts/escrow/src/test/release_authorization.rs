//! Tests for `release_milestone` caller authorization.
//!
//! Covers:
//! - Legitimate client can release a funded milestone.
//! - Arbitrary attacker address is rejected with `UnauthorizedRole`.
//! - Double-releasing the same milestone is rejected with `AlreadyReleased`.
//! - Freelancer (non-client) is rejected with `UnauthorizedRole`.

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use super::assert_contract_error;
use crate::{
    ContractStatus, Escrow, EscrowClient, EscrowError, ReleaseAuthorizationMode,
};

use super::assert_contract_error;

/// Register the escrow contract and return a client.
fn register(env: &Env) -> EscrowClient<'_> {
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

/// Create a fully-funded 2-milestone contract (500 + 300 = 800 total).
/// Returns `(client_addr, freelancer_addr, contract_id)`.
fn funded_contract(env: &Env, client: &EscrowClient<'_>) -> (Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = vec![env, 500_i128, 300_i128];
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &milestones,
        &DepositMode::ExactTotal,
    );
    client.deposit_funds(&id, &800_i128);
    (client_addr, freelancer_addr, id)
}

// ---------------------------------------------------------------------------
// Happy path: legitimate client releases a milestone
// ---------------------------------------------------------------------------

#[test]
fn client_can_release_funded_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register(&env);
    let (client_addr, _freelancer_addr, id) = funded_contract(&env, &client);

    assert!(client.release_milestone(&id, &client_addr, &0));

    let contract = client.get_contract(&id);
    assert_eq!(contract.released_amount, 500_i128);
}

// ---------------------------------------------------------------------------
// Attacker is rejected with UnauthorizedRole
// ---------------------------------------------------------------------------

#[test]
fn attacker_cannot_release_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register(&env);
    let (_client_addr, _freelancer_addr, id) = funded_contract(&env, &client);

    let attacker = Address::generate(&env);
    let result = client.try_release_milestone(&id, &attacker, &0);
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

// ---------------------------------------------------------------------------
// Double-release is rejected with AlreadyReleased; no duplicate transfer
// ---------------------------------------------------------------------------

#[test]
fn double_release_is_rejected_and_amount_not_duplicated() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register(&env);
    let (client_addr, _freelancer_addr, id) = funded_contract(&env, &client);

    // First release succeeds.
    assert!(client.release_milestone(&id, &client_addr, &0));

    // Second release on the same milestone must fail with AlreadyReleased.
    let result = client.try_release_milestone(&id, &client_addr, &0);
    assert_contract_error(result, EscrowError::AlreadyReleased);

    // released_amount must not be doubled.
    let contract = client.get_contract(&id);
    assert_eq!(contract.released_amount, 500_i128);
}

// ---------------------------------------------------------------------------
// Freelancer (non-client) is also rejected
// ---------------------------------------------------------------------------

#[test]
fn freelancer_cannot_release_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register(&env);
    let (_client_addr, freelancer_addr, id) = funded_contract(&env, &client);

    let result = client.try_release_milestone(&id, &freelancer_addr, &0);
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

#[test]
fn release_emits_events() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let contract_id = create_contract_with_mode(
        &env,
        &client,
        &client_addr,
        &freelancer_addr,
        &None,
        &ReleaseAuthorizationMode::ClientOnly,
    );

    fund_contract(&env, &client, &contract_id);

    // Release milestone
    client.release_milestone(&contract_id, &0, &client_addr);

    // Check release event was emitted
    let events = env.events().all();
    assert!(events.len() > 0);

    // Find the release event
    let release_event = events.iter().find(|event| {
        event.0 == soroban_sdk::symbol_short!("milestone_released")
    });
    assert!(release_event.is_some());
}

#[test]
fn rejects_double_release_and_completes_contract() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let contract_id = create_contract_with_mode(
        &env,
        &client,
        &client_addr,
        &freelancer_addr,
        &None,
        &ReleaseAuthorizationMode::ClientOnly,
    );
    fund_contract(&env, &client, &contract_id);

    assert!(client.release_milestone(&contract_id, &0, &client_addr));

    let result = client.try_release_milestone(&contract_id, &0, &client_addr);
    assert_contract_error(result, EscrowError::AlreadyReleased);

    assert!(client.release_milestone(&contract_id, &1, &client_addr));
    assert!(client.release_milestone(&contract_id, &2, &client_addr));

    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.status, ContractStatus::Completed);
    assert_eq!(client.get_pending_reputation_credits(&freelancer_addr), 1);
}

#[test]
fn rejects_refund_after_release_and_release_after_refund() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let contract_id = create_contract_with_mode(
        &env,
        &client,
        &client_addr,
        &freelancer_addr,
        &None,
        &ReleaseAuthorizationMode::ClientOnly,
    );
    fund_contract(&env, &client, &contract_id);

    assert!(client.release_milestone(&contract_id, &0, &client_addr));
    let refund_ids = vec![&env, 0_u32];
    let refund_result = client.try_refund_unreleased_milestones(&contract_id, &refund_ids);
    assert_contract_error(refund_result, EscrowError::AlreadyReleased);

    let refund_ids = vec![&env, 1_u32];
    assert!(client.refund_unreleased_milestones(&contract_id, &refund_ids));

    let result = client.try_release_milestone(&contract_id, &1, &client_addr);
    assert_contract_error(result, EscrowError::Refunded);
}
