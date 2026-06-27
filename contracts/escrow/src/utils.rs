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
/// # Returns
/// The current ledger timestamp as a `u64` representing seconds since Unix epoch
///
/// # Example
/// ```ignore
/// use crate::utils::now_seconds;
///
/// pub fn check_timeout(env: &Env, deadline: u64) -> bool {
///     now_seconds(env) > deadline
/// }
/// ```
///
/// # Testing
/// In tests, use `env.ledger().set()` to control time:
/// ```ignore
/// use soroban_sdk::testutils::Ledger;
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
