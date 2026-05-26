//! Tests for `release_milestone` caller authorization.
//!
//! Covers:
//! - Legitimate client can release a funded milestone.
//! - Arbitrary attacker address is rejected with `UnauthorizedRole`.
//! - Double-releasing the same milestone is rejected with `AlreadyReleased`.
//! - Freelancer (non-client) is rejected with `UnauthorizedRole`.

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{Escrow, EscrowClient, EscrowError};
use crate::types::DepositMode;

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
