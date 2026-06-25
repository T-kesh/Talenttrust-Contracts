//! Tests for `raise_dispute`, `resolve_dispute`, `resolve_dispute_split`,
//! and `get_dispute`.
//!
//! These tests are aligned with the dispute.rs module exposed by the
//! `Escrow` contract: the arbiter-only flow guarded by auth and contract
//! state, the dedicated `Split` entry point with its accounting
//! invariants, and metadata persistence — all as required by issue #486.

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, BytesN, Env};

use crate::{ContractStatus, DisputeResolution, DisputeSplit, Escrow, EscrowClient};

// ── helpers ──────────────────────────────────────────────────────────────────

fn register(env: &Env) -> EscrowClient<'_> {
    EscrowClient::new(env, &env.register(Escrow, ()))
}

fn participants(env: &Env) -> (Address, Address, Address) {
    (
        Address::generate(env),
        Address::generate(env),
        Address::generate(env),
    )
}

fn reason_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0xabu8; 32])
}

/// Create a funded contract with an arbiter registered.
///
/// Mirrors the contract's public surface: `create_contract_with_arbiter`
/// followed by a full deposit so the contract is `Funded` and ready for
/// `raise_dispute`.
fn funded_with_arbiter(env: &Env, escrow: &EscrowClient<'_>) -> (Address, Address, Address, u32) {
    let (client_addr, freelancer_addr, arbiter_addr) = participants(env);
    let milestones = vec![env, 100_i128, 200_i128];
    let id = escrow.create_contract_with_arbiter(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &crate::DepositMode::ExactTotal,
    );
    // Full deposit: 100 + 200 = 300.
    escrow.deposit_funds(&id, &300_i128);
    (client_addr, freelancer_addr, arbiter_addr, id)
}

// ── raise_dispute happy paths ─────────────────────────────────────────────────

#[test]
fn client_can_raise_dispute_on_funded_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, _, id) = funded_with_arbiter(&env, &escrow);

    assert!(escrow.raise_dispute(&id, &client_addr, &reason_hash(&env)));

    let contract = escrow.get_contract(&id);
    assert_eq!(contract.status, ContractStatus::Disputed);
}

#[test]
fn freelancer_can_raise_dispute_on_funded_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (_, freelancer_addr, _, id) = funded_with_arbiter(&env, &escrow);

    assert!(escrow.raise_dispute(&id, &freelancer_addr, &reason_hash(&env)));

    let contract = escrow.get_contract(&id);
    assert_eq!(contract.status, ContractStatus::Disputed);
}

#[test]
fn partial_funding_still_admits_dispute() {
    // `PartiallyFunded` should also admit disputes: in practice an
    // under-funded contract may still need arbiter intervention.
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, freelancer_addr, arbiter_addr) = participants(&env);
    let milestones = vec![&env, 100_i128, 200_i128];
    let id = escrow.create_contract_with_arbiter(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &milestones,
        &crate::DepositMode::Incremental,
    );
    // Deposit only half — contract will be PartiallyFunded.
    escrow.deposit_funds(&id, &150_i128);

    assert!(escrow.raise_dispute(&id, &client_addr, &reason_hash(&env)));

    let contract = escrow.get_contract(&id);
    assert_eq!(contract.status, ContractStatus::Disputed);
}

#[test]
fn raise_dispute_stores_metadata() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, _, id) = funded_with_arbiter(&env, &escrow);
    let hash = reason_hash(&env);

    escrow.raise_dispute(&id, &client_addr, &hash);

    let meta = escrow.get_dispute(&id);
    assert_eq!(meta.reason_hash, hash);
    assert_eq!(meta.raised_by, client_addr);
}

// ── raise_dispute error paths ─────────────────────────────────────────────────

#[test]
#[should_panic]
fn arbiter_cannot_raise_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (_, _, arbiter_addr, id) = funded_with_arbiter(&env, &escrow);

    escrow.raise_dispute(&id, &arbiter_addr, &reason_hash(&env));
}

#[test]
#[should_panic]
fn third_party_cannot_raise_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (_, _, _, id) = funded_with_arbiter(&env, &escrow);
    let outsider = Address::generate(&env);

    escrow.raise_dispute(&id, &outsider, &reason_hash(&env));
}

#[test]
#[should_panic]
fn cannot_raise_dispute_without_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 100_i128];
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &milestones,
        &crate::DepositMode::ExactTotal,
    );
    escrow.deposit_funds(&id, &100_i128);

    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));
}

#[test]
#[should_panic]
fn cannot_raise_dispute_on_created_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, freelancer_addr, arbiter_addr) = participants(&env);
    let milestones = vec![&env, 100_i128];
    let id = escrow.create_contract_with_arbiter(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &milestones,
        &crate::DepositMode::ExactTotal,
    );
    // Not funded — should fail.

    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));
}

#[test]
#[should_panic]
fn cannot_raise_dispute_twice() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, _, id) = funded_with_arbiter(&env, &escrow);

    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));
    // Already Disputed, not Funded — second call must fail.
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));
}

// ── resolve_dispute happy paths ───────────────────────────────────────────────

#[test]
fn arbiter_can_resolve_with_release() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, arbiter_addr, id) = funded_with_arbiter(&env, &escrow);
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));

    assert!(escrow.resolve_dispute(&id, &arbiter_addr, &DisputeResolution::Release));

    let contract = escrow.get_contract(&id);
    assert_eq!(contract.status, ContractStatus::Completed);
    assert_eq!(contract.released_amount, 300_i128);
    assert_eq!(contract.refunded_amount, 0);
}

#[test]
fn arbiter_can_resolve_with_refund() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, arbiter_addr, id) = funded_with_arbiter(&env, &escrow);
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));

    assert!(escrow.resolve_dispute(&id, &arbiter_addr, &DisputeResolution::Refund));

    let contract = escrow.get_contract(&id);
    assert_eq!(contract.status, ContractStatus::Refunded);
    assert_eq!(contract.refunded_amount, 300_i128);
    assert_eq!(contract.released_amount, 0);
}

#[test]
fn arbiter_can_resolve_with_cancel() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, arbiter_addr, id) = funded_with_arbiter(&env, &escrow);
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));

    assert!(escrow.resolve_dispute(&id, &arbiter_addr, &DisputeResolution::Cancel));

    let contract = escrow.get_contract(&id);
    assert_eq!(contract.status, ContractStatus::Cancelled);
}

// ── resolve_dispute_split happy paths ────────────────────────────────────────

#[test]
fn arbiter_can_resolve_with_split_persists_accounting() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, arbiter_addr, id) = funded_with_arbiter(&env, &escrow);
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));

    // Split 300 → 100 to client, 200 to freelancer.
    let split = DisputeSplit {
        client_amount: 100,
        freelancer_amount: 200,
    };
    assert!(escrow.resolve_dispute_split(&id, &arbiter_addr, &split));

    let contract = escrow.get_contract(&id);
    assert_eq!(contract.refunded_amount, 100);
    assert_eq!(contract.released_amount, 200);
    assert_eq!(contract.status, ContractStatus::Funded);
}

#[test]
fn split_must_sum_to_available_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, arbiter_addr, id) = funded_with_arbiter(&env, &escrow);
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));

    // 50 + 100 = 150, but available is 300 → AccountingInvariantViolated.
    let split = DisputeSplit {
        client_amount: 50,
        freelancer_amount: 100,
    };
    let result = escrow.try_resolve_dispute_split(&id, &arbiter_addr, &split);
    crate::test::assert_contract_error(result, crate::EscrowError::AccountingInvariantViolated);
}

#[test]
fn split_rejects_negative_components() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, arbiter_addr, id) = funded_with_arbiter(&env, &escrow);
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));

    // Splits with a negative component are rejected by NonPositiveAmount.
    let split = DisputeSplit {
        client_amount: -1,
        freelancer_amount: 301,
    };
    let result = escrow.try_resolve_dispute_split(&id, &arbiter_addr, &split);
    crate::test::assert_contract_error(result, crate::EscrowError::NonPositiveAmount);
}

#[test]
fn split_all_to_client_terminates_as_refunded() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, arbiter_addr, id) = funded_with_arbiter(&env, &escrow);
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));

    let split = DisputeSplit {
        client_amount: 300,
        freelancer_amount: 0,
    };
    assert!(escrow.resolve_dispute_split(&id, &arbiter_addr, &split));

    let contract = escrow.get_contract(&id);
    assert_eq!(contract.status, ContractStatus::Refunded);
}

#[test]
fn split_all_to_freelancer_terminates_as_completed() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, arbiter_addr, id) = funded_with_arbiter(&env, &escrow);
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));

    let split = DisputeSplit {
        client_amount: 0,
        freelancer_amount: 300,
    };
    assert!(escrow.resolve_dispute_split(&id, &arbiter_addr, &split));

    let contract = escrow.get_contract(&id);
    assert_eq!(contract.status, ContractStatus::Completed);
}

// ── resolve_dispute error paths ───────────────────────────────────────────────

#[test]
#[should_panic]
fn client_cannot_resolve_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, _, id) = funded_with_arbiter(&env, &escrow);
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));

    escrow.resolve_dispute(&id, &client_addr, &DisputeResolution::Release);
}

#[test]
#[should_panic]
fn freelancer_cannot_resolve_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, freelancer_addr, _, id) = funded_with_arbiter(&env, &escrow);
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));

    escrow.resolve_dispute(&id, &freelancer_addr, &DisputeResolution::Release);
}

#[test]
#[should_panic]
fn third_party_cannot_resolve_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, _, id) = funded_with_arbiter(&env, &escrow);
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));
    let outsider = Address::generate(&env);

    escrow.resolve_dispute(&id, &outsider, &DisputeResolution::Release);
}

#[test]
#[should_panic]
fn cannot_resolve_non_disputed_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (_, _, arbiter_addr, id) = funded_with_arbiter(&env, &escrow);
    // Not disputed yet — should fail with InvalidState.

    escrow.resolve_dispute(&id, &arbiter_addr, &DisputeResolution::Release);
}

#[test]
#[should_panic]
fn third_party_cannot_resolve_dispute_split() {
    // Mirror of the resolve_dispute auth check for the split entry point.
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, _, id) = funded_with_arbiter(&env, &escrow);
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));
    let outsider = Address::generate(&env);

    let split = DisputeSplit {
        client_amount: 150,
        freelancer_amount: 150,
    };
    escrow.resolve_dispute_split(&id, &outsider, &split);
}

#[test]
#[should_panic]
fn cannot_resolve_dispute_split_on_non_disputed() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (_, _, arbiter_addr, id) = funded_with_arbiter(&env, &escrow);

    let split = DisputeSplit {
        client_amount: 150,
        freelancer_amount: 150,
    };
    escrow.resolve_dispute_split(&id, &arbiter_addr, &split);
}

// ── state blocking ────────────────────────────────────────────────────────────

#[test]
#[should_panic]
fn release_milestone_blocked_in_disputed_state() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, _, id) = funded_with_arbiter(&env, &escrow);
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));

    escrow.release_milestone(&id, &0);
}

// ── get_dispute error path ────────────────────────────────────────────────────

#[test]
#[should_panic]
fn get_dispute_fails_when_no_dispute_exists() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (_, _, _, id) = funded_with_arbiter(&env, &escrow);

    escrow.get_dispute(&id);
}

// ── pause / accountability ────────────────────────────────────────────────────

#[test]
#[should_panic]
fn pause_blocks_raise_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, _, id) = funded_with_arbiter(&env, &escrow);

    // Initialise then pause so require_not_paused fires. Note that
    // raise_dispute and resolve_dispute themselves do not require
    // initialize — they only require `not_paused`.
    escrow.initialize(&Address::generate(&env));
    escrow.pause();
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));
}

#[test]
#[should_panic]
fn pause_blocks_resolve_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, arbiter_addr, id) = funded_with_arbiter(&env, &escrow);
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));

    escrow.initialize(&Address::generate(&env));
    escrow.pause();
    escrow.resolve_dispute(&id, &arbiter_addr, &DisputeResolution::Release);
}

#[test]
#[should_panic]
fn pause_blocks_resolve_dispute_split() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, arbiter_addr, id) = funded_with_arbiter(&env, &escrow);
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));

    escrow.initialize(&Address::generate(&env));
    escrow.pause();
    let split = DisputeSplit {
        client_amount: 100,
        freelancer_amount: 200,
    };
    escrow.resolve_dispute_split(&id, &arbiter_addr, &split);
}
