//! Storage TTL tests for transient approval and migration entries.
//!
//! These tests exercise the TTL helpers in [`crate::ttl`] directly via
//! `env.as_contract`, advancing the ledger sequence to prove Soroban's
//! auto-eviction semantics for both approval and migration TTL constants.

#![cfg(test)]

use soroban_sdk::{symbol_short, testutils::Ledger as _, Env, Symbol};

use crate::{
    ttl::{
        compute_expiry, extend_if_below_threshold, has_transient, read_if_live, remove_transient,
        store_with_ttl,
    },
    Escrow, LEDGERS_PER_DAY, PENDING_APPROVAL_BUMP_THRESHOLD, PENDING_APPROVAL_TTL_LEDGERS,
    PENDING_MIGRATION_BUMP_THRESHOLD, PENDING_MIGRATION_TTL_LEDGERS,
};

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Large enough that the contract instance never archives during any test.
const INSTANCE_TTL: u32 = PENDING_MIGRATION_TTL_LEDGERS * 4;

fn setup() -> (Env, soroban_sdk::Address) {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.max_entry_ttl = INSTANCE_TTL;
        li.min_persistent_entry_ttl = INSTANCE_TTL;
        li.sequence_number = 1_000;
    });
    let contract_id = env.register(Escrow, ());
    (env, contract_id)
}

/// Advance the ledger sequence and keep the contract instance alive by
/// extending its persistent TTL to `INSTANCE_TTL` after the jump.
fn advance(env: &Env, contract_id: &soroban_sdk::Address, by: u32) {
    env.ledger()
        .with_mut(|li| li.sequence_number = li.sequence_number.saturating_add(by));
    // Re-extend the contract instance so it is never archived.
    env.as_contract(contract_id, || {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_TTL, INSTANCE_TTL);
    });
}

fn approval_key() -> Symbol {
    symbol_short!("appr")
}

fn migration_key() -> Symbol {
    symbol_short!("migr")
}

// ─── compute_expiry ───────────────────────────────────────────────────────────

#[test]
fn compute_expiry_equals_sequence_plus_ttl() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        let seq = env.ledger().sequence();
        assert_eq!(
            compute_expiry(&env, PENDING_APPROVAL_TTL_LEDGERS),
            seq + PENDING_APPROVAL_TTL_LEDGERS
        );
        assert_eq!(
            compute_expiry(&env, PENDING_MIGRATION_TTL_LEDGERS),
            seq + PENDING_MIGRATION_TTL_LEDGERS
        );
    });
}

#[test]
fn compute_expiry_saturates_on_overflow() {
    // Verify the saturating_add contract without needing the host at u32::MAX.
    // At sequence 1_000 (setup default), adding u32::MAX saturates to u32::MAX.
    let (env, id) = setup();
    env.as_contract(&id, || {
        let seq = env.ledger().sequence(); // 1_000
                                           // saturating_add(u32::MAX) from any non-zero sequence == u32::MAX
        assert_eq!(compute_expiry(&env, u32::MAX - seq), u32::MAX);
        // One more would overflow without saturation; with it we stay at u32::MAX.
        assert_eq!(compute_expiry(&env, u32::MAX), u32::MAX);
    });
}

// ─── LEDGERS_PER_DAY math ─────────────────────────────────────────────────────

#[test]
fn ledgers_per_day_constant_is_correct() {
    assert_eq!(LEDGERS_PER_DAY, 17_280);
    assert_eq!(PENDING_APPROVAL_TTL_LEDGERS, LEDGERS_PER_DAY * 7);
    assert_eq!(PENDING_MIGRATION_TTL_LEDGERS, LEDGERS_PER_DAY * 21);
    assert_eq!(PENDING_APPROVAL_BUMP_THRESHOLD, LEDGERS_PER_DAY);
    assert_eq!(PENDING_MIGRATION_BUMP_THRESHOLD, LEDGERS_PER_DAY * 3);
}

// ─── Approval TTL: read_if_live ───────────────────────────────────────────────

#[test]
fn approval_readable_before_expiry() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        store_with_ttl(&env, &approval_key(), &42u32, PENDING_APPROVAL_TTL_LEDGERS);
    });

    // One ledger before expiry — entry must still be live.
    advance(&env, &id, PENDING_APPROVAL_TTL_LEDGERS - 1);

    env.as_contract(&id, || {
        let val: Option<u32> = read_if_live(&env, &approval_key());
        assert_eq!(val, Some(42u32), "entry must be live before TTL elapses");
    });
}

#[test]
fn approval_evicted_after_expiry() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        store_with_ttl(&env, &approval_key(), &99u32, PENDING_APPROVAL_TTL_LEDGERS);
    });

    // One ledger past the TTL — entry must be evicted.
    advance(&env, &id, PENDING_APPROVAL_TTL_LEDGERS + 1);

    env.as_contract(&id, || {
        let val: Option<u32> = read_if_live(&env, &approval_key());
        assert!(val.is_none(), "entry must be evicted after TTL elapses");
    });
}

// ─── Migration TTL: read_if_live ─────────────────────────────────────────────

#[test]
fn migration_readable_before_expiry() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        store_with_ttl(&env, &migration_key(), &7u32, PENDING_MIGRATION_TTL_LEDGERS);
    });

    advance(&env, &id, PENDING_MIGRATION_TTL_LEDGERS - 1);

    env.as_contract(&id, || {
        let val: Option<u32> = read_if_live(&env, &migration_key());
        assert_eq!(
            val,
            Some(7u32),
            "migration entry must be live before TTL elapses"
        );
    });
}

#[test]
fn migration_evicted_after_expiry() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        store_with_ttl(&env, &migration_key(), &7u32, PENDING_MIGRATION_TTL_LEDGERS);
    });

    advance(&env, &id, PENDING_MIGRATION_TTL_LEDGERS + 1);

    env.as_contract(&id, || {
        let val: Option<u32> = read_if_live(&env, &migration_key());
        assert!(
            val.is_none(),
            "migration entry must be evicted after TTL elapses"
        );
    });
}

// ─── extend_if_below_threshold ───────────────────────────────────────────────

#[test]
fn extend_returns_false_for_absent_key() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        let result = extend_if_below_threshold(
            &env,
            &approval_key(),
            PENDING_APPROVAL_BUMP_THRESHOLD,
            PENDING_APPROVAL_TTL_LEDGERS,
        );
        assert!(!result, "must return false when key is absent");
    });
}

#[test]
fn extend_returns_true_and_entry_survives_past_original_expiry() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        store_with_ttl(&env, &approval_key(), &1u32, PENDING_APPROVAL_TTL_LEDGERS);
    });

    // Advance to within the bump threshold (TTL nearly exhausted).
    advance(
        &env,
        &id,
        PENDING_APPROVAL_TTL_LEDGERS - PENDING_APPROVAL_BUMP_THRESHOLD + 1,
    );

    env.as_contract(&id, || {
        let bumped = extend_if_below_threshold(
            &env,
            &approval_key(),
            PENDING_APPROVAL_BUMP_THRESHOLD,
            PENDING_APPROVAL_TTL_LEDGERS,
        );
        assert!(bumped, "must return true for a live entry");
    });

    // Advance past the *original* expiry — entry should still be live after bump.
    advance(&env, &id, PENDING_APPROVAL_BUMP_THRESHOLD + 1);

    env.as_contract(&id, || {
        let val: Option<u32> = read_if_live(&env, &approval_key());
        assert!(
            val.is_some(),
            "entry must survive past original expiry after bump"
        );
    });
}

#[test]
fn extend_migration_returns_false_for_absent_key() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        let result = extend_if_below_threshold(
            &env,
            &migration_key(),
            PENDING_MIGRATION_BUMP_THRESHOLD,
            PENDING_MIGRATION_TTL_LEDGERS,
        );
        assert!(!result);
    });
}

// ─── remove_transient ────────────────────────────────────────────────────────

#[test]
fn remove_transient_clears_entry_immediately() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        store_with_ttl(&env, &approval_key(), &5u32, PENDING_APPROVAL_TTL_LEDGERS);
        assert!(
            has_transient(&env, &approval_key()),
            "entry must exist before removal"
        );
        remove_transient(&env, &approval_key());
        assert!(
            !has_transient(&env, &approval_key()),
            "entry must be gone after removal"
        );
        let val: Option<u32> = read_if_live(&env, &approval_key());
        assert!(val.is_none(), "read_if_live must return None after removal");
    });
}

#[test]
fn remove_transient_is_idempotent() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        store_with_ttl(&env, &approval_key(), &5u32, PENDING_APPROVAL_TTL_LEDGERS);
        remove_transient(&env, &approval_key());
        // Second remove must not panic.
        remove_transient(&env, &approval_key());
        assert!(!has_transient(&env, &approval_key()));
    });
}

// ─── has_transient ────────────────────────────────────────────────────────────

#[test]
fn has_transient_false_before_store() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        assert!(!has_transient(&env, &approval_key()));
    });
}

#[test]
fn has_transient_true_after_store_false_after_expiry() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        store_with_ttl(&env, &approval_key(), &1u32, PENDING_APPROVAL_TTL_LEDGERS);
        assert!(has_transient(&env, &approval_key()));
    });

    advance(&env, &id, PENDING_APPROVAL_TTL_LEDGERS + 1);

    env.as_contract(&id, || {
        assert!(
            !has_transient(&env, &approval_key()),
            "has_transient must be false after eviction"
        );
    });
}

// ─── Determinism ─────────────────────────────────────────────────────────────

#[test]
fn expiry_is_deterministic_across_independent_envs() {
    let (env_a, id_a) = setup();
    let (env_b, id_b) = setup();

    let expiry_a = env_a.as_contract(&id_a, || {
        compute_expiry(&env_a, PENDING_APPROVAL_TTL_LEDGERS)
    });
    let expiry_b = env_b.as_contract(&id_b, || {
        compute_expiry(&env_b, PENDING_APPROVAL_TTL_LEDGERS)
    });

    assert_eq!(
        expiry_a, expiry_b,
        "expiry must be deterministic given the same starting sequence"
    );
}

// ─── Persistent key TTL survival tests (#400) ────────────────────────────────
//
// These tests initialise the contract, perform operations that write the
// critical persistent keys, advance the ledger past the bump threshold, and
// assert that the entries are still reachable (i.e. the helpers bumped TTL).

use crate::ttl::{
    extend_accumulated_fees_ttl, extend_admin_ttl, extend_finalization_ttl,
    extend_pending_reputation_credits_ttl, extend_protocol_fee_bps_ttl, extend_reputation_ttl,
    PERSISTENT_BUMP_THRESHOLD, PERSISTENT_TTL_LEDGERS,
};
use crate::{EscrowClient, ReleaseAuthorization};

/// After `initialize` the `Admin` key must survive past `PERSISTENT_BUMP_THRESHOLD` ledgers.
#[test]
fn admin_key_survives_past_bump_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.sequence_number = 100;
        li.max_entry_ttl = PERSISTENT_TTL_LEDGERS * 4;
        li.min_persistent_entry_ttl = PERSISTENT_TTL_LEDGERS * 4;
    });
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = soroban_sdk::Address::generate(&env);
    client.initialize(&admin);

    // Advance past the bump threshold
    env.ledger().with_mut(|li| {
        li.sequence_number = li.sequence_number + PERSISTENT_BUMP_THRESHOLD + 1;
    });

    // get_admin bumps TTL; it must return Some(admin)
    let fetched = client.get_admin();
    assert_eq!(fetched, Some(admin), "Admin key must survive past bump threshold");
}

/// After `set_protocol_fee_bps` the `ProtocolFeeBps` key survives long inactivity.
#[test]
fn protocol_fee_bps_key_survives_after_write() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.sequence_number = 100;
        li.max_entry_ttl = PERSISTENT_TTL_LEDGERS * 4;
        li.min_persistent_entry_ttl = PERSISTENT_TTL_LEDGERS * 4;
    });
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = soroban_sdk::Address::generate(&env);
    client.initialize(&admin);
    client.set_protocol_fee_bps(&50u32);

    // Advance past bump threshold
    env.ledger().with_mut(|li| {
        li.sequence_number = li.sequence_number + PERSISTENT_BUMP_THRESHOLD + 1;
    });

    // Key must still exist — call set again (also bumps TTL) and assert success
    assert!(client.set_protocol_fee_bps(&75u32));
}

/// `extend_admin_ttl` is a no-op when the key is absent (must not panic).
#[test]
fn extend_admin_ttl_absent_key_is_noop() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    env.as_contract(&contract_id, || {
        // No initialize called — key is absent; should not panic
        extend_admin_ttl(&env);
    });
}

/// `extend_accumulated_fees_ttl` is a no-op when the key is absent.
#[test]
fn extend_accumulated_fees_ttl_absent_key_is_noop() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    env.as_contract(&contract_id, || {
        extend_accumulated_fees_ttl(&env);
    });
}

/// `extend_finalization_ttl` is a no-op when the finalization key is absent.
#[test]
fn extend_finalization_ttl_absent_key_is_noop() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    env.as_contract(&contract_id, || {
        extend_finalization_ttl(&env, 9999);
    });
}

/// `extend_reputation_ttl` is a no-op when the reputation key is absent.
#[test]
fn extend_reputation_ttl_absent_key_is_noop() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let addr = soroban_sdk::Address::generate(&env);
    env.as_contract(&contract_id, || {
        extend_reputation_ttl(&env, &addr);
    });
}

/// `extend_pending_reputation_credits_ttl` is a no-op when absent.
#[test]
fn extend_pending_reputation_credits_ttl_absent_is_noop() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let addr = soroban_sdk::Address::generate(&env);
    env.as_contract(&contract_id, || {
        extend_pending_reputation_credits_ttl(&env, &addr);
    });
}

/// `extend_protocol_fee_bps_ttl` is a no-op when the key is absent.
#[test]
fn extend_protocol_fee_bps_ttl_absent_key_is_noop() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    env.as_contract(&contract_id, || {
        extend_protocol_fee_bps_ttl(&env);
    });
}

/// After `finalize_contract` the finalization record survives past bump threshold.
#[test]
fn finalization_record_survives_past_bump_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.sequence_number = 100;
        li.max_entry_ttl = PERSISTENT_TTL_LEDGERS * 4;
        li.min_persistent_entry_ttl = PERSISTENT_TTL_LEDGERS * 4;
    });
    let contract_id_addr = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id_addr);
    let admin = soroban_sdk::Address::generate(&env);
    client.initialize(&admin);

    let client_addr = soroban_sdk::Address::generate(&env);
    let freelancer_addr = soroban_sdk::Address::generate(&env);
    let cid = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &soroban_sdk::vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&cid, &client_addr, &100_i128);
    client.approve_milestone_release(&cid, &client_addr, &0);
    client.release_milestone(&cid, &client_addr, &0);
    client.finalize_contract(&cid, &client_addr);

    // Advance past bump threshold
    env.ledger().with_mut(|li| {
        li.sequence_number = li.sequence_number + PERSISTENT_BUMP_THRESHOLD + 1;
    });

    // get_finalization_record bumps TTL; record must still be present
    let record = client.get_finalization_record(&cid);
    assert!(record.is_some(), "Finalization record must survive past bump threshold");
}