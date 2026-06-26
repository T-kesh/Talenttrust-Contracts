use super::{create_contract, default_milestones, generated_participants, register_client, total_milestone_amount};
use crate::{EscrowError, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, vec, Env, Vec};

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
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);
    let comment = soroban_sdk::String::from_str(&env, "Good job");

    let result = client.try_issue_reputation(&contract_id, &client_addr, &5, &comment);
    super::assert_contract_error(result, EscrowError::NotCompleted);
}

#[test]
fn issue_reputation_rejects_invalid_rating() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);
    let comment = soroban_sdk::String::from_str(&env, "Good job");

    let result = client.try_issue_reputation(&contract_id, &client_addr, &0, &comment);
    super::assert_contract_error(result, EscrowError::InvalidRating);
}

#[test]
fn issue_reputation_once_per_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);
    let comment = soroban_sdk::String::from_str(&env, "Good job");

    assert!(client.issue_reputation(&contract_id, &client_addr, &5, &comment));
    let result = client.try_issue_reputation(&contract_id, &client_addr, &4, &comment);
    super::assert_contract_error(result, EscrowError::ReputationAlreadyIssued);
}

#[test]
fn issue_reputation_rejects_unauthorized_caller() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);
    let unauthorized = soroban_sdk::Address::generate(&env);
    let comment = soroban_sdk::String::from_str(&env, "Good job");

    let result = client.try_issue_reputation(&contract_id, &unauthorized, &5, &comment);
    super::assert_contract_error(result, EscrowError::UnauthorizedRole);
}

#[test]
fn issue_reputation_rejects_empty_comment() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);
    let comment = soroban_sdk::String::from_str(&env, "");

    let result = client.try_issue_reputation(&contract_id, &client_addr, &5, &comment);
    super::assert_contract_error(result, EscrowError::EmptyComment);
}

#[test]
fn issue_reputation_rejects_comment_too_long() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);
    
    let long_str = "a".repeat(201);
    let comment = soroban_sdk::String::from_str(&env, &long_str);

    let result = client.try_issue_reputation(&contract_id, &client_addr, &5, &comment);
    super::assert_contract_error(result, EscrowError::CommentTooLong);
}

#[test]
fn submit_work_evidence_rejects_evidence_too_long() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = create_contract(&env, &client);
    
    client.deposit_funds(&contract_id, &client_addr, &super::total_milestone_amount());

    let long_str = "a".repeat(257);
    let evidence = soroban_sdk::String::from_str(&env, &long_str);

    let result = client.try_submit_work_evidence(&contract_id, &freelancer_addr, &0, &evidence);
    super::assert_contract_error(result, EscrowError::EvidenceTooLong);
}

#[test]
fn governance_admin_rejects_timelock_not_elapsed() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let admin = soroban_sdk::Address::generate(&env);
    client.initialize(&admin);
    
    let proposed = soroban_sdk::Address::generate(&env);
    client.propose_governance_admin(&proposed);

    let result = client.try_accept_governance_admin();
    super::assert_contract_error(result, EscrowError::TimelockNotElapsed);
}

#[test]
fn governance_rejects_invalid_protocol_parameters() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let admin = soroban_sdk::Address::generate(&env);
    client.initialize(&admin);

    let result = client.try_set_protocol_fee_bps(&10001);
    super::assert_contract_error(result, EscrowError::InvalidProtocolParameters);
}