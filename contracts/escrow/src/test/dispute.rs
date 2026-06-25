use crate::dispute::{final_status_after_resolution, resolution_payouts};
use crate::{
    Contract, ContractStatus, DepositMode, DisputeResolution, Escrow, EscrowClient, EscrowError,
    ReleaseAuthorization,
};
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

fn setup_initialized() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));
    (env, contract_id)
}

fn create_client<'a>(env: &'a Env, contract_id: &Address) -> EscrowClient<'a> {
    EscrowClient::new(env, contract_id)
}

fn payout_contract(
    env: &Env,
    funded_amount: i128,
    released_amount: i128,
    refunded_amount: i128,
) -> Contract {
    Contract {
        client: Address::generate(env),
        freelancer: Address::generate(env),
        arbiter: Some(Address::generate(env)),
        status: ContractStatus::Disputed,
        funded_amount,
        released_amount,
        refunded_amount,
        release_authorization: ReleaseAuthorization::ClientAndArbiter,
    }
}

fn funded_contract_with_arbiter(
    env: &Env,
    client: &EscrowClient<'_>,
    milestones: soroban_sdk::Vec<i128>,
    deposit_amount: i128,
    deposit_mode: DepositMode,
) -> (Address, Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let arbiter_addr = Address::generate(env);
    let contract_id = client.create_contract_with_arbiter(
        &client_addr,
        &freelancer_addr,
        &arbiter_addr,
        &milestones,
        &deposit_mode,
    );
    assert!(client.deposit_funds(&contract_id, &deposit_amount));
    (client_addr, freelancer_addr, arbiter_addr, contract_id)
}

/// Verifies FullRefund conserves all available balance for the client.
#[test]
fn resolution_payouts_full_refund_returns_available_to_client() {
    let env = Env::default();
    let contract = payout_contract(&env, 100, 20, 10);

    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::FullRefund),
        Ok((70, 0))
    );
}

/// Verifies FullPayout conserves all available balance for the freelancer.
#[test]
fn resolution_payouts_full_payout_returns_available_to_freelancer() {
    let env = Env::default();
    let contract = payout_contract(&env, 100, 20, 10);

    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::FullPayout),
        Ok((0, 70))
    );
}

/// Verifies PartialRefund applies the documented 70/30 split with floor rounding.
#[test]
fn resolution_payouts_partial_refund_uses_floor_rounded_70_30_split() {
    let env = Env::default();
    let contract = payout_contract(&env, 101, 0, 0);

    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::PartialRefund),
        Ok((71, 30))
    );
}

/// Verifies PartialRefund handles zero and one-stroop balances without creating value.
#[test]
fn resolution_payouts_partial_refund_handles_rounding_boundaries() {
    let env = Env::default();
    let zero_available = payout_contract(&env, 0, 0, 0);
    let one_stroop_available = payout_contract(&env, 1, 0, 0);

    assert_eq!(
        resolution_payouts(&zero_available, &DisputeResolution::PartialRefund),
        Ok((0, 0))
    );
    assert_eq!(
        resolution_payouts(&one_stroop_available, &DisputeResolution::PartialRefund),
        Ok((1, 0))
    );
}

/// Verifies Split rejects negative client or freelancer payouts.
#[test]
fn resolution_payouts_split_rejects_negative_legs() {
    let env = Env::default();
    let contract = payout_contract(&env, 100, 0, 0);

    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(-1, 101)),
        Err(EscrowError::InvalidDisputeSplit)
    );
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(101, -1)),
        Err(EscrowError::InvalidDisputeSplit)
    );
}

/// Verifies Split rejects under-sized and oversized sums that do not equal available balance.
#[test]
fn resolution_payouts_split_rejects_non_conserving_sums() {
    let env = Env::default();
    let contract = payout_contract(&env, 100, 0, 0);

    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(40, 59)),
        Err(EscrowError::InvalidDisputeSplit)
    );
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(40, 61)),
        Err(EscrowError::InvalidDisputeSplit)
    );
}

/// Verifies Split accepts exact conservation, including zero available balance.
#[test]
fn resolution_payouts_split_accepts_exact_splits() {
    let env = Env::default();
    let contract = payout_contract(&env, 100, 0, 0);
    let zero_available = payout_contract(&env, 0, 0, 0);

    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(40, 60)),
        Ok((40, 60))
    );
    assert_eq!(
        resolution_payouts(&zero_available, &DisputeResolution::Split(0, 0)),
        Ok((0, 0))
    );
}

/// Verifies Split uses checked addition and rejects overflowing payout sums.
#[test]
fn resolution_payouts_split_rejects_overflowing_sum() {
    let env = Env::default();
    let contract = payout_contract(&env, i128::MAX, 0, 0);

    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(i128::MAX, 1)),
        Err(EscrowError::PotentialOverflow)
    );
}

/// Verifies payout math fails closed when released and refunded amounts exceed deposits.
#[test]
fn resolution_payouts_rejects_accounting_invariant_violation() {
    let env = Env::default();
    let contract = payout_contract(&env, 100, 70, 31);

    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::FullRefund),
        Err(EscrowError::AccountingInvariantViolated)
    );
}

/// Verifies final status is Refunded only when the full deposit has been refunded.
#[test]
fn final_status_after_resolution_marks_refunded_only_for_full_refund() {
    let env = Env::default();
    let fully_refunded = payout_contract(&env, 100, 0, 100);
    let partially_refunded = payout_contract(&env, 100, 30, 70);

    assert_eq!(
        final_status_after_resolution(&fully_refunded),
        ContractStatus::Refunded
    );
    assert_eq!(
        final_status_after_resolution(&partially_refunded),
        ContractStatus::Completed
    );
}

#[test]
fn client_can_raise_dispute_on_funded_contract() {
    let (env, contract_id) = setup_initialized();
    let client = create_client(&env, &contract_id);
    let (client_addr, _, _, escrow_id) = funded_contract_with_arbiter(
        &env,
        &client,
        vec![&env, 100_i128, 200_i128],
        300_i128,
        DepositMode::ExactTotal,
    );

    assert!(client.raise_dispute(&escrow_id, &client_addr));
    assert_eq!(
        client.get_contract(&escrow_id).status,
        ContractStatus::Disputed
    );
}

#[test]
fn freelancer_can_raise_dispute_on_partially_funded_contract() {
    let (env, contract_id) = setup_initialized();
    let client = create_client(&env, &contract_id);
    let (_, freelancer_addr, _, escrow_id) = funded_contract_with_arbiter(
        &env,
        &client,
        vec![&env, 100_i128, 200_i128],
        150_i128,
        DepositMode::Incremental,
    );

    assert!(client.raise_dispute(&escrow_id, &freelancer_addr));
    assert_eq!(
        client.get_contract(&escrow_id).status,
        ContractStatus::Disputed
    );
}

#[test]
fn raise_dispute_requires_contract_party() {
    let (env, contract_id) = setup_initialized();
    let client = create_client(&env, &contract_id);
    let (_, _, _, escrow_id) = funded_contract_with_arbiter(
        &env,
        &client,
        vec![&env, 100_i128],
        100_i128,
        DepositMode::ExactTotal,
    );

    let outsider = Address::generate(&env);
    super::assert_contract_error(
        client.try_raise_dispute(&escrow_id, &outsider),
        EscrowError::UnauthorizedRole,
    );
}

#[test]
fn raise_dispute_requires_assigned_arbiter() {
    let (env, contract_id) = setup_initialized();
    let client = create_client(&env, &contract_id);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let escrow_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &vec![&env, 100_i128],
        &DepositMode::ExactTotal,
    );
    assert!(client.deposit_funds(&escrow_id, &100_i128));

    super::assert_contract_error(
        client.try_raise_dispute(&escrow_id, &client_addr),
        EscrowError::ArbiterRequired,
    );
}

#[test]
fn resolve_full_refund_marks_refunded_and_closes_accounting() {
    let (env, contract_id) = setup_initialized();
    let client = create_client(&env, &contract_id);
    let (client_addr, _, arbiter_addr, escrow_id) = funded_contract_with_arbiter(
        &env,
        &client,
        vec![&env, 125_i128, 75_i128],
        200_i128,
        DepositMode::ExactTotal,
    );
    assert!(client.raise_dispute(&escrow_id, &client_addr));

    assert!(client.resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::FullRefund,));

    let contract = client.get_contract(&escrow_id);
    assert_eq!(contract.status, ContractStatus::Refunded);
    assert_eq!(contract.released_amount, 0);
    assert_eq!(contract.refunded_amount, 200);
    assert_eq!(
        contract.released_amount + contract.refunded_amount,
        contract.total_deposited
    );
}

#[test]
fn resolve_partial_refund_applies_70_30_to_remaining_balance() {
    let (env, contract_id) = setup_initialized();
    let client = create_client(&env, &contract_id);
    let (client_addr, _, arbiter_addr, escrow_id) = funded_contract_with_arbiter(
        &env,
        &client,
        vec![&env, 101_i128, 100_i128],
        201_i128,
        DepositMode::ExactTotal,
    );
    assert!(client.release_milestone(&escrow_id, &0));
    assert!(client.raise_dispute(&escrow_id, &client_addr));

    assert!(client.resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::PartialRefund,));

    let contract = client.get_contract(&escrow_id);
    assert_eq!(contract.status, ContractStatus::Completed);
    assert_eq!(contract.released_amount, 131);
    assert_eq!(contract.refunded_amount, 70);
    assert_eq!(
        contract.released_amount + contract.refunded_amount,
        contract.total_deposited
    );
}

#[test]
fn resolve_split_accepts_custom_amounts_that_match_available_balance() {
    let (env, contract_id) = setup_initialized();
    let client = create_client(&env, &contract_id);
    let (client_addr, _, arbiter_addr, escrow_id) = funded_contract_with_arbiter(
        &env,
        &client,
        vec![&env, 40_i128, 60_i128],
        100_i128,
        DepositMode::ExactTotal,
    );
    assert!(client.raise_dispute(&escrow_id, &client_addr));

    assert!(client.resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::Split(35, 65),));

    let contract = client.get_contract(&escrow_id);
    assert_eq!(contract.status, ContractStatus::Completed);
    assert_eq!(contract.refunded_amount, 35);
    assert_eq!(contract.released_amount, 65);
}

#[test]
fn resolve_split_rejects_invalid_totals() {
    let (env, contract_id) = setup_initialized();
    let client = create_client(&env, &contract_id);
    let (client_addr, _, arbiter_addr, escrow_id) = funded_contract_with_arbiter(
        &env,
        &client,
        vec![&env, 100_i128],
        100_i128,
        DepositMode::ExactTotal,
    );
    assert!(client.raise_dispute(&escrow_id, &client_addr));

    super::assert_contract_error(
        client.try_resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::Split(30, 50)),
        EscrowError::InvalidDisputeSplit,
    );
}

#[test]
fn resolve_dispute_requires_assigned_arbiter() {
    let (env, contract_id) = setup_initialized();
    let client = create_client(&env, &contract_id);
    let (client_addr, _, _, escrow_id) = funded_contract_with_arbiter(
        &env,
        &client,
        vec![&env, 100_i128],
        100_i128,
        DepositMode::ExactTotal,
    );
    assert!(client.raise_dispute(&escrow_id, &client_addr));

    let outsider = Address::generate(&env);
    super::assert_contract_error(
        client.try_resolve_dispute(&escrow_id, &outsider, &DisputeResolution::FullPayout),
        EscrowError::UnauthorizedRole,
    );
}

#[test]
fn resolve_dispute_rejects_non_disputed_contract() {
    let (env, contract_id) = setup_initialized();
    let client = create_client(&env, &contract_id);
    let (_, _, arbiter_addr, escrow_id) = funded_contract_with_arbiter(
        &env,
        &client,
        vec![&env, 100_i128],
        100_i128,
        DepositMode::ExactTotal,
    );

    super::assert_contract_error(
        client.try_resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::FullRefund),
        EscrowError::InvalidStatusTransition,
    );
}

#[test]
fn release_is_blocked_while_disputed() {
    let (env, contract_id) = setup_initialized();
    let client = create_client(&env, &contract_id);
    let (client_addr, _, _, escrow_id) = funded_contract_with_arbiter(
        &env,
        &client,
        vec![&env, 100_i128, 50_i128],
        150_i128,
        DepositMode::ExactTotal,
    );
    assert!(client.raise_dispute(&escrow_id, &client_addr));

    super::assert_contract_error(
        client.try_release_milestone(&escrow_id, &0),
        EscrowError::InvalidStatusTransition,
    );
}

#[test]
fn pause_blocks_raise_and_resolve_dispute() {
    let (env, contract_id) = setup_initialized();
    let client = create_client(&env, &contract_id);
    let (client_addr, _, arbiter_addr, escrow_id) = funded_contract_with_arbiter(
        &env,
        &client,
        vec![&env, 100_i128],
        100_i128,
        DepositMode::ExactTotal,
    );

    assert!(client.pause());
    super::assert_contract_error(
        client.try_raise_dispute(&escrow_id, &client_addr),
        EscrowError::ContractPaused,
    );

    assert!(client.unpause());
    assert!(client.raise_dispute(&escrow_id, &client_addr));
    assert!(client.pause());

    super::assert_contract_error(
        client.try_resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::FullRefund),
        EscrowError::ContractPaused,
    );
}
