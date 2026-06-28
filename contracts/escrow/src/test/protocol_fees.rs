#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, vec};
use crate::{Escrow, EscrowClient, DataKey, ReleaseAuthorization};

#[test]
fn test_default_fees_are_zero() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    // Default values before initialization or setting must be 0
    assert_eq!(client.get_protocol_fee_bps(), 0);
    assert_eq!(client.get_accumulated_protocol_fees(), 0);
}

/// Test that `get_protocol_fee_bps` returns 0 when uninitialized.
#[test]
fn test_get_protocol_fee_bps_returns_zero_when_uninitialized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    assert_eq!(client.get_protocol_fee_bps(), 0);
}

/// Test that `get_accumulated_protocol_fees` returns 0 when uninitialized.
#[test]
fn test_get_accumulated_protocol_fees_returns_zero_when_uninitialized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    assert_eq!(client.get_accumulated_protocol_fees(), 0);
}

/// Test that `get_protocol_fee_bps` returns the configured value after admin sets it.
#[test]
fn test_get_protocol_fee_bps_after_configuration() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin);

    assert_eq!(client.get_protocol_fee_bps(), 0);

    client.set_protocol_fee_bps(&500u32);
    assert_eq!(client.get_protocol_fee_bps(), 500);

    client.set_protocol_fee_bps(&1000u32);
    assert_eq!(client.get_protocol_fee_bps(), 1000);
}

/// Test that `get_accumulated_protocol_fees` reflects fees accumulated after milestone releases.
#[test]
fn test_get_accumulated_protocol_fees_after_releases() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin);
    let token_admin = Address::generate(&env);
    let token = create_token_contract(&env, &token_admin);
    client.set_settlement_token(&admin, &token);

    const FEE_BPS: u32 = 1000;
    client.set_protocol_fee_bps(&FEE_BPS);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestone_amounts = [1000_i128, 2500_i128, 3333_i128];
    let milestones = vec![
        &env,
        milestone_amounts[0],
        milestone_amounts[1],
        milestone_amounts[2],
    ];

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    soroban_sdk::token::StellarAssetClient::new(&env, &token)
        .mint(&client_addr, &6833_i128);
    client.deposit_funds(&id, &client_addr, &6833_i128);

    assert_eq!(client.get_accumulated_protocol_fees(), 0);

    let mut expected_accumulated = 0_i128;
    for (index, amount) in milestone_amounts.iter().enumerate() {
        let milestone_index = index as u32;
        client.approve_milestone_release(&id, &client_addr, &milestone_index);
        client.release_milestone(&id, &client_addr, &milestone_index);

        expected_accumulated += Escrow::calculate_protocol_fee(*amount, FEE_BPS);
        assert_eq!(client.get_accumulated_protocol_fees(), expected_accumulated);
    }

    let expected_total: i128 = milestone_amounts
        .iter()
        .map(|amount| Escrow::calculate_protocol_fee(*amount, FEE_BPS))
        .sum();
    assert_eq!(client.get_accumulated_protocol_fees(), expected_total);
}

/// Test that accumulated fees remain at 0 when fee rate is 0.
#[test]
fn test_no_fees_accumulated_when_rate_is_zero() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin);
    let token_admin = Address::generate(&env);
    let token = create_token_contract(&env, &token_admin);
    client.set_settlement_token(&admin, &token);
    assert_eq!(client.get_protocol_fee_bps(), 0);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_i128];

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    soroban_sdk::token::StellarAssetClient::new(&env, &token)
        .mint(&client_addr, &1000_i128);
    client.deposit_funds(&id, &client_addr, &1000_i128);
    client.approve_milestone_release(&id, &client_addr, &0);
    client.release_milestone(&id, &client_addr, &0);

    assert_eq!(client.get_accumulated_protocol_fees(), 0);
}

/// Test that read functions bump TTL and can be called multiple times without error.
#[test]
fn test_readers_bump_ttl_and_are_non_destructive() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.set_protocol_fee_bps(&250u32);

    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::AccumulatedProtocolFees, &5000_i128);
    });

    for _ in 0..10 {
        assert_eq!(client.get_protocol_fee_bps(), 250);
        assert_eq!(client.get_accumulated_protocol_fees(), 5000);
    }
}

/// Test readers work when keys are set directly without initialization.
#[test]
fn test_readers_work_without_initialization() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::ProtocolFeeBps, &123u32);
        env.storage()
            .persistent()
            .set(&DataKey::AccumulatedProtocolFees, &456_i128);
    });

    assert_eq!(client.get_protocol_fee_bps(), 123);
    assert_eq!(client.get_accumulated_protocol_fees(), 456);
}

#[test]
fn test_fee_math_0_bps() {
    let fee = Escrow::calculate_protocol_fee(1000, 0);
    assert_eq!(fee, 0);
}

#[test]
fn test_fee_math_normal_bps() {
    let fee = Escrow::calculate_protocol_fee(1000, 1000);
    assert_eq!(fee, 100);
}

#[test]
fn test_fee_math_overflow_returns_zero() {
    let fee = Escrow::calculate_protocol_fee(i128::MAX, 1000);
    assert_eq!(fee, 0);
}

#[test]
fn test_fee_math_tiny_amount() {
    // 9 * 1000 = 9000. 9000 / 10000 = 0 (rounds to zero)
    let fee = Escrow::calculate_protocol_fee(9, 1000);
    assert_eq!(fee, 0);
}

fn create_token_contract(e: &Env, admin: &Address) -> Address {
    e.register_stellar_asset_contract(admin.clone())
}

#[test]
fn test_fee_accrual_and_withdrawal() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);
    
    let token_admin = Address::generate(&env);
    let token = create_token_contract(&env, &token_admin);
    let token_client = soroban_sdk::token::Client::new(&env, &token);
    let token_admin_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);

    client.initialize(&admin);
    client.set_settlement_token(&admin, &token);
    client.set_protocol_fee_bps(&1000u32);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    
    let milestones = vec![&env, 1000_i128, 2500_i128, 3333_i128];
    
    let id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones, &crate::ReleaseAuthorization::ClientOnly);

    token_admin_client.mint(&client_addr, &6833_i128);
    client.deposit_funds(&id, &client_addr, &6833_i128);

    client.approve_milestone_release(&id, &client_addr, &0);
    assert!(client.release_milestone(&id, &client_addr, &0));
    
    client.approve_milestone_release(&id, &client_addr, &1);
    assert!(client.release_milestone(&id, &client_addr, &1));
    
    client.approve_milestone_release(&id, &client_addr, &2);
    assert!(client.release_milestone(&id, &client_addr, &2));

    let accumulated = client.get_accumulated_protocol_fees();
    assert_eq!(accumulated, 683);
    
    let destination = Address::generate(&env);
    
    // Bind/set SettlementToken in storage
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::SettlementToken, &token);
    });

    // Admin withdraws protocol fees
    let success = client.withdraw_protocol_fees(&683_i128, &destination);
    assert!(success);
    
    assert_eq!(token_client.balance(&destination), 683);
    assert_eq!(client.get_accumulated_protocol_fees(), 0);
}

#[test]
fn test_unauthorized_withdrawal() {
    let env = Env::default();
    
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);
    
    client.initialize(&admin);
    client.set_protocol_fee_bps(&1000u32);
    
    let destination = Address::generate(&env);
    
    // Call without mock_all_auths to verify auth check fails
    let result = client.try_withdraw_protocol_fees(&100_i128, &destination);
    assert!(result.is_err());
}

#[test]
fn test_over_withdrawal() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);
    
    client.initialize(&admin);
    client.set_protocol_fee_bps(&1000u32);
    
    let accumulated = 99_i128;
    let destination = Address::generate(&env);
    let token = Address::generate(&env);
    
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::SettlementToken, &token);
        env.storage()
            .persistent()
            .set(&DataKey::AccumulatedProtocolFees, &accumulated);
    });

    let result = client.try_withdraw_protocol_fees(&(accumulated + 1), &destination);
    assert_eq!(result, Err(Ok(crate::Error::InsufficientAccumulatedFees)));
}
