use super::{create_contract, default_milestones, generated_participants, register_client, total_milestone_amount};
use crate::{EscrowError, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, vec, Env, Vec};

// ─── CEI ordering regression (#401) ──────────────────────────────────────────
//
// Verify that `release_milestone` and `refund_unreleased_milestones` commit all
// state mutations (milestone.released / milestone.refunded, contract accounting)
// BEFORE returning, so that any subsequent read from an external caller (or a
// future SAC transfer hook) observes final, consistent state.
//
// These tests act as a proxy for the full reentrancy guard: since Soroban's
// single-host model prevents cross-contract reentrancy on the same ledger, the
// critical invariant is that state is committed atomically before the function
// returns — a second call on the same milestone must be rejected with an error.

#[test]
fn release_milestone_state_committed_before_return() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);
    client.deposit_funds(&contract_id, &client_addr, &total_milestone_amount());
    client.approve_milestone_release(&contract_id, &client_addr, &0);
    assert!(client.release_milestone(&contract_id, &client_addr, &0));

    // State must be committed: a second release must be rejected with AlreadyReleased.
    super::assert_contract_error(
        client.try_release_milestone(&contract_id, &client_addr, &0),
        EscrowError::AlreadyReleased,
    );

    // The milestone vector reflects released = true
    let milestones = client.get_milestones(&contract_id);
    assert!(milestones.get(0).unwrap().released, "milestone.released must be true after release");
}

#[test]
fn refund_milestone_state_committed_before_return() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);
    client.deposit_funds(&contract_id, &client_addr, &total_milestone_amount());

    let indices = vec![&env, 0u32];
    assert!(client.refund_unreleased_milestones(&contract_id, &indices) > 0);

    // State committed: second refund of the same milestone must fail.
    super::assert_contract_error(
        client.try_refund_unreleased_milestones(&contract_id, &indices),
        EscrowError::AlreadyRefunded,
    );

    let milestones = client.get_milestones(&contract_id);
    assert!(milestones.get(0).unwrap().refunded, "milestone.refunded must be true after refund");
}

#[test]
fn cancel_contract_state_committed_before_return() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    assert!(client.cancel_contract(&contract_id, &client_addr));

    // State committed: second cancel must fail with InvalidState.
    super::assert_contract_error(
        client.try_cancel_contract(&contract_id, &client_addr),
        EscrowError::InvalidState,
    );

    use crate::ContractStatus;
    assert_eq!(client.get_contract(&contract_id).status, ContractStatus::Cancelled);
}

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

    let result = client.try_issue_reputation(&contract_id, &client_addr, &freelancer_addr, &5);
    super::assert_contract_error(result, EscrowError::NotCompleted);
}

#[test]
fn issue_reputation_rejects_invalid_rating() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = super::complete_contract(&env, &client);

    let result = client.try_issue_reputation(&contract_id, &client_addr, &freelancer_addr, &0);
    super::assert_contract_error(result, EscrowError::InvalidRating);
}

#[test]
fn issue_reputation_once_per_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = super::complete_contract(&env, &client);

    assert!(client.issue_reputation(&contract_id, &client_addr, &freelancer_addr, &5));
    let result = client.try_issue_reputation(&contract_id, &client_addr, &freelancer_addr, &4);
    super::assert_contract_error(result, EscrowError::ReputationAlreadyIssued);
}

#[test]
fn issue_reputation_rejects_freelancer_mismatch() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);
    let wrong_freelancer = soroban_sdk::Address::generate(&env);

    let result = client.try_issue_reputation(&contract_id, &client_addr, &wrong_freelancer, &5);
    super::assert_contract_error(result, EscrowError::FreelancerMismatch);
}

#[test]
fn issue_reputation_rejects_unauthorized_caller() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, freelancer_addr, contract_id) = super::complete_contract(&env, &client);
    let unauthorized = soroban_sdk::Address::generate(&env);

    let result = client.try_issue_reputation(&contract_id, &unauthorized, &freelancer_addr, &5);
    super::assert_contract_error(result, EscrowError::UnauthorizedRole);
}
