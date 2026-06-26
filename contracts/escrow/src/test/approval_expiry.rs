//! Approval expiry and TTL eviction coverage.
//!
//! `approve_milestone_release` records approvals in *temporary* storage with a
//! lifetime of [`PENDING_APPROVAL_TTL_LEDGERS`] ledgers. Soroban auto-evicts
//! temporary entries whose TTL has elapsed, so once the window passes
//! `get_milestone_approvals` returns `None` and `release_milestone` must fail
//! closed with `InsufficientApprovals`.
//!
//! These tests advance the ledger sequence number deterministically across the
//! TTL boundary and assert that expired approvals never authorize a release.
//! See `docs/escrow/state-persistence.md` for the documented semantics.

use super::{assert_contract_error, MILESTONE_ONE, MILESTONE_THREE, MILESTONE_TWO};
use crate::ttl::{LEDGERS_PER_DAY, PENDING_APPROVAL_BUMP_THRESHOLD, PENDING_APPROVAL_TTL_LEDGERS};
use crate::{DataKey, Error, Escrow, EscrowClient, ReleaseAuthorization};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    vec, Address, Env,
};

// ── Fixtures ─────────────────────────────────────────────────────────────────

/// Build an `Env` whose ledger tolerates the multi-day TTLs used here.
///
/// Persistent entries (the contract + its milestones) are pinned to a 60-day
/// floor so they always outlive the 7-day temporary approval window — this
/// isolates the variable under test: only the *approval* expires, never the
/// contract itself, so a post-expiry release fails with `InsufficientApprovals`
/// rather than `ContractNotFound`. The temporary floor is kept tiny so the
/// approval's lifetime is governed purely by the contract's explicit
/// `extend_ttl(.., PENDING_APPROVAL_TTL_LEDGERS)` call.
fn setup_ttl_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.max_entry_ttl = LEDGERS_PER_DAY * 90;
        li.min_persistent_entry_ttl = LEDGERS_PER_DAY * 60;
        li.min_temp_entry_ttl = 16;
        li.sequence_number = 1_000;
    });
    env
}

/// Register + initialize an escrow, then create and fully fund one contract
/// with the requested authorization mode.
///
/// Returns the bound client, the participant addresses, and the contract id.
/// The contract is left in `Funded` state, ready for approval/release.
fn funded_contract<'a>(
    env: &'a Env,
    auth: ReleaseAuthorization,
    arbiter: &Option<Address>,
) -> (EscrowClient<'a>, Address, Address, u32) {
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &id);
    let admin = Address::generate(env);
    client.initialize(&admin);

    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = vec![env, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE];
    let total = MILESTONE_ONE + MILESTONE_TWO + MILESTONE_THREE;

    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, arbiter, &milestones, &auth);
    assert!(client.deposit_funds(&contract_id, &client_addr, &total));

    (client, client_addr, freelancer_addr, contract_id)
}

/// Read the remaining TTL (in ledgers) of a milestone-approval entry directly
/// from temporary storage. Returns `None` if the entry has been evicted.
fn approval_ttl(env: &Env, client: &EscrowClient, contract_id: u32, milestone: u32) -> Option<u32> {
    env.as_contract(&client.address, || {
        let key = DataKey::MilestoneApprovals(contract_id, milestone);
        if env.storage().temporary().has(&key) {
            Some(env.storage().temporary().get_ttl(&key))
        } else {
            None
        }
    })
}

/// Advance the ledger sequence number by `ledgers`, simulating elapsed time.
fn advance_ledgers(env: &Env, ledgers: u32) {
    env.ledger().with_mut(|li| {
        li.sequence_number = li.sequence_number.saturating_add(ledgers);
    });
}

// ── TTL constant sanity ──────────────────────────────────────────────────────

/// The bump threshold must sit strictly inside the full lifetime, otherwise a
/// freshly-written approval could never be "below threshold" and activity-based
/// TTL extension would be unreachable.
#[test]
fn bump_threshold_is_below_full_ttl() {
    assert!(
        PENDING_APPROVAL_BUMP_THRESHOLD < PENDING_APPROVAL_TTL_LEDGERS,
        "bump threshold must be strictly less than the full approval TTL",
    );
}

// ── Baseline eviction ────────────────────────────────────────────────────────

/// A recorded approval is written with exactly `PENDING_APPROVAL_TTL_LEDGERS`
/// of life, and is evicted once the ledger advances one past that window:
/// `get_milestone_approvals` flips from `Some(..)` to `None`.
#[test]
fn approval_evicted_after_ttl_window() {
    let env = setup_ttl_env();
    let (client, client_addr, _freelancer, id) =
        funded_contract(&env, ReleaseAuthorization::ClientOnly, &None);

    assert!(client.approve_milestone_release(&id, &client_addr, &0));

    // Freshly recorded: present, client-approved, and stamped with the full TTL.
    let approvals = client.get_milestone_approvals(&id, &0);
    assert!(approvals.is_some());
    assert!(approvals.unwrap().client_approved);
    assert_eq!(
        approval_ttl(&env, &client, id, 0),
        Some(PENDING_APPROVAL_TTL_LEDGERS),
        "approval should be stamped with the full pending-approval TTL",
    );

    // One ledger past the window: Soroban evicts the temporary entry.
    advance_ledgers(&env, PENDING_APPROVAL_TTL_LEDGERS + 1);
    assert!(
        client.get_milestone_approvals(&id, &0).is_none(),
        "approval must be evicted once its TTL has fully elapsed",
    );
}

/// Security invariant: an expired approval never authorizes a release. After
/// the TTL elapses, `release_milestone` fails closed with
/// `InsufficientApprovals` and the milestone stays unreleased.
#[test]
fn release_fails_closed_after_approval_expiry() {
    let env = setup_ttl_env();
    let (client, client_addr, _freelancer, id) =
        funded_contract(&env, ReleaseAuthorization::ClientOnly, &None);

    assert!(client.approve_milestone_release(&id, &client_addr, &0));
    advance_ledgers(&env, PENDING_APPROVAL_TTL_LEDGERS + 1);

    let result = client.try_release_milestone(&id, &client_addr, &0);
    assert_contract_error(result, Error::InsufficientApprovals);

    // Funds untouched: the milestone is still unreleased.
    assert!(!client.get_milestones(&id).get(0).unwrap().released);
}

// ── Exact boundary behavior ──────────────────────────────────────────────────

/// Exact-boundary expiry (inclusive end): at the final live ledger
/// (`approval_ledger + PENDING_APPROVAL_TTL_LEDGERS`) the approval is still
/// present and a release succeeds.
#[test]
fn approval_valid_at_exact_ttl_boundary() {
    let env = setup_ttl_env();
    let (client, client_addr, _freelancer, id) =
        funded_contract(&env, ReleaseAuthorization::ClientOnly, &None);

    assert!(client.approve_milestone_release(&id, &client_addr, &0));

    // Advance to the last ledger at which the entry is still live.
    advance_ledgers(&env, PENDING_APPROVAL_TTL_LEDGERS);
    assert!(
        client.get_milestone_approvals(&id, &0).is_some(),
        "approval must remain live at the exact TTL boundary",
    );
    assert!(client.release_milestone(&id, &client_addr, &0));
    assert!(client.get_milestones(&id).get(0).unwrap().released);
}

/// Exact-boundary expiry (one past the end): a single ledger beyond the window
/// evicts the approval and the release fails closed. Pairs with
/// [`approval_valid_at_exact_ttl_boundary`] to pin the off-by-one edge.
#[test]
fn approval_invalid_one_ledger_past_ttl_boundary() {
    let env = setup_ttl_env();
    let (client, client_addr, _freelancer, id) =
        funded_contract(&env, ReleaseAuthorization::ClientOnly, &None);

    assert!(client.approve_milestone_release(&id, &client_addr, &0));

    advance_ledgers(&env, PENDING_APPROVAL_TTL_LEDGERS + 1);
    assert!(client.get_milestone_approvals(&id, &0).is_none());

    let result = client.try_release_milestone(&id, &client_addr, &0);
    assert_contract_error(result, Error::InsufficientApprovals);
}

// ── Re-approval after expiry ─────────────────────────────────────────────────

/// Re-approval after expiry succeeds: once an approval is evicted, the caller
/// may approve again, which writes a fresh entry with a full TTL and re-enables
/// release.
#[test]
fn reapproval_after_expiry_restores_release() {
    let env = setup_ttl_env();
    let (client, client_addr, _freelancer, id) =
        funded_contract(&env, ReleaseAuthorization::ClientOnly, &None);

    // First approval expires.
    assert!(client.approve_milestone_release(&id, &client_addr, &0));
    advance_ledgers(&env, PENDING_APPROVAL_TTL_LEDGERS + 1);
    assert!(client.get_milestone_approvals(&id, &0).is_none());

    // Release is blocked at this point.
    assert_contract_error(
        client.try_release_milestone(&id, &client_addr, &0),
        Error::InsufficientApprovals,
    );

    // Re-approve: a brand-new entry with the full TTL.
    assert!(client.approve_milestone_release(&id, &client_addr, &0));
    assert!(client.get_milestone_approvals(&id, &0).is_some());
    assert_eq!(
        approval_ttl(&env, &client, id, 0),
        Some(PENDING_APPROVAL_TTL_LEDGERS),
        "re-approval must reset the TTL to the full window",
    );

    // Release now succeeds.
    assert!(client.release_milestone(&id, &client_addr, &0));
    assert!(client.get_milestones(&id).get(0).unwrap().released);
}

// ── Activity-driven TTL extension (bump threshold path) ──────────────────────

/// Bump-threshold path: a second approval recorded while the entry sits *below*
/// `PENDING_APPROVAL_BUMP_THRESHOLD` of expiry extends the lifetime back to the
/// full window. The record then survives past where the original approval would
/// have expired, and a MultiSig release succeeds.
#[test]
fn activity_below_bump_threshold_extends_ttl() {
    let env = setup_ttl_env();
    let (client, client_addr, freelancer_addr, id) =
        funded_contract(&env, ReleaseAuthorization::MultiSig, &None);

    // Client approves; entry stamped with the full TTL.
    assert!(client.approve_milestone_release(&id, &client_addr, &0));

    // Advance until the remaining TTL drops below the bump threshold (but the
    // entry is still live).
    let into_window = PENDING_APPROVAL_TTL_LEDGERS - PENDING_APPROVAL_BUMP_THRESHOLD + 1;
    advance_ledgers(&env, into_window);
    let remaining = approval_ttl(&env, &client, id, 0).expect("entry still live");
    assert!(
        remaining < PENDING_APPROVAL_BUMP_THRESHOLD,
        "precondition: entry must sit below the bump threshold (remaining = {remaining})",
    );

    // Freelancer approves: this activity bumps the TTL back to the full window.
    assert!(client.approve_milestone_release(&id, &freelancer_addr, &0));
    assert_eq!(
        approval_ttl(&env, &client, id, 0),
        Some(PENDING_APPROVAL_TTL_LEDGERS),
        "activity below the bump threshold must extend TTL to the full window",
    );

    // Advance past where the ORIGINAL approval would have expired. Because the
    // bump extended the lifetime, the record (with both approvals) survives.
    advance_ledgers(&env, PENDING_APPROVAL_BUMP_THRESHOLD);
    let approvals = client
        .get_milestone_approvals(&id, &0)
        .expect("record survived the bump");
    assert!(approvals.client_approved && approvals.freelancer_approved);

    assert!(client.release_milestone(&id, &client_addr, &0));
    assert!(client.get_milestones(&id).get(0).unwrap().released);
}

// ── Independent expiry across milestones ─────────────────────────────────────

/// Multiple milestones expire independently: each approval carries its own TTL
/// anchored to its own approval ledger. An older milestone's approval can
/// expire while a newer one is still live, so release is rejected for the
/// expired milestone yet succeeds for the live one.
#[test]
fn milestones_expire_independently() {
    let env = setup_ttl_env();
    let (client, client_addr, _freelancer, id) =
        funded_contract(&env, ReleaseAuthorization::ClientOnly, &None);

    // Approve milestone 0 now.
    assert!(client.approve_milestone_release(&id, &client_addr, &0));

    // Advance halfway through the window, then approve milestone 1. Its TTL is
    // anchored later, so it outlives milestone 0's.
    let half = PENDING_APPROVAL_TTL_LEDGERS / 2;
    advance_ledgers(&env, half);
    assert!(client.approve_milestone_release(&id, &client_addr, &1));

    // Advance just past milestone 0's expiry. Milestone 1 is still live.
    advance_ledgers(&env, half + 1);
    assert!(
        client.get_milestone_approvals(&id, &0).is_none(),
        "milestone 0's approval should have expired",
    );
    assert!(
        client.get_milestone_approvals(&id, &1).is_some(),
        "milestone 1's approval should still be live",
    );

    // Release is rejected for the expired milestone, accepted for the live one.
    assert_contract_error(
        client.try_release_milestone(&id, &client_addr, &0),
        Error::InsufficientApprovals,
    );
    assert!(client.release_milestone(&id, &client_addr, &1));

    let milestones = client.get_milestones(&id);
    assert!(!milestones.get(0).unwrap().released);
    assert!(milestones.get(1).unwrap().released);
}

// ── MultiSig: expired partial approval is not resurrected ────────────────────

/// Security invariant for MultiSig: an expired partial approval cannot be
/// silently completed by a later co-signer. After the client's approval is
/// evicted, a freelancer approval starts a *fresh* record holding only the
/// freelancer's signature — release stays blocked until the client re-approves
/// inside the new window.
#[test]
fn multisig_expired_partial_approval_not_resurrected() {
    let env = setup_ttl_env();
    let (client, client_addr, freelancer_addr, id) =
        funded_contract(&env, ReleaseAuthorization::MultiSig, &None);

    // Client approves, then the approval expires.
    assert!(client.approve_milestone_release(&id, &client_addr, &0));
    advance_ledgers(&env, PENDING_APPROVAL_TTL_LEDGERS + 1);
    assert!(client.get_milestone_approvals(&id, &0).is_none());

    // Freelancer approves into a fresh record: the client's expired signature
    // is NOT carried over.
    assert!(client.approve_milestone_release(&id, &freelancer_addr, &0));
    let approvals = client
        .get_milestone_approvals(&id, &0)
        .expect("fresh record exists");
    assert!(
        !approvals.client_approved && approvals.freelancer_approved,
        "expired client approval must not be resurrected by a later co-signer",
    );

    // Release stays blocked: only one of two required signatures is live.
    assert_contract_error(
        client.try_release_milestone(&id, &client_addr, &0),
        Error::InsufficientApprovals,
    );

    // Client re-approves within the live window -> both signatures present.
    assert!(client.approve_milestone_release(&id, &client_addr, &0));
    assert!(client.release_milestone(&id, &client_addr, &0));
    assert!(client.get_milestones(&id).get(0).unwrap().released);
}

// ── Arbiter mode under expiry ────────────────────────────────────────────────

/// The fail-closed guarantee holds across authorization modes: an `ArbiterOnly`
/// approval that expires no longer authorizes release, and a re-approval by the
/// arbiter restores it.
#[test]
fn arbiter_only_approval_expiry_and_reapproval() {
    let env = setup_ttl_env();
    let arbiter = Address::generate(&env);
    let (client, client_addr, _freelancer, id) = funded_contract(
        &env,
        ReleaseAuthorization::ArbiterOnly,
        &Some(arbiter.clone()),
    );

    assert!(client.approve_milestone_release(&id, &arbiter, &0));
    advance_ledgers(&env, PENDING_APPROVAL_TTL_LEDGERS + 1);
    assert!(client.get_milestone_approvals(&id, &0).is_none());

    // Client cannot release on an expired arbiter approval.
    assert_contract_error(
        client.try_release_milestone(&id, &client_addr, &0),
        Error::InsufficientApprovals,
    );

    // Arbiter re-approves -> release authorized again.
    assert!(client.approve_milestone_release(&id, &arbiter, &0));
    assert!(client.release_milestone(&id, &client_addr, &0));
    assert!(client.get_milestones(&id).get(0).unwrap().released);
}
