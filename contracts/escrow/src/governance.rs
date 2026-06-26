use crate::{DataKey, Escrow, EscrowArgs, EscrowClient};
use soroban_sdk::{contractimpl, symbol_short, Address, Env, Symbol};

/// Governance-related privileged operations and audit events.
///
/// This module implements admin-facing functions for protocol fee management
/// and two-step admin transfer. All entrypoints require the contract to be
/// initialized and enforce caller authentication via `require_auth`.
///
/// Events follow the convention of short `symbol_short!` topics used by
/// other lifecycle events (e.g. `init`, `paused`, `emergency`).
#[contractimpl]
impl Escrow {
    /// Return the currently pending admin, if any.
    pub fn get_pending_governance_admin(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::PendingAdmin)
    }

    /// Return the current admin address.
    pub fn get_governance_admin(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Admin)
    }
    /// Set the protocol fee (basis points). Emits an event with
    /// `(old_bps, new_bps, admin, timestamp)` under topic `protocol_fee_bps`.
    ///
    /// # Requirements
    /// - Contract must be initialized.
    /// - Caller must be the stored admin.
    ///
    /// # Events
    /// - `("protocol_fee_bps",)` with data `(old_bps, new_bps, admin, timestamp)`
    pub fn set_protocol_fee_bps(env: Env, new_bps: u32) -> bool {
        if !env
            .storage()
            .persistent()
            .get::<_, bool>(&crate::DataKey::Initialized)
            .unwrap_or(false)
        {
            env.panic_with_error(crate::Error::NotInitialized);
        }

        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(crate::Error::NotInitialized));
        admin.require_auth();

        let old_bps: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ProtocolFeeBps)
            .unwrap_or(0u32);
        env.storage()
            .persistent()
            .set(&DataKey::ProtocolFeeBps, &new_bps);

        env.events().publish(
            (Symbol::new(&env, "protocol_fee_bps"),),
            (old_bps, new_bps, admin.clone(), env.ledger().timestamp()),
        );
        true
    }

    /// Propose a new governance admin.
    ///
    /// Stores the `proposed` address as the pending admin and emits an event.
    /// If a pending proposal already exists, it is silently overwritten
    /// (re-proposing is allowed without explicit cancellation).
    ///
    /// # Requirements
    /// - Contract must be initialized.
    /// - Caller must be the stored admin.
    /// - `proposed` must differ from the current admin.
    ///
    /// # Events
    /// - `(symbol_short!("admin"), "proposed")` with data `(admin, proposed, timestamp)`
    ///
    /// # Errors
    /// - `NotInitialized` if the contract has not been initialized.
    /// - `CannotProposeSelf` if `proposed` equals the current admin.
    pub fn propose_governance_admin(env: Env, proposed: Address) -> bool {
        if !env
            .storage()
            .persistent()
            .get::<_, bool>(&crate::DataKey::Initialized)
            .unwrap_or(false)
        {
            env.panic_with_error(crate::Error::NotInitialized);
        }

        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(crate::Error::NotInitialized));
        admin.require_auth();

        if proposed == admin {
            env.panic_with_error(crate::Error::CannotProposeSelf);
        }

        env.storage()
            .persistent()
            .set(&DataKey::PendingAdmin, &proposed);

        env.events().publish(
            (symbol_short!("admin"), Symbol::new(&env, "proposed")),
            (admin, proposed.clone(), env.ledger().timestamp()),
        );
        true
    }

    /// Accept a pending admin proposal and finalise the transfer.
    ///
    /// The caller must be the proposed admin. Emits an event and clears the
    /// pending admin from storage.
    ///
    /// # Requirements
    /// - Contract must be initialized.
    /// - A pending admin proposal must exist.
    /// - Caller must be the proposed address.
    ///
    /// # Events
    /// - `(symbol_short!("admin"), "accepted")` with data `(old_admin, new_admin, timestamp)`
    ///
    /// # Errors
    /// - `NotInitialized` if the contract has not been initialized.
    /// - `InvalidState` if no pending proposal exists.
    pub fn accept_governance_admin(env: Env) -> bool {
        if !env
            .storage()
            .persistent()
            .get::<_, bool>(&crate::DataKey::Initialized)
            .unwrap_or(false)
        {
            env.panic_with_error(crate::Error::NotInitialized);
        }

        let pending: Option<Address> = env.storage().persistent().get(&DataKey::PendingAdmin);
        if pending.is_none() {
            env.panic_with_error(crate::Error::InvalidState);
        }
        let pending_admin = pending.unwrap();

        pending_admin.require_auth();

        let old_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(crate::Error::NotInitialized));

        env.storage()
            .persistent()
            .set(&DataKey::Admin, &pending_admin);
        env.storage().persistent().remove(&DataKey::PendingAdmin);

        env.events().publish(
            (symbol_short!("admin"), Symbol::new(&env, "accepted")),
            (old_admin, pending_admin.clone(), env.ledger().timestamp()),
        );
        true
    }

    /// Cancel a pending governance admin proposal.
    ///
    /// Clears `DataKey::PendingAdmin` and emits an event. This is an
    /// admin-gated operation: only the current admin may cancel a proposal.
    /// If no proposal is pending, the call panics with `NoPendingAdminProposal`.
    ///
    /// # Requirements
    /// - Contract must be initialized.
    /// - A pending admin proposal must exist.
    /// - Caller must be the stored admin.
    ///
    /// # Events
    /// - `(symbol_short!("admin"), "cancelled")` with data `(admin, cancelled_proposal, timestamp)`
    ///
    /// # Errors
    /// - `NotInitialized` if the contract has not been initialized.
    /// - `NoPendingAdminProposal` if there is no pending proposal to cancel.
    pub fn cancel_governance_admin_proposal(env: Env) -> bool {
        if !env
            .storage()
            .persistent()
            .get::<_, bool>(&crate::DataKey::Initialized)
            .unwrap_or(false)
        {
            env.panic_with_error(crate::Error::NotInitialized);
        }

        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(crate::Error::NotInitialized));
        admin.require_auth();

        let pending: Option<Address> = env.storage().persistent().get(&DataKey::PendingAdmin);
        let cancelled = pending.unwrap_or_else(|| {
            env.panic_with_error(crate::Error::NoPendingAdminProposal);
        });

        env.storage().persistent().remove(&DataKey::PendingAdmin);

        env.events().publish(
            (symbol_short!("admin"), Symbol::new(&env, "cancelled")),
            (admin, cancelled.clone(), env.ledger().timestamp()),
        );
        true
    }
}
