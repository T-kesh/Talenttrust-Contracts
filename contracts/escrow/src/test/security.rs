use super::{create_contract, create_contract_with_arbiter, default_milestones, generated_participants, register_client, total_milestone_amount};
use crate::{DisputeResolution, Escrow, EscrowClient, EscrowError, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, vec, Address, Env, Vec};

#[test]
fn create_rejects_same_participants() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (addr, _) = generated_participants(&env);

    let result =
        client.try_create_contract(&addr, &addr, &None, &default_milestones(&env), &ReleaseAuthorization::ClientOnly);
    super::assert_contract_error(result, EscrowError::InvalidParticipant);
}

#[test]
fn create_rejects_empty_milestone_list() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let empty = Vec::<i128>::new(&env);

    let result =
        client.try_create_contract(&client_addr, &freelancer_addr, &None, &empty, &ReleaseAuthorization::ClientOnly);
    super::assert_contract_error(result, EscrowError::EmptyMilestones);
}

#[test]
fn create_rejects_non_positive_milestone_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let milestones = vec![&env, 100_i128, 0_i128];

    let result = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    super::assert_contract_error(result, EscrowError::InvalidMilestoneAmount);
}

#[test]
#[should_panic]
fn create_requires_client_authorization() {
    let env = Env::default();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);

    let _ = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
}

#[test]
fn deposit_rejects_non_positive_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    let result = client.try_deposit_funds(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, EscrowError::InvalidDepositAmount);
}

#[test]
fn release_rejects_when_contract_not_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    let result = client.try_release_milestone(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, EscrowError::InsufficientFunds);
}

#[test]
fn release_rejects_invalid_milestone_id() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    assert!(client.deposit_funds(&contract_id, &client_addr, &super::total_milestone_amount()));
    let result = client.try_release_milestone(&contract_id, &client_addr, &99);
    super::assert_contract_error(result, EscrowError::InvalidMilestone);
}

#[test]
fn release_rejects_double_release() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    assert!(client.deposit_funds(&contract_id, &client_addr, &super::total_milestone_amount()));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));

    let result = client.try_release_milestone(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, EscrowError::AlreadyReleased);
}

#[test]
fn issue_reputation_rejects_unfinished_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = create_contract(&env, &client);

    let result = client.try_issue_reputation(&contract_id, &client_addr, &5_u32, &soroban_sdk::String::from_str(&env, "Great"));
    super::assert_contract_error(result, EscrowError::NotCompleted);
}

#[test]
fn issue_reputation_rejects_invalid_rating() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = super::complete_contract(&env, &client);

    let result = client.try_issue_reputation(&contract_id, &client_addr, &5_u32, &soroban_sdk::String::from_str(&env, "Great"));
    super::assert_contract_error(result, EscrowError::InvalidRating);
}

#[test]
fn issue_reputation_once_per_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = super::complete_contract(&env, &client);

    assert!(client.issue_reputation(&contract_id, &client_addr, &5_u32, &soroban_sdk::String::from_str(&env, "Great")));
    let result = client.try_issue_reputation(&contract_id, &client_addr, &5_u32, &soroban_sdk::String::from_str(&env, "Great"));
    super::assert_contract_error(result, EscrowError::ReputationAlreadyIssued);
}

#[test]
fn issue_reputation_rejects_freelancer_mismatch() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);
    let wrong_freelancer = soroban_sdk::Address::generate(&env);

    let result = client.try_issue_reputation(&contract_id, &client_addr, &5_u32, &soroban_sdk::String::from_str(&env, "Great"));
    super::assert_contract_error(result, EscrowError::FreelancerMismatch);
}

#[test]
fn issue_reputation_rejects_unauthorized_caller() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, freelancer_addr, contract_id) = super::complete_contract(&env, &client);
    let unauthorized = soroban_sdk::Address::generate(&env);

    let result = client.try_issue_reputation(&contract_id, &unauthorized, &5_u32, &soroban_sdk::String::from_str(&env, "Great"));
    super::assert_contract_error(result, EscrowError::UnauthorizedRole);
}

// ── require_initialized gate tests ──────────────────────────────────────────
//
// Each test registers a fresh contract WITHOUT calling `initialize`, then
// confirms the entrypoint returns `NotInitialized` before touching any other
// state. This validates the uniform safety rail added in
// security/contracts-uniform-init-gate.

/// Returns an uninitialized EscrowClient (no `initialize` call).
fn uninitialized_client(env: &Env) -> EscrowClient<'_> {
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

#[test]
fn cancel_contract_rejects_before_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let client = uninitialized_client(&env);
    let caller = Address::generate(&env);

    let result = client.try_cancel_contract(&1_u32, &caller);
    super::assert_contract_error(result, EscrowError::NotInitialized);
}

#[test]
fn submit_work_evidence_rejects_before_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let client = uninitialized_client(&env);
    let caller = Address::generate(&env);
    let evidence = soroban_sdk::String::from_str(&env, "ipfs://Qm123");

    let result = client.try_submit_work_evidence(&1_u32, &caller, &0_u32, &evidence);
    super::assert_contract_error(result, EscrowError::NotInitialized);
}

#[test]
fn raise_dispute_rejects_before_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let client = uninitialized_client(&env);
    let caller = Address::generate(&env);

    let result = client.try_raise_dispute(&1_u32, &caller);
    super::assert_contract_error(result, EscrowError::NotInitialized);
}

#[test]
fn resolve_dispute_rejects_before_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let client = uninitialized_client(&env);
    let arbiter = Address::generate(&env);

    let result = client.try_resolve_dispute(&1_u32, &arbiter, &DisputeResolution::FullRefund);
    super::assert_contract_error(result, EscrowError::NotInitialized);
}
