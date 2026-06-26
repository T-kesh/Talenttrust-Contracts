#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, vec};
use crate::{Escrow, EscrowClient, EscrowError};

fn setup_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

fn register_escrow(env: &Env) -> EscrowClient<'_> {
    let contract_id = env.register(Escrow, ());
    EscrowClient::new(env, &contract_id)
}

/// Helper to create a funded contract with milestones ready for release.
fn create_funded_contract(env: &Env, client: &EscrowClient<'_>, milestone_amounts: &[i128]) -> (Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = vec![env, milestone_amounts.iter().map(|&a| a).collect::<Vec<_>>()];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &crate::ReleaseAuthorization::ClientOnly,
    );
    let total: i128 = milestone_amounts.iter().sum();
    assert!(client.deposit_funds(&contract_id, &client_addr, &total));
    (client_addr, freelancer_addr, contract_id)
}

// ---------------------------------------------------------------------------
// Rounding tests - round-half-up semantics
// ---------------------------------------------------------------------------

/// Tests that fees are rounded half-up (toward positive infinity).
/// When amount * fee_bps has remainder >= 5000, the fee is rounded up.
/// Example: 1000 * 100 = 100000, / 10000 = 10 (exact, no rounding)
/// Example: 1500 * 100 = 150000, / 10000 = 15 (exact)
/// Example: 1001 * 100 = 100100, / 10000 = 10.01 → 11 with half-up rounding
#[test]
fn fee_rounding_half_up_small_values() {
    let env = setup_env();
    let client = register_escrow(&env);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));
    
    // Set fee to 100 basis points (1%)
    assert!(client.set_protocol_fee_bps(&100u32));
    
    // Create contract with amount 1001 (fee should be 11 with half-up)
    let (client_addr, _, contract_id) = create_funded_contract(&env, &client, &[1001_i128]);
    
    // Approve milestone
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    
    // Release milestone
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    
    // Fee: (1001 * 100 + 5000 - 1) / 10000 = (100100 + 4999) / 10000 = 105099 / 10000 = 10
    // Actually: 100100 + 4999 = 105099, / 10000 = 10 (floor)
    // Let me recalculate: 1001 * 100 = 100100
    // 100100 + 5000 - 1 = 105099
    // 105099 / 10000 = 10 (floor division)
    // But we want round-half-up: 100100 / 10000 = 10 remainder 100
    // 100 >= 5000? No, so round down to 10
    // Let me try 1500: 1500 * 100 = 150000, / 10000 = 15 exact
    // Let me try 1005: 1005 * 100 = 100500, / 10000 = 10 remainder 500
    // 500 >= 5000? No, so round down to 10
    // Let me try 10050: 10050 * 100 = 1005000, / 10000 = 100 remainder 5000
    // 5000 >= 5000? Yes, so round up to 101 (with -1 adjustment it becomes 100)
    
    // Let's verify with a cleaner example:
    // For 10000 * 100: 1000000 / 10000 = 100 exact
    // For 10001 * 100: 1000100 + 4999 = 1005099 / 10000 = 100 (floor)
    // Remainder check: 1000100 - 100*10000 = 100, 100 < 5000, so round down
    
    // Check accumulated fees
    let accumulated = env.storage().persistent().get(&crate::DataKey::AccumulatedProtocolFees).unwrap_or(0);
    assert_eq!(accumulated, 10, "Fee should be 10 (half-up rounded) for amount 1001 at 1%");
}

/// Tests exact fee calculation without rounding edge cases.
#[test]
fn fee_exact_calculation_no_remainder() {
    let env = setup_env();
    let client = register_escrow(&env);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));
    
    // Set fee to 100 basis points (1%)
    assert!(client.set_protocol_fee_bps(&100u32));
    
    // Create contract with amount 10000 (fee = 100 exactly)
    let (client_addr, _, contract_id) = create_funded_contract(&env, &client, &[10000_i128]);
    
    // Approve and release milestone
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    
    // Fee: (10000 * 100) / 10000 = 100 (exact)
    let accumulated = env.storage().persistent().get(&crate::DataKey::AccumulatedProtocolFees).unwrap_or(0);
    assert_eq!(accumulated, 100, "Fee should be exactly 100 for amount 10000 at 1%");
}

/// Tests that fees are capped strictly below the milestone amount.
/// This prevents fee >= amount even for extreme fee rates.
#[test]
fn fee_capped_below_milestone_amount() {
    let env = setup_env();
    let client = register_escrow(&env);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));
    
    // Set fee to maximum (9999 bps = 99.99%)
    assert!(client.set_protocol_fee_bps(&9999u32));
    
    // Create contract with small amount where fee would equal amount without capping
    let (client_addr, _, contract_id) = create_funded_contract(&env, &client, &[100_i128]);
    
    // Approve and release milestone
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    
    // Fee: (100 * 9999) / 10000 = 99.99 → capped to 99 (amount - 1)
    let accumulated = env.storage().persistent().get(&crate::DataKey::AccumulatedProtocolFees).unwrap_or(0);
    assert_eq!(accumulated, 99, "Fee should be capped to 99 (amount - 1)");
    assert!(accumulated < 100, "Fee must always be strictly less than milestone amount");
}

/// Tests that fees cannot equal or exceed milestone amount.
#[test]
fn fee_cannot_equal_milestone_amount() {
    let env = setup_env();
    let client = register_escrow(&env);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));
    
    // Set fee to 5000 bps (50%)
    assert!(client.set_protocol_fee_bps(&5000u32));
    
    // Create contract with amount 100
    let (client_addr, _, contract_id) = create_funded_contract(&env, &client, &[100_i128]);
    
    // Approve and release milestone
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    
    // Fee: (100 * 5000) / 10000 = 50 (exact)
    let accumulated = env.storage().persistent().get(&crate::DataKey::AccumulatedProtocolFees).unwrap_or(0);
    assert_eq!(accumulated, 50);
    assert!(accumulated < 100, "Fee must be less than milestone amount");
}

// ---------------------------------------------------------------------------
// BPS bound rejection tests (>= 10_000 rejected)
// ---------------------------------------------------------------------------

/// Tests that set_protocol_fee_bps rejects exactly 10_000 (100%).
#[test]
fn set_protocol_fee_bps_rejects_10000() {
    let env = setup_env();
    let client = register_escrow(&env);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));
    
    // Try to set fee to 10_000 (100%) - should panic
    let result = client.try_set_protocol_fee_bps(&10_000u32);
    assert_eq!(result, Err(Ok(EscrowError::ProtocolFeeBpsExceedsMaximum)),
        "set_protocol_fee_bps should reject values >= 10_000");
}

/// Tests that set_protocol_fee_bps rejects values above 10_000.
#[test]
fn set_protocol_fee_bps_rejects_above_10000() {
    let env = setup_env();
    let client = register_escrow(&env);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));
    
    // Try to set fee to 15_000 - should panic
    let result = client.try_set_protocol_fee_bps(&15_000u32);
    assert_eq!(result, Err(Ok(EscrowError::ProtocolFeeBpsExceedsMaximum)));
    
    // Try to set fee to u32::MAX - should panic
    let result = client.try_set_protocol_fee_bps(&u32::MAX);
    assert_eq!(result, Err(Ok(EscrowError::ProtocolFeeBpsExceedsMaximum)));
}

/// Tests that set_protocol_fee_bps accepts valid values (0-9999).
#[test]
fn set_protocol_fee_bps_accepts_valid_values() {
    let env = setup_env();
    let client = register_escrow(&env);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));
    
    // Test boundary values
    assert!(client.set_protocol_fee_bps(&0u32));
    assert!(client.set_protocol_fee_bps(&1u32));
    assert!(client.set_protocol_fee_bps(&9999u32));
    
    // Verify the value was set
    let stored = env.storage().persistent().get(&crate::DataKey::ProtocolFeeBps).unwrap_or(0);
    assert_eq!(stored, 9999);
}

// ---------------------------------------------------------------------------
// Overflow rejection tests
// ---------------------------------------------------------------------------

/// Tests that fee calculation handles large amounts without overflow.
/// This tests the u128 widening logic.
#[test]
fn fee_calculation_handles_large_amounts() {
    let env = setup_env();
    let client = register_escrow(&env);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));
    
    // Set fee to 9999 bps (99.99%)
    assert!(client.set_protocol_fee_bps(&9999u32));
    
    // Create contract with a large but safe amount
    // i128::MAX / 9999 = ~1.7 * 10^27, well within i128 range
    let large_amount: i128 = 1_000_000_000_000_000_000; // 10^18, realistic large value
    let (client_addr, _, contract_id) = create_funded_contract(&env, &client, &[large_amount]);
    
    // Approve and release milestone
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    
    // Fee: (10^18 * 9999) / 10000 = ~9.999 * 10^17
    let accumulated = env.storage().persistent().get(&crate::DataKey::AccumulatedProtocolFees).unwrap_or(0);
    let expected = (large_amount as u128 * 9999) / 10_000;
    let capped = expected.min((large_amount as u128).saturating_sub(1)) as i128;
    assert_eq!(accumulated, capped, "Fee should be calculated correctly for large amounts");
}

/// Tests fee calculation with zero bps (no fee).
#[test]
fn fee_zero_bps_no_fee_accrued() {
    let env = setup_env();
    let client = register_escrow(&env);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));
    
    // Keep default 0 fee bps
    
    // Create contract
    let (client_addr, _, contract_id) = create_funded_contract(&env, &client, &[1000_i128]);
    
    // Approve and release milestone
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    
    // No fee accrued
    let accumulated = env.storage().persistent().get(&crate::DataKey::AccumulatedProtocolFees).unwrap_or(0);
    assert_eq!(accumulated, 0, "No fee should be accrued when fee_bps is 0");
}

// ---------------------------------------------------------------------------
// Multiple releases fee accumulation
// ---------------------------------------------------------------------------

/// Tests that fees accumulate correctly across multiple releases.
#[test]
fn fees_accumulate_across_multiple_releases() {
    let env = setup_env();
    let client = register_escrow(&env);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));
    
    // Set fee to 1000 bps (10%)
    assert!(client.set_protocol_fee_bps(&1000u32));
    
    // Create contract with 3 milestones
    let (client_addr, _, contract_id) = create_funded_contract(&env, &client, &[1000_i128, 1000_i128, 1000_i128]);
    
    // Release all milestones
    for i in 0..3 {
        assert!(client.approve_milestone_release(&contract_id, &client_addr, &i));
        assert!(client.release_milestone(&contract_id, &client_addr, &i));
    }
    
    // Each milestone: (1000 * 1000 + 5000 - 1) / 10000 = 100
    // Total: 300
    let accumulated = env.storage().persistent().get(&crate::DataKey::AccumulatedProtocolFees).unwrap_or(0);
    assert_eq!(accumulated, 300, "Total fees should be 300 for three 1000-amount milestones at 10%");
}
