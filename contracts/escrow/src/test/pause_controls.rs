use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{Escrow, EscrowClient, ReleaseAuthorization};

// NOTE: These tests are for the two-tier pause model (soft pause vs emergency pause).
// They will compile once the escrow contract exposes the corresponding pause/emergency API.

#[test]
fn test_pause_tier_distinction_soft_pause_does_not_block_admin_emergency() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    // Create and fund a simple contract to put it in Funded status.
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

    // Admin pauses softly.
    // Expected behavior (once implemented): soft pause blocks milestone release,
    // but still allows the admin to escalate to emergency pause.
    env.mock_all_auths();
    client.pause_soft(&client_addr);

    // Escalate to emergency should still succeed.
    env.mock_all_auths();
    client.pause_emergency(&client_addr);
}

#[test]
#[should_panic(expected = "Only admin can unpause")]
fn test_pause_tier_distinction_emergency_blocks_unpause_by_non_admin() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin_addr = Address::generate(&env);
    let non_admin_addr = Address::generate(&env);

    env.mock_all_auths();
    client.pause_emergency(&admin_addr);

    // Expected behavior (once implemented): non-admin cannot unpause emergency.
    // Expected behavior (once implemented): non-admin cannot unpause emergency.
    // Use should_panic assertion style to match repo test conventions.
    //
    // This call is expected to panic with: "Only admin can unpause".
    env.mock_all_auths();
    client.unpause_all(&non_admin_addr);
}

#[test]
fn test_pause_tier_distinction_double_unpause_is_idempotent() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin_addr = Address::generate(&env);

    env.mock_all_auths();
    client.pause_soft(&admin_addr);

    env.mock_all_auths();
    client.unpause_all(&admin_addr);

    // Expected: second unpause should not panic.
    env.mock_all_auths();
    client.unpause_all(&admin_addr);
}
