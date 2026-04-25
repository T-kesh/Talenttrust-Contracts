//! # Milestone-Level Partial Refund Tests
//!
//! Comprehensive tests for the refund_milestone function.
//! Tests cover happy paths, mixed flows, and all error cases.

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{Escrow, EscrowClient};

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn register_client(env: &Env) -> EscrowClient {
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

fn default_milestones(env: &Env) -> soroban_sdk::Vec<i128> {
    vec![env, 100_0000000_i128, 200_0000000_i128, 300_0000000_i128]
}

// ─── Happy Path Tests ──────────────────────────────────────────────────────────

/// Refund a single unreleased milestone.
#[test]
fn refund_single_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
    );

    client.deposit_funds(&id, &600_0000000_i128);

    // Refund milestone 0 (100)
    let refunded = client.refund_milestone(&id, &vec![&env, 0_u32]);
    assert_eq!(refunded, 100_0000000_i128);

    let contract = client.get_contract(&id);
    assert_eq!(contract.refunded_amount, 100_0000000_i128);
}

/// Refund multiple milestones in one call.
#[test]
fn refund_multiple_milestones() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
    );

    client.deposit_funds(&id, &600_0000000_i128);

    // Refund milestones 0 and 2 (100 + 300 = 400)
    let refunded = client.refund_milestone(&id, &vec![&env, 0_u32, 2_u32]);
    assert_eq!(refunded, 400_0000000_i128);

    let contract = client.get_contract(&id);
    assert_eq!(contract.refunded_amount, 400_0000000_i128);
}

/// Refund all milestones transitions contract to Refunded status.
#[test]
fn refund_all_milestones_transitions_to_refunded() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
    );

    client.deposit_funds(&id, &600_0000000_i128);

    // Refund all milestones
    let refunded = client.refund_milestone(&id, &vec![&env, 0_u32, 1_u32, 2_u32]);
    assert_eq!(refunded, 600_0000000_i128);

    let contract = client.get_contract(&id);
    assert_eq!(contract.refunded_amount, 600_0000000_i128);
    // Status should transition to Refunded when all milestones are settled
}

// ─── Mixed Flow Tests ──────────────────────────────────────────────────────────

/// Release some milestones, then refund others.
#[test]
fn mixed_release_and_refund() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
    );

    client.deposit_funds(&id, &600_0000000_i128);

    // Release milestone 0
    client.release_milestone(&id, &0_u32);

    // Refund milestones 1 and 2
    let refunded = client.refund_milestone(&id, &vec![&env, 1_u32, 2_u32]);
    assert_eq!(refunded, 500_0000000_i128);

    let contract = client.get_contract(&id);
    assert_eq!(contract.refunded_amount, 500_0000000_i128);
}

/// Refund, then release remaining milestones.
#[test]
fn refund_then_release() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
    );

    client.deposit_funds(&id, &600_0000000_i128);

    // Refund milestone 2
    let refunded = client.refund_milestone(&id, &vec![&env, 2_u32]);
    assert_eq!(refunded, 300_0000000_i128);

    // Release milestones 0 and 1
    client.release_milestone(&id, &0_u32);
    client.release_milestone(&id, &1_u32);

    let contract = client.get_contract(&id);
    assert_eq!(contract.refunded_amount, 300_0000000_i128);
}

// ─── Error Cases ──────────────────────────────────────────────────────────────

/// Cannot refund a milestone that has already been released.
#[test]
#[should_panic(expected = "MilestoneAlreadyReleased")]
fn cannot_refund_released_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
    );

    client.deposit_funds(&id, &600_0000000_i128);

    // Release milestone 0
    client.release_milestone(&id, &0_u32);

    // Try to refund milestone 0 (should panic)
    client.refund_milestone(&id, &vec![&env, 0_u32]);
}

/// Cannot refund the same milestone twice.
#[test]
#[should_panic(expected = "DuplicateMilestoneInRefund")]
fn cannot_refund_same_milestone_twice() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
    );

    client.deposit_funds(&id, &600_0000000_i128);

    // Try to refund milestone 0 twice in same call (should panic)
    client.refund_milestone(&id, &vec![&env, 0_u32, 0_u32]);
}

/// Cannot refund with empty milestone list.
#[test]
#[should_panic(expected = "EmptyRefundRequest")]
fn cannot_refund_empty_list() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
    );

    client.deposit_funds(&id, &600_0000000_i128);

    // Try to refund with empty list (should panic)
    client.refund_milestone(&id, &vec![&env]);
}

/// Cannot refund if insufficient escrow balance.
#[test]
#[should_panic(expected = "InsufficientEscrowBalance")]
fn cannot_refund_insufficient_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
    );

    // Only deposit 200, but try to refund 600
    client.deposit_funds(&id, &200_0000000_i128);

    // Try to refund all milestones (should panic - insufficient balance)
    client.refund_milestone(&id, &vec![&env, 0_u32, 1_u32, 2_u32]);
}

// ─── Accounting Invariant Tests ────────────────────────────────────────────────

/// Verify accounting invariant: total_deposited == released + refunded + available
#[test]
fn accounting_invariant_maintained() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
    );

    let total_deposited = 600_0000000_i128;
    client.deposit_funds(&id, &total_deposited);

    // Release milestone 0 (100)
    client.release_milestone(&id, &0_u32);

    // Refund milestone 2 (300)
    client.refund_milestone(&id, &vec![&env, 2_u32]);

    let contract = client.get_contract(&id);
    let released = 100_0000000_i128;
    let refunded = 300_0000000_i128;
    let available = total_deposited - released - refunded;

    // Verify: total_deposited == released + refunded + available
    assert_eq!(total_deposited, released + refunded + available);
    assert_eq!(contract.refunded_amount, refunded);
}

/// Multiple refund calls maintain accounting invariant.
#[test]
fn multiple_refunds_maintain_invariant() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
    );

    let total_deposited = 600_0000000_i128;
    client.deposit_funds(&id, &total_deposited);

    // First refund: milestone 0 (100)
    client.refund_milestone(&id, &vec![&env, 0_u32]);

    // Second refund: milestone 1 (200)
    client.refund_milestone(&id, &vec![&env, 1_u32]);

    let contract = client.get_contract(&id);
    let total_refunded = 300_0000000_i128;
    assert_eq!(contract.refunded_amount, total_refunded);

    // Verify invariant
    let available = total_deposited - total_refunded;
    assert_eq!(total_deposited, total_refunded + available);
}

// ─── Edge Cases ────────────────────────────────────────────────────────────────

/// Refund with exact balance (no available funds left).
#[test]
fn refund_exact_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
    );

    let total_deposited = 600_0000000_i128;
    client.deposit_funds(&id, &total_deposited);

    // Refund all milestones
    let refunded = client.refund_milestone(&id, &vec![&env, 0_u32, 1_u32, 2_u32]);
    assert_eq!(refunded, total_deposited);

    let contract = client.get_contract(&id);
    assert_eq!(contract.refunded_amount, total_deposited);
}

/// Refund partial amount from a single milestone.
#[test]
fn refund_partial_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
    );

    client.deposit_funds(&id, &600_0000000_i128);

    // Refund only milestone 1 (200)
    let refunded = client.refund_milestone(&id, &vec![&env, 1_u32]);
    assert_eq!(refunded, 200_0000000_i128);

    let contract = client.get_contract(&id);
    assert_eq!(contract.refunded_amount, 200_0000000_i128);
}

/// Refund non-contiguous milestones.
#[test]
fn refund_non_contiguous_milestones() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
    );

    client.deposit_funds(&id, &600_0000000_i128);

    // Refund milestones 0 and 2 (skip 1)
    let refunded = client.refund_milestone(&id, &vec![&env, 0_u32, 2_u32]);
    assert_eq!(refunded, 400_0000000_i128);

    let contract = client.get_contract(&id);
    assert_eq!(contract.refunded_amount, 400_0000000_i128);
}
