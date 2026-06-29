//! Dispute module integration and unit tests.
//!
//! These tests verify that the single canonical `raise_dispute` and
//! `resolve_dispute` entrypoints (defined once in `lib.rs`) correctly enforce
//! all security invariants and that `resolution_payouts` / `final_status_after_resolution`
//! (defined once in `dispute.rs`) produce correct arithmetic for all four
//! resolution variants.
//!
//! Conservation invariant checked after every resolution:
//!   `released_amount + refunded_amount == funded_amount`

#![cfg(test)]

use crate::{
    dispute::{final_status_after_resolution, resolution_payouts},
    Contract, ContractStatus, DisputeResolution, DisputeSplit, Escrow, EscrowClient,
    Error as EscrowError, ReleaseAuthorization,
};
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn make_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

fn make_client(env: &Env) -> (EscrowClient<'_>, Address) {
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

/// Create a funded escrow with an arbiter. Returns `(client, freelancer, arbiter, contract_id)`.
fn funded_with_arbiter(
    env: &Env,
    client: &EscrowClient,
    milestones: soroban_sdk::Vec<i128>,
    deposit: i128,
) -> (Address, Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let arbiter_addr = Address::generate(env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    client.deposit_funds(&contract_id, &client_addr, &deposit);

    (client_addr, freelancer_addr, arbiter_addr, contract_id)
}

/// Build a bare `Contract` for unit-testing `resolution_payouts` /
/// `final_status_after_resolution` without going through the full contract env.
fn payout_contract(env: &Env, funded: i128, released: i128, refunded: i128) -> Contract {
    Contract {
        client: Address::generate(env),
        freelancer: Address::generate(env),
        arbiter: Some(Address::generate(env)),
        status: ContractStatus::Disputed,
        total_deposited: funded,
        funded_amount: funded,
        released_amount: released,
        refunded_amount: refunded,
        release_authorization: ReleaseAuthorization::ClientOnly,
        reputation_issued: false,
    }
}

fn assert_conservation(client: &EscrowClient, id: u32) {
    let c = client.get_contract(&id);
    assert_eq!(
        c.released_amount + c.refunded_amount,
        c.funded_amount,
        "conservation violated: released={} refunded={} funded={}",
        c.released_amount,
        c.refunded_amount,
        c.funded_amount
    );
}

// ---------------------------------------------------------------------------
// Unit tests: resolution_payouts (pure arithmetic)
// ---------------------------------------------------------------------------

#[test]
fn resolution_payouts_full_refund_routes_all_to_client() {
    let env = make_env();
    // available = 100 - 20 - 10 = 70
    let contract = payout_contract(&env, 100, 20, 10);
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::FullRefund),
        Ok((70, 0))
    );
}

#[test]
fn resolution_payouts_full_payout_routes_all_to_freelancer() {
    let env = make_env();
    let contract = payout_contract(&env, 100, 20, 10);
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::FullPayout),
        Ok((0, 70))
    );
}

#[test]
fn resolution_payouts_partial_refund_applies_floor_rounded_30_pct_to_freelancer() {
    let env = make_env();
    // 101 available: freelancer = floor(101 * 30 / 100) = 30; client = 71
    let contract = payout_contract(&env, 101, 0, 0);
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::PartialRefund),
        Ok((71, 30))
    );
}

#[test]
fn resolution_payouts_split_accepts_exact_conserving_amounts() {
    let env = make_env();
    assert_eq!(
        resolution_payouts(
            &payout_contract(&env, 100, 0, 0),
            &DisputeResolution::Split(DisputeSplit {
                client_amount: 40,
                freelancer_amount: 60,
            })
        ),
        Ok((40, 60))
    );
}

#[test]
fn resolution_payouts_split_rejects_negative_legs() {
    let env = make_env();
    let contract = payout_contract(&env, 100, 0, 0);
    assert_eq!(
        resolution_payouts(
            &contract,
            &DisputeResolution::Split(DisputeSplit {
                client_amount: -1,
                freelancer_amount: 101,
            })
        ),
        Err(EscrowError::InvalidDisputeSplit)
    );
}

#[test]
fn resolution_payouts_split_rejects_non_conserving_sum() {
    let env = make_env();
    let contract = payout_contract(&env, 100, 0, 0);
    assert_eq!(
        resolution_payouts(
            &contract,
            &DisputeResolution::Split(DisputeSplit {
                client_amount: 40,
                freelancer_amount: 59,
            })
        ),
        Err(EscrowError::InvalidDisputeSplit)
    );
}

#[test]
fn resolution_payouts_rejects_corrupted_accounting_state() {
    let env = make_env();
    // released(70) + refunded(31) = 101 > funded(100) → available < 0
    let contract = payout_contract(&env, 100, 70, 31);
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::FullRefund),
        Err(EscrowError::AccountingInvariantViolated)
    );
}

#[test]
fn final_status_after_resolution_returns_refunded_only_when_fully_refunded() {
    let env = make_env();
    assert_eq!(
        final_status_after_resolution(&payout_contract(&env, 100, 0, 100)),
        ContractStatus::Refunded
    );
    assert_eq!(
        final_status_after_resolution(&payout_contract(&env, 100, 30, 70)),
        ContractStatus::Completed
    );
}

// ---------------------------------------------------------------------------
// Integration tests: raise_dispute
// ---------------------------------------------------------------------------

#[test]
fn client_can_raise_dispute_on_funded_contract() {
    let env = make_env();
    let (client, _) = make_client(&env);
    let (client_addr, _, _, id) =
        funded_with_arbiter(&env, &client, vec![&env, 100_i128, 200_i128], 300);

    assert!(client.raise_dispute(&id, &client_addr));
    assert_eq!(client.get_contract(&id).status, ContractStatus::Disputed);
}

#[test]
fn freelancer_can_raise_dispute_on_funded_contract() {
    let env = make_env();
    let (client, _) = make_client(&env);
    let (_, freelancer_addr, _, id) =
        funded_with_arbiter(&env, &client, vec![&env, 100_i128, 200_i128], 300);

    assert!(client.raise_dispute(&id, &freelancer_addr));
    assert_eq!(client.get_contract(&id).status, ContractStatus::Disputed);
}

#[test]
fn raise_dispute_rejects_non_party_caller() {
    let env = make_env();
    let (client, _) = make_client(&env);
    let (_, _, _, id) =
        funded_with_arbiter(&env, &client, vec![&env, 100_i128], 100);
    let outsider = Address::generate(&env);

    let result = client.try_raise_dispute(&id, &outsider);
    assert!(result.is_err());
}

#[test]
fn raise_dispute_requires_assigned_arbiter() {
    let env = make_env();
    let (client, _) = make_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&id, &client_addr, &100_i128);

    let result = client.try_raise_dispute(&id, &client_addr);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Integration tests: resolve_dispute
// ---------------------------------------------------------------------------

#[test]
fn full_refund_conserves_accounting_and_marks_refunded() {
    let env = make_env();
    let (client, _) = make_client(&env);
    let (client_addr, _, arbiter_addr, id) =
        funded_with_arbiter(&env, &client, vec![&env, 100_i128, 200_i128], 300);

    assert!(client.raise_dispute(&id, &client_addr));
    assert!(client.resolve_dispute(&id, &arbiter_addr, &DisputeResolution::FullRefund));

    let c = client.get_contract(&id);
    assert_eq!(c.status, ContractStatus::Refunded);
    assert_eq!(c.refunded_amount, 300);
    assert_eq!(c.released_amount, 0);
    assert_conservation(&client, id);
}

#[test]
fn full_payout_conserves_accounting_and_marks_completed() {
    let env = make_env();
    let (client, _) = make_client(&env);
    let (client_addr, _, arbiter_addr, id) =
        funded_with_arbiter(&env, &client, vec![&env, 150_i128], 150);

    assert!(client.raise_dispute(&id, &client_addr));
    assert!(client.resolve_dispute(&id, &arbiter_addr, &DisputeResolution::FullPayout));

    let c = client.get_contract(&id);
    assert_eq!(c.status, ContractStatus::Completed);
    assert_eq!(c.released_amount, 150);
    assert_eq!(c.refunded_amount, 0);
    assert_conservation(&client, id);
}

#[test]
fn partial_refund_applies_70_30_split_and_conserves_accounting() {
    let env = make_env();
    let (client, _) = make_client(&env);
    let (client_addr, _, arbiter_addr, id) =
        funded_with_arbiter(&env, &client, vec![&env, 100_i128], 100);

    assert!(client.raise_dispute(&id, &client_addr));
    assert!(client.resolve_dispute(&id, &arbiter_addr, &DisputeResolution::PartialRefund));

    let c = client.get_contract(&id);
    assert_eq!(c.status, ContractStatus::Completed);
    // 30% to freelancer (floor), 70% refund to client
    assert_eq!(c.released_amount, 30);
    assert_eq!(c.refunded_amount, 70);
    assert_conservation(&client, id);
}

#[test]
fn split_resolution_accepts_exact_amounts_and_conserves_accounting() {
    let env = make_env();
    let (client, _) = make_client(&env);
    let (client_addr, _, arbiter_addr, id) =
        funded_with_arbiter(&env, &client, vec![&env, 40_i128, 60_i128], 100);

    assert!(client.raise_dispute(&id, &client_addr));
    assert!(client.resolve_dispute(
        &id,
        &arbiter_addr,
        &DisputeResolution::Split(DisputeSplit {
            client_amount: 35,
            freelancer_amount: 65,
        })
    ));

    let c = client.get_contract(&id);
    assert_eq!(c.status, ContractStatus::Completed);
    assert_eq!(c.refunded_amount, 35);
    assert_eq!(c.released_amount, 65);
    assert_conservation(&client, id);
}

#[test]
fn resolve_dispute_rejects_non_arbiter_caller() {
    let env = make_env();
    let (client, _) = make_client(&env);
    let (client_addr, _, _, id) =
        funded_with_arbiter(&env, &client, vec![&env, 100_i128], 100);
    let outsider = Address::generate(&env);

    assert!(client.raise_dispute(&id, &client_addr));
    let result = client.try_resolve_dispute(&id, &outsider, &DisputeResolution::FullPayout);
    assert!(result.is_err());
}

#[test]
fn resolve_dispute_rejects_non_disputed_contract() {
    let env = make_env();
    let (client, _) = make_client(&env);
    let (_, _, arbiter_addr, id) =
        funded_with_arbiter(&env, &client, vec![&env, 100_i128], 100);

    // Contract is Funded, not Disputed
    let result = client.try_resolve_dispute(&id, &arbiter_addr, &DisputeResolution::FullRefund);
    assert!(result.is_err());
}

#[test]
fn resolve_dispute_cannot_be_called_twice() {
    let env = make_env();
    let (client, _) = make_client(&env);
    let (client_addr, _, arbiter_addr, id) =
        funded_with_arbiter(&env, &client, vec![&env, 100_i128], 100);

    assert!(client.raise_dispute(&id, &client_addr));
    assert!(client.resolve_dispute(&id, &arbiter_addr, &DisputeResolution::FullRefund));

    // Second resolution must fail (no longer Disputed)
    let result = client.try_resolve_dispute(&id, &arbiter_addr, &DisputeResolution::FullPayout);
    assert!(result.is_err());
}
