use super::{create_contract, default_milestones, generated_participants, register_client};
use crate::types::DepositMode;
use soroban_sdk::Env;

#[test]
fn average_rating_returns_none_when_no_completed_contracts() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (_client_addr, freelancer_addr) = generated_participants(&env);
    assert_eq!(client.get_average_rating(&freelancer_addr), None);
}

#[test]
fn average_rating_returns_scaled_value_for_completed_contracts() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, freelancer_addr) = generated_participants(&env);
    let milestones = default_milestones(&env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &milestones,
        &DepositMode::ExactTotal,
    );
    assert!(client.deposit_funds(&contract_id, &super::total_milestone_amount()));
    assert!(client.release_milestone(&contract_id, &0));
    assert!(client.release_milestone(&contract_id, &1));
    assert!(client.release_milestone(&contract_id, &2));
    assert!(client.issue_reputation(&contract_id, &client_addr, &freelancer_addr, &5));

    let average = client.get_average_rating(&freelancer_addr);
    assert_eq!(average, Some(500));
}

#[test]
fn average_rating_aggregates_across_multiple_completed_contracts() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, freelancer_addr) = generated_participants(&env);
    let milestones = default_milestones(&env);

    let first_contract = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &milestones,
        &DepositMode::ExactTotal,
    );
    assert!(client.deposit_funds(&first_contract, &super::total_milestone_amount()));
    assert!(client.release_milestone(&first_contract, &0));
    assert!(client.release_milestone(&first_contract, &1));
    assert!(client.release_milestone(&first_contract, &2));
    assert!(client.issue_reputation(&first_contract, &client_addr, &freelancer_addr, &5));

    let second_contract = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &milestones,
        &DepositMode::ExactTotal,
    );
    assert!(client.deposit_funds(&second_contract, &super::total_milestone_amount()));
    assert!(client.release_milestone(&second_contract, &0));
    assert!(client.release_milestone(&second_contract, &1));
    assert!(client.release_milestone(&second_contract, &2));
    assert!(client.issue_reputation(&second_contract, &client_addr, &freelancer_addr, &4));

    let reputation = client.get_reputation(&freelancer_addr).unwrap();
    assert_eq!(reputation.completed_contracts, 2);
    assert_eq!(reputation.total_rating, 9);
    assert_eq!(client.get_average_rating(&freelancer_addr), Some(450));
}
