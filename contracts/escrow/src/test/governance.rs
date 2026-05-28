use super::register_client;
use crate::{EscrowError, ProtocolParameters};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn protocol_parameters_default_before_governance_is_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let parameters = client.get_protocol_parameters();

    assert_eq!(
        parameters,
        ProtocolParameters {
            min_milestone_amount: 1,
            max_milestones: 16,
            min_reputation_rating: 1,
            max_reputation_rating: 5,
        }
    );
    assert_eq!(client.get_governance_admin(), None);
    assert_eq!(client.get_pending_governance_admin(), None);
}

#[test]
fn governance_initialization_and_updates_change_live_validation_rules() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    assert!(client.initialize_governance(&admin));

    assert_eq!(client.get_governance_admin(), Some(admin));
    assert_eq!(client.get_pending_governance_admin(), None);
}

#[test]
#[should_panic(expected = "protocol governance is already initialized")]
fn initialize_governance_twice_panics() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize_governance(&admin);

    let admin2 = Address::generate(&env);
    client.initialize_governance(&admin2);
}

#[test]
fn update_protocol_parameters_changes_validation_rules() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize_governance(&admin);

    assert!(client.update_protocol_parameters(&100_i128, &10_u32, &1_i128, &10_i128));

    let parameters = client.get_protocol_parameters();
    assert_eq!(parameters.min_milestone_amount, 100);
    assert_eq!(parameters.max_milestones, 10);
    assert_eq!(parameters.min_reputation_rating, 1);
    assert_eq!(parameters.max_reputation_rating, 10);
}

#[test]
#[should_panic(expected = "protocol governance is not initialized")]
fn update_protocol_parameters_without_initialization_panics() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    client.update_protocol_parameters(&100_i128, &10_u32, &1_i128, &10_i128);
}

#[test]
#[should_panic(expected = "invalid protocol parameters")]
fn update_protocol_parameters_with_zero_min_milestone_panics() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize_governance(&admin);

    client.update_protocol_parameters(&0_i128, &4_u32, &1_i128, &5_i128);
}

#[test]
fn governance_admin_transfer_is_two_step() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    let next_admin = Address::generate(&env);
    assert!(client.initialize_protocol_governance(&admin, &10_i128, &4_u32, &1_i128, &5_i128));

    assert!(client.propose_governance_admin(&next_admin));
    assert_eq!(client.get_pending_governance_admin(), Some(next_admin.clone()));

    assert!(client.accept_governance_admin());
    assert_eq!(client.get_governance_admin(), Some(next_admin));
    assert_eq!(client.get_pending_governance_admin(), None);
}

#[test]
fn governance_propose_overwrites_pending_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    let first_pending = Address::generate(&env);
    let second_pending = Address::generate(&env);
    assert!(client.initialize_protocol_governance(&admin, &10_i128, &4_u32, &1_i128, &5_i128));

    assert!(client.propose_governance_admin(&first_pending));
    assert_eq!(client.get_pending_governance_admin(), Some(first_pending.clone()));

    assert!(client.propose_governance_admin(&second_pending));
    assert_eq!(client.get_pending_governance_admin(), Some(second_pending.clone()));
}

#[test]
fn governance_rejects_invalid_parameter_updates() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    assert!(client.initialize_protocol_governance(&admin, &10_i128, &4_u32, &1_i128, &5_i128));

    let result = client.try_update_protocol_parameters(&0_i128, &4_u32, &1_i128, &5_i128);
    super::assert_contract_error(result, EscrowError::InvalidProtocolParameters);
}

#[test]
fn governance_requires_initialization_for_mutations() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let next_admin = Address::generate(&env);
    super::assert_contract_error(
        client.try_propose_governance_admin(&next_admin),
        EscrowError::GovernanceNotInitialized,
    );
    super::assert_contract_error(
        client.try_accept_governance_admin(),
        EscrowError::InvalidState,
    );
}
