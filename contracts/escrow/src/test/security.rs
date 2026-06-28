use super::{create_contract, default_milestones, generated_participants, register_client, total_milestone_amount};
use crate::ttl::ADMIN_ROTATION_MIN_DELAY_LEDGERS;
use crate::{Error, Escrow, EscrowClient, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, vec, Address, Env, Vec};

#[test]
fn create_rejects_same_participants() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (addr, _) = generated_participants(&env);

    let result =
        client.try_create_contract(&addr, &addr, &None, &default_milestones(&env), &ReleaseAuthorization::ClientOnly);
    super::assert_contract_error(result, Error::InvalidParticipant);
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
    super::assert_contract_error(result, Error::EmptyMilestones);
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
    super::assert_contract_error(result, Error::InvalidMilestoneAmount);
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
    super::assert_contract_error(result, Error::InvalidDepositAmount);
}

#[test]
fn release_rejects_when_contract_not_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    let result = client.try_release_milestone(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, Error::InsufficientFunds);
}

#[test]
fn release_rejects_invalid_milestone_id() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    assert!(client.deposit_funds(&contract_id, &client_addr, &super::total_milestone_amount()));
    let result = client.try_release_milestone(&contract_id, &client_addr, &99);
    super::assert_contract_error(result, Error::InvalidMilestone);
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
    super::assert_contract_error(result, Error::AlreadyReleased);
}

#[test]
fn issue_reputation_rejects_unfinished_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);
    let comment = soroban_sdk::String::from_str(&env, "Good job");

    let result = client.try_issue_reputation(&contract_id, &client_addr, &5_u32, &soroban_sdk::String::from_str(&env, "Great"));
    super::assert_contract_error(result, Error::NotCompleted);
}

#[test]
fn issue_reputation_rejects_invalid_rating() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);
    let comment = soroban_sdk::String::from_str(&env, "Good job");

    let result = client.try_issue_reputation(&contract_id, &client_addr, &5_u32, &soroban_sdk::String::from_str(&env, "Great"));
    super::assert_contract_error(result, Error::InvalidRating);
}

#[test]
fn issue_reputation_once_per_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = super::complete_contract(&env, &client);

    assert!(client.issue_reputation(&contract_id, &client_addr, &5_u32, &soroban_sdk::String::from_str(&env, "Great")));
    let result = client.try_issue_reputation(&contract_id, &client_addr, &5_u32, &soroban_sdk::String::from_str(&env, "Great"));
    super::assert_contract_error(result, Error::ReputationAlreadyIssued);
}

#[test]
fn issue_reputation_rejects_freelancer_mismatch() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);
    let comment = soroban_sdk::String::from_str(&env, "Good job");

    let result = client.try_issue_reputation(&contract_id, &client_addr, &5_u32, &soroban_sdk::String::from_str(&env, "Great"));
    super::assert_contract_error(result, Error::FreelancerMismatch);
}

#[test]
fn issue_reputation_rejects_unauthorized_caller() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);
    let unauthorized = soroban_sdk::Address::generate(&env);
    let comment = soroban_sdk::String::from_str(&env, "Good job");

    let result = client.try_issue_reputation(&contract_id, &unauthorized, &5_u32, &soroban_sdk::String::from_str(&env, "Great"));
    super::assert_contract_error(result, Error::UnauthorizedRole);
}

#[test]
fn test_error_code_stability() {
    assert_eq!(Error::IndexOutOfBounds as u32, 3);
    assert_eq!(Error::AlreadyReleased as u32, 4);
    assert_eq!(Error::EmptyRefundRequest as u32, 6);
    assert_eq!(Error::DuplicateMilestoneInRefund as u32, 7);
    assert_eq!(Error::AlreadyRefunded as u32, 8);
    assert_eq!(Error::InsufficientFunds as u32, 9);
    assert_eq!(Error::ContractNotFound as u32, 10);
    assert_eq!(Error::UnauthorizedRole as u32, 11);
    assert_eq!(Error::InvalidParticipants as u32, 14);
    assert_eq!(Error::AmountMustBePositive as u32, 15);
    assert_eq!(Error::InvalidState as u32, 16);
    assert_eq!(Error::EmptyMilestones as u32, 25);
    assert_eq!(Error::InvalidMilestoneAmount as u32, 26);
    assert_eq!(Error::CommentTooLong as u32, 30);
    assert_eq!(Error::InvalidParticipant as u32, 31);
    assert_eq!(Error::InvalidDepositAmount as u32, 32);
    assert_eq!(Error::AlreadyInitialized as u32, 34);
    assert_eq!(Error::NotInitialized as u32, 36);
    assert_eq!(Error::ContractPaused as u32, 37);
    assert_eq!(Error::EmergencyActive as u32, 38);
    assert_eq!(Error::InvalidStatusTransition as u32, 41);
    assert_eq!(Error::AccountingInvariantViolated as u32, 44);
    assert_eq!(Error::AlreadyFinalized as u32, 46);
    assert_eq!(Error::EvidenceTooLong as u32, 47);
    assert_eq!(Error::TimelockNotElapsed as u32, 48);
    assert_eq!(Error::InvalidProtocolParameters as u32, 49);
}

// ── cancel_governance_admin_proposal ──────────────────────────────────────────
//
// Security coverage for aborting a pending two-step admin transfer. A
// cancellation must clear the pending proposal, block a later accept by the
// previously proposed address, be gated on the current admin's authorization,
// and reject when there is nothing to cancel or the contract is uninitialized.

/// Register the escrow and initialize it with a freshly generated admin,
/// returning the client and the admin address. Auths must already be mocked.
fn init_with_admin(env: &Env) -> (EscrowClient<'_>, Address) {
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &id);
    let admin = Address::generate(env);
    assert!(client.initialize(&admin), "initialize must succeed");
    (client, admin)
}

#[test]
fn cancel_clears_pending_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = init_with_admin(&env);
    let proposed = Address::generate(&env);

    assert!(client.propose_governance_admin(&proposed));
    assert_eq!(client.get_pending_governance_admin(), Some(proposed));

    assert!(client.cancel_governance_admin_proposal());
    assert_eq!(client.get_pending_governance_admin(), None);
}

#[test]
fn cancel_blocks_later_accept() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = init_with_admin(&env);
    let proposed = Address::generate(&env);

    assert!(client.propose_governance_admin(&proposed));
    assert!(client.cancel_governance_admin_proposal());

    // Even after the timelock elapses, the cancelled proposal cannot be
    // accepted — the previously proposed admin can no longer seize control.
    let proposed_at = env.ledger().sequence();
    env.ledger()
        .set_sequence(proposed_at + ADMIN_ROTATION_MIN_DELAY_LEDGERS + 1);

    let result = client.try_accept_governance_admin();
    super::assert_contract_error(result, Error::InvalidState);

    // The admin is unchanged.
    assert_eq!(client.get_pending_governance_admin(), None);
}

#[test]
fn cancel_without_proposal_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = init_with_admin(&env);

    let result = client.try_cancel_governance_admin_proposal();
    super::assert_contract_error(result, Error::InvalidState);
}

#[test]
fn cancel_before_initialize_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &id);

    let result = client.try_cancel_governance_admin_proposal();
    super::assert_contract_error(result, Error::NotInitialized);
}

#[test]
fn cancel_then_repropose_and_accept_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = init_with_admin(&env);
    let first = Address::generate(&env);
    let second = Address::generate(&env);

    assert!(client.propose_governance_admin(&first));
    assert!(client.cancel_governance_admin_proposal());

    // A fresh proposal still works after a cancellation and resets the timelock
    // anchor to the new proposal's ledger.
    assert!(client.propose_governance_admin(&second));
    let proposed_at = env.ledger().sequence();
    env.ledger()
        .set_sequence(proposed_at + ADMIN_ROTATION_MIN_DELAY_LEDGERS);

    assert!(client.accept_governance_admin());
    assert_eq!(client.get_governance_admin(), Some(second));
    assert_eq!(client.get_pending_governance_admin(), None);
}

/// Only the current admin may cancel: with auth granted to a non-admin address,
/// `admin.require_auth()` fails and the host aborts the invocation.
#[test]
#[should_panic]
fn cancel_rejects_non_admin_auth() {
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    use soroban_sdk::IntoVal;

    let env = Env::default();
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &id);
    let admin = Address::generate(&env);
    let proposed = Address::generate(&env);
    let attacker = Address::generate(&env);

    // Authorize the admin for the setup calls.
    env.mock_auths(&[
        MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &id,
                fn_name: "initialize",
                args: (admin.clone(),).into_val(&env),
                sub_invokes: &[],
            },
        },
        MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &id,
                fn_name: "propose_governance_admin",
                args: (proposed.clone(),).into_val(&env),
                sub_invokes: &[],
            },
        },
    ]);
    client.initialize(&admin);
    client.propose_governance_admin(&proposed);

    // Now grant auth only to a non-admin; cancel must abort on the admin's
    // require_auth.
    env.mock_auths(&[MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &id,
            fn_name: "cancel_governance_admin_proposal",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.cancel_governance_admin_proposal();
}

#[test]
fn cancel_emits_cancelled_event() {
    use soroban_sdk::testutils::Events;
    use soroban_sdk::{Symbol, TryFromVal};

    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = init_with_admin(&env);
    let proposed = Address::generate(&env);

    client.propose_governance_admin(&proposed);
    client.cancel_governance_admin_proposal();

    let admin_topic = soroban_sdk::symbol_short!("admin");
    let cancelled_topic = Symbol::new(&env, "cancelled");
    let found = env.events().all().iter().any(|event| {
        event.1.len() >= 2
            && Symbol::try_from_val(&env, &event.1.get(0).unwrap())
                .ok()
                .as_ref()
                == Some(&admin_topic)
            && Symbol::try_from_val(&env, &event.1.get(1).unwrap())
                .ok()
                .as_ref()
                == Some(&cancelled_topic)
    });
    assert!(found, "cancel must emit an (admin, cancelled) event");
}

