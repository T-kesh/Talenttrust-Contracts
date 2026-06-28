#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env};
use crate::{Escrow, EscrowClient, ReleaseAuthorization};

#[test]
#[should_panic(expected = "Only client can deposit funds")]
fn test_unauthorized_deposit() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let malicious_actor = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    env.mock_all_auths();
    // Malicious actor tries to deposit
    client.deposit_funds(&1, &malicious_actor, &1000_0000000);
}

#[test]
#[should_panic(expected = "Contract must be in Created status to deposit funds")]
fn test_deposit_wrong_status_funded() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &1000_0000000);
    // Double funding attempt
    client.deposit_funds(&1, &client_addr, &1000_0000000);
}

#[test]
#[should_panic(expected = "Contract must be in Funded status to approve milestones")]
fn test_approve_wrong_status_created() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    env.mock_all_auths();
    // Try to approve before funding
    client.approve_milestone_release(&1, &client_addr, &0);
}

#[test]
#[should_panic(expected = "Contract must be in Funded status to release milestones")]
fn test_release_wrong_status_created() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    env.mock_all_auths();
    // Try to release before funding
    client.release_milestone(&1, &client_addr, &0);
}

#[test]
#[should_panic(expected = "Caller not authorized to approve milestone release")]
fn test_approve_unauthorized_arbiter_in_clientonly() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &1000_0000000);
    // Arbiter tries to approve in a ClientOnly auth scheme
    client.approve_milestone_release(&1, &arbiter_addr, &0);
}
#[test]
#[should_panic(expected = "ContractPaused")]
fn test_finalize_paused_rejects() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    // finalize contract should fail while paused
    client.pause();
    super::assert_contract_error(
        client.try_finalize_contract(&contract_id, &client_addr),
        Error::ContractPaused,
    );
}

#[test]
#[should_panic(expected = "UnauthorizedRole")]
fn test_finalize_unauthorized_finalizer() {
    let (env, client) = setup();
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let outsider = Address::generate(&env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    // deposit and release to reach Completed
    env.mock_all_auths();
    client.deposit_funds(&contract_id, &client_addr, &100_i128);
    client.release_milestone(&contract_id, &client_addr, &0);
    super::assert_contract_error(
        client.try_finalize_contract(&contract_id, &outsider),
        Error::UnauthorizedRole,
    );
}

#[test]
#[should_panic(expected = "InvalidStatusTransition")]
fn test_finalize_invalid_status() {
    let (env, client) = setup();
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    // contract is still Created
    super::assert_contract_error(
        client.try_finalize_contract(&contract_id, &client_addr),
        Error::InvalidStatusTransition,
    );
}

#[test]
#[should_panic(expected = "AlreadyFinalized")]
fn test_finalize_already_finalized() {
    let (env, client) = setup();
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    env.mock_all_auths();
    client.deposit_funds(&contract_id, &client_addr, &100_i128);
    client.release_milestone(&contract_id, &client_addr, &0);
    // first finalize succeeds
    assert!(client.finalize_contract(&contract_id, &client_addr));
    // second finalize should error
    super::assert_contract_error(
        client.try_finalize_contract(&contract_id, &client_addr),
        Error::AlreadyFinalized,
    );
}
