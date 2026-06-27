use soroban_sdk::Env;

/// Returns the current ledger timestamp in seconds.
///
/// This is the single source of truth for all time-related operations in the escrow contract.
/// All production code must use this function rather than calling `env.ledger().timestamp()`
/// directly, so that time can be controlled deterministically in tests via
/// `env.ledger().set_timestamp()`.
///
/// CRITICAL: Every place in lib.rs that needs the current time must call this function.
/// Direct calls to `env.ledger().timestamp()` bypass this abstraction and make it impossible
/// to test timeout-driven refunds reliably.
///
/// # Security
/// Ledger time on Soroban is set by validators and cannot be manipulated by contract callers.
/// A milestone deadline cannot be artificially triggered by any on-chain actor.
/// The timestamp is in Unix seconds, matching the Soroban ledger's native representation.
///
/// # Testing
/// In tests, control time using:
/// ```ignore
/// env.ledger().with_mut(|li| {
///     li.timestamp = target_time_in_seconds;
/// });
/// ```
///
/// # Example
/// ```ignore
/// use crate::utils::now_seconds;
///
/// let current_time = now_seconds(&env);
/// let is_overdue = current_time > milestone_deadline;
/// ```
pub fn now_seconds(env: &Env) -> u64 {
    env.ledger().timestamp()
}
