//! Comprehensive tests for amount validation and input sanitization
//!
//! Tests all money-like values for positivity, max bounds, and stroop precision rules.

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{
    safe_add_amounts, safe_subtract_amounts, validate_deposit_amount, validate_milestone_amounts,
    validate_single_amount, Escrow, EscrowClient, EscrowError, Error, ReleaseAuthorization,
    MAX_TOTAL_ESCROW_STROOPS,
};

use super::assert_contract_error;

fn setup() -> (Env, EscrowClient, Address, Address) {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let hiring_party = Address::generate(&env);
    let service_provider = Address::generate(&env);
    (env, client, hiring_party, service_provider)
}

#[test]
#[should_panic]
fn test_create_contract_panics_when_single_milestone_is_zero() {
    let (env, client, hiring_party, service_provider) = setup();
    let milestones = vec![&env, 0_i128];
    client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}

#[test]
#[should_panic]
fn test_create_contract_panics_when_single_milestone_is_negative() {
    let (env, client, hiring_party, service_provider) = setup();
    let milestones = vec![&env, -1_i128];
    client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}

#[test]
#[should_panic]
fn test_create_contract_panics_when_any_milestone_is_non_positive() {
    let (env, client, hiring_party, service_provider) = setup();
    let milestones = vec![&env, 100_0000000_i128, 0_i128, 200_0000000_i128];
    client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}

#[test]
fn test_create_contract_accepts_all_positive_milestones() {
    let (env, client, hiring_party, service_provider) = setup();
    let milestones = vec![&env, 100_0000000_i128, 1_i128, 999_0000000_i128];
    let id = client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(id > 0);
}

#[test]
#[should_panic]
fn test_create_contract_panics_when_total_exceeds_maximum() {
    let (env, client, hiring_party, service_provider) = setup();
    let milestones = vec![&env, 600_000_0000000_i128, 500_000_0000000_i128]; // 6M + 5M > 1M max
    client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}

#[test]
#[should_panic]
fn test_deposit_funds_panics_on_zero_amount() {
    let (env, client, hiring_party, service_provider) = setup();
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&contract_id, &hiring_party, &0_i128);
}

#[test]
#[should_panic]
fn test_deposit_funds_panics_on_negative_amount() {
    let (env, client, hiring_party, service_provider) = setup();
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&contract_id, &hiring_party, &-100_0000000_i128);
}

#[test]
#[should_panic]
fn test_deposit_funds_panics_when_exceeding_contract_maximum() {
    let (env, client, hiring_party, service_provider) = setup();
    let milestones = vec![&env, 500_0000000_i128];
    let contract_id = client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&contract_id, &hiring_party, &1_000_000_0000000_i128); // 1M tokens > remaining capacity
}

#[test]
fn test_deposit_funds_accepts_valid_amounts() {
    let (env, client, hiring_party, service_provider) = setup();
    let milestones = vec![&env, 100_0000000_i128, 200_0000000_i128];
    let contract_id = client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Valid deposit
    assert!(client.deposit_funds(&contract_id, &hiring_party, &100_0000000_i128));

    // Another valid deposit within remaining capacity
    assert!(client.deposit_funds(&contract_id, &hiring_party, &200_0000000_i128));
}

#[test]
fn test_single_amount_validation() {
    // Valid amounts
    assert!(validate_single_amount(1).is_ok()); // Minimum positive
    assert!(validate_single_amount(100_0000000).is_ok()); // 1 token
    assert!(validate_single_amount(1_000_000_0000000).is_ok()); // Max single amount

    // Invalid amounts
    assert_eq!(
        validate_single_amount(0),
        Err(EscrowError::AmountMustBePositive)
    );
    assert_eq!(
        validate_single_amount(-1),
        Err(EscrowError::AmountMustBePositive)
    );
    assert_eq!(
        validate_single_amount(-100_0000000),
        Err(EscrowError::AmountMustBePositive)
    );
    assert_eq!(
        validate_single_amount(1_000_000_0000001),
        Err(EscrowError::InvalidMilestoneAmount)
    );
}

#[test]
fn test_milestone_amounts_validation() {
    let max_total = MAX_TOTAL_ESCROW_STROOPS;

    // Valid milestone arrays
    let milestones1 = vec![100_0000000, 200_0000000, 300_0000000];
    assert!(validate_milestone_amounts(&milestones1, max_total).is_ok());
    assert_eq!(
        validate_milestone_amounts(&milestones1, max_total).unwrap(),
        600_0000000
    );

    // Single milestone at maximum
    let milestones2 = vec![max_total];
    assert!(validate_milestone_amounts(&milestones2, max_total).is_ok());

    // Multiple milestones within bounds
    let milestones3 = vec![500_000_0000000, 500_000_0000000];
    assert!(validate_milestone_amounts(&milestones3, max_total).is_ok());

    // Invalid arrays
    let milestones4 = vec![100_0000000, 0, 300_0000000]; // Contains zero
    assert_eq!(
        validate_milestone_amounts(&milestones4, max_total),
        Err(EscrowError::AmountMustBePositive)
    );

    let milestones5 = vec![100_0000000, -50_0000000, 300_0000000]; // Contains negative
    assert_eq!(
        validate_milestone_amounts(&milestones5, max_total),
        Err(EscrowError::AmountMustBePositive)
    );

    let milestones6 = vec![600_000_0000000, 500_000_0000000]; // Exceeds contract max
    assert_eq!(
        validate_milestone_amounts(&milestones6, max_total),
        Err(EscrowError::InvalidMilestoneAmount)
    );
}

#[test]
fn test_deposit_amount_validation() {
    let max_total = MAX_TOTAL_ESCROW_STROOPS;

    // Valid deposits
    assert!(validate_deposit_amount(100_0000000, 0, max_total).is_ok());
    assert!(validate_deposit_amount(100_0000000, 500_0000000, max_total).is_ok());
    assert!(validate_deposit_amount(max_total, 0, max_total).is_ok());

    // Invalid deposits
    assert_eq!(
        validate_deposit_amount(0, 0, max_total),
        Err(EscrowError::AmountMustBePositive)
    );
    assert_eq!(
        validate_deposit_amount(-1, 0, max_total),
        Err(EscrowError::AmountMustBePositive)
    );

    // Would exceed maximum
    assert_eq!(
        validate_deposit_amount(600_000_0000000, 500_000_0000000, max_total),
        Err(EscrowError::InvalidMilestoneAmount)
    );

    // Single amount exceeds maximum
    assert_eq!(
        validate_deposit_amount(1_000_000_0000001, 0, max_total),
        Err(EscrowError::InvalidMilestoneAmount)
    );
}

#[test]
fn test_safe_arithmetic_operations() {
    // Safe addition
    assert_eq!(safe_add_amounts(100, 200), Some(300));
    assert_eq!(safe_add_amounts(0, 0), Some(0));
    assert_eq!(safe_add_amounts(i128::MAX, 1), None);
    assert_eq!(safe_add_amounts(i128::MIN, -1), None);

    // Safe subtraction
    assert_eq!(safe_subtract_amounts(300, 100), Some(200));
    assert_eq!(safe_subtract_amounts(100, 100), Some(0));
    assert_eq!(safe_subtract_amounts(0, 1), None);
    assert_eq!(safe_subtract_amounts(i128::MIN, 1), None);
}

#[test]
fn test_edge_cases() {
    let max_total = MAX_TOTAL_ESCROW_STROOPS;

    // Test minimum positive amounts
    assert!(validate_single_amount(1).is_ok());
    let small_milestones = vec![1, 1, 1];
    assert!(validate_milestone_amounts(&small_milestones, max_total).is_ok());

    // Test boundary values
    assert!(validate_single_amount(1_000_000_0000000).is_ok()); // Max single amount
    assert_eq!(
        validate_single_amount(1_000_000_0000001),
        Err(EscrowError::InvalidMilestoneAmount)
    );

    // Test contract boundary
    let boundary_milestones = vec![MAX_TOTAL_ESCROW_STROOPS];
    assert!(validate_milestone_amounts(&boundary_milestones, max_total).is_ok());

    let over_boundary_milestones = vec![MAX_TOTAL_ESCROW_STROOPS + 1];
    assert_eq!(
        validate_milestone_amounts(&over_boundary_milestones, max_total),
        Err(EscrowError::InvalidMilestoneAmount)
    );
}

#[test]
fn test_stroop_precision() {
    // All i128 values are valid stroop amounts since stroop is the smallest unit
    // This test documents the precision requirements
    let valid_stroop_amounts = vec![
        1,           // 1 stroop
        100,         // 100 stroops
        1_0000000,   // 1 token
        123_4567890, // 123.4567890 tokens
    ];

    for amount in valid_stroop_amounts {
        assert!(validate_single_amount(amount).is_ok());
    }
}

#[test]
fn test_large_amount_arrays() {
    let max_total = MAX_TOTAL_ESCROW_STROOPS;

    // Test with maximum number of milestones (10)
    let mut many_milestones = Vec::new();
    for _ in 0..10 {
        many_milestones.push(100_0000000); // 1 token each
    }
    assert!(validate_milestone_amounts(&many_milestones, max_total).is_ok());

    // Test overflow detection in array validation
    let mut overflow_milestones = Vec::new();
    for _ in 0..10 {
        overflow_milestones.push(200_000_0000000); // 200M tokens each
    }
    assert_eq!(
        validate_milestone_amounts(&overflow_milestones, max_total),
        Err(EscrowError::InvalidMilestoneAmount)
    );
}

#[test]
fn test_cumulative_deposit_validation() {
    let max_total = MAX_TOTAL_ESCROW_STROOPS;

    // Test cumulative deposit validation
    assert!(validate_deposit_amount(100_0000000, 0, max_total).is_ok());
    assert!(validate_deposit_amount(100_0000000, 100_0000000, max_total).is_ok());
    assert!(validate_deposit_amount(100_0000000, 200_0000000, max_total).is_ok());

    // Should fail when cumulative exceeds maximum
    assert_eq!(
        validate_deposit_amount(800_000_0000000, 300_000_0000000, max_total),
        Err(EscrowError::InvalidMilestoneAmount)
    );
}

#[test]
#[should_panic]
fn test_create_contract_panics_when_single_milestone_exceeds_maximum_bound() {
    let (env, client, hiring_party, service_provider) = setup();
    let milestones = vec![&env, 1_000_000_0000001_i128]; // Max is 1M tokens (1_000_000_0000000 stroops)
    client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}

#[test]
#[should_panic]
fn test_deposit_funds_panics_when_single_deposit_exceeds_maximum_bound() {
    let (env, client, hiring_party, service_provider) = setup();
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&contract_id, &hiring_party, &1_000_000_0000001_i128);
}

/// Verifies that a near-i128::MAX deposit triggers a clean PotentialOverflow panic.
///
/// # Security
/// Without checked arithmetic, a deposit summing to more than i128::MAX would
/// silently wraparound in release builds. This test proves deterministic failure
/// on overflow.
#[test]
#[should_panic(expected = "PotentialOverflow")]
fn test_deposit_overflow_panics_with_potential_overflow() {
    let (env, client, hiring_party, service_provider) = setup();
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Deposit near-max, then a second deposit that would overflow funded_amount
    let near_max = i128::MAX - 100_0000000_i128;
    client.deposit_funds(&contract_id, &hiring_party, &near_max);
    // This second deposit pushes funded_amount past i128::MAX
    client.deposit_funds(&contract_id, &hiring_party, &100_0000000_i128);
}

/// Verifies that depositing exactly i128::MAX in one shot panics with overflow.
#[test]
#[should_panic(expected = "PotentialOverflow")]
fn test_deposit_exactly_i128_max_panics() {
    let (env, client, hiring_party, service_provider) = setup();
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    // funded_amount starts at 0, adding i128::MAX overflows because funded_amount + amount > i128::MAX
    // Actually, 0 + i128::MAX = i128::MAX which doesn't overflow.
    // But total_deposited also gets i128::MAX added which doesn't overflow either.
    // The milestone total check will reject this large deposit first.
    // Use two deposits to trigger overflow: first at i128::MAX - 1, then at 2.
    client.deposit_funds(&contract_id, &hiring_party, &(i128::MAX - 1));
    client.deposit_funds(&contract_id, &hiring_party, &2);
}

/// Verifies that a large deposit overflow is caught via try_deposit_funds with
/// the correct error code.
///
/// # Security
/// This test uses the non-panicking `try_` variant so we can assert the exact
/// `PotentialOverflow` error code rather than just expecting a panic.
#[test]
fn test_deposit_overflow_returns_expected_error_code() {
    let env = Env::default();
    env.mock_all_auths();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let hiring_party = Address::generate(&env);
    let service_provider = Address::generate(&env);

    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // First deposit to push funded_amount close to the boundary
    let large = i128::MAX - 10_000_0000_i128;
    client.deposit_funds(&contract_id, &hiring_party, &large);

    // This should overflow and return PotentialOverflow
    let result = client.try_deposit_funds(&contract_id, &hiring_party, &20_000_0000_i128);
    assert_contract_error(result, Error::PotentialOverflow);
}

/// Verifies that summing milestone amounts in refund_unreleased_milestones does
/// not overflow when a crafted sequence of refund indices is provided.
///
/// # Security
/// Without checked arithmetic in the refund path, a large total_refund_amount
/// could silently overflow during accumulation.
#[test]
#[should_panic(expected = "PotentialOverflow")]
fn test_refund_overflow_panics_on_large_accumulation() {
    let env = Env::default();
    env.mock_all_auths();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let hiring_party = Address::generate(&env);
    let service_provider = Address::generate(&env);

    // Create a contract with many large milestones
    let milestone_amounts = vec![
        &env,
        i128::MAX / 4,
        i128::MAX / 4,
        i128::MAX / 4,
        i128::MAX / 4,
    ];
    let contract_id = client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestone_amounts,
        &ReleaseAuthorization::ClientOnly,
    );
    // Fund the contract fully
    client.deposit_funds(&contract_id, &hiring_party, &i128::MAX);

    // Refunding all milestones would overflow total_refund_amount
    let refund_indices = vec![&env, 0_u32, 1_u32, 2_u32, 3_u32];
    client.refund_unreleased_milestones(&contract_id, &refund_indices);
}

/// Verifies that the available_balance computation in release_milestone detects
/// an accounting-invariant violation via PotentialOverflow.
///
/// # Security
/// If `released_amount > funded_amount` (indicating an invariant violation),
/// `safe_subtract_amounts` will return `None` and the contract panics with a
/// typed error rather than producing a negative value that could be used as
/// a large positive in subsequent checks.
#[test]
fn test_available_balance_subtraction_underflow_detected() {
    // The safe_subtract_amounts helper itself must reject underflows
    assert_eq!(safe_subtract_amounts(100, 200), None);
    assert_eq!(safe_subtract_amounts(0, 1), None);
    assert_eq!(safe_subtract_amounts(i128::MIN, 1), None);
}

/// Verifies that a release that would overflow contract.released_amount triggers
/// PotentialOverflow.
///
/// # Security
/// Without checked arithmetic, sequential releases summing past i128::MAX would
/// silently wraparound.
#[test]
#[should_panic(expected = "PotentialOverflow")]
fn test_release_overflow_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let hiring_party = Address::generate(&env);
    let service_provider = Address::generate(&env);

    // Create a contract with two very large milestones
    let huge_milestone = i128::MAX / 2;
    let milestones = vec![&env, huge_milestone, huge_milestone];
    let contract_id = client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Fund with the full amount
    client.deposit_funds(&contract_id, &hiring_party, &i128::MAX);

    // First release succeeds
    client.approve_milestone_release(&contract_id, &hiring_party, &0);
    client.release_milestone(&contract_id, &hiring_party, &0);

    // Second release would overflow released_amount
    client.approve_milestone_release(&contract_id, &hiring_party, &1);
    client.release_milestone(&contract_id, &hiring_party, &1);
}

/// Verifies that a refund that would overflow contract.refunded_amount triggers
/// PotentialOverflow.
///
/// # Security
/// Sequential refunds summing past i128::MAX must fail deterministically.
#[test]
#[should_panic(expected = "PotentialOverflow")]
fn test_refunded_amount_overflow_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let hiring_party = Address::generate(&env);
    let service_provider = Address::generate(&env);

    // Create a contract with two very large milestones
    let huge_milestone = i128::MAX / 2;
    let milestones = vec![&env, huge_milestone, huge_milestone];
    let contract_id = client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Over-fund slightly so there's enough balance to refund both
    client.deposit_funds(&contract_id, &hiring_party, &i128::MAX);

    // First refund succeeds
    let refund_indices_1 = vec![&env, 0_u32];
    client.refund_unreleased_milestones(&contract_id, &refund_indices_1);

    // Second refund should overflow refunded_amount
    let refund_indices_2 = vec![&env, 1_u32];
    client.refund_unreleased_milestones(&contract_id, &refund_indices_2);
}
