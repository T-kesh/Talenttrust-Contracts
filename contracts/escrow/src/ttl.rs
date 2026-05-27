//! Deterministic TTL / expiration policy for transient storage.
//!
//! All TTL values are denominated in ledgers (Soroban-native, ~5 s per ledger
//! on Stellar mainnet). Pending approvals and pending migrations are stored in
//! `env.storage().temporary()`; Soroban auto-evicts entries whose TTL has
//! elapsed, so [`read_if_live`] returns `None` for both "never set" and
//! "expired".

use soroban_sdk::{Env, IntoVal, TryFromVal, Val};

/// Approximate number of ledgers produced per day on Stellar mainnet (~5 s/ledger).
pub const LEDGERS_PER_DAY: u32 = 17_280;

/// TTL for a pending-approval entry: 7 days.
pub const PENDING_APPROVAL_TTL_LEDGERS: u32 = LEDGERS_PER_DAY * 7;

/// Bump threshold for pending-approval entries: extend when remaining TTL
/// falls below 1 day.
pub const PENDING_APPROVAL_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY;

/// TTL for a pending-migration entry: 21 days.
pub const PENDING_MIGRATION_TTL_LEDGERS: u32 = LEDGERS_PER_DAY * 21;

/// Bump threshold for pending-migration entries: extend when remaining TTL
/// falls below 3 days.
pub const PENDING_MIGRATION_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY * 3;

/// Returns the ledger sequence number at which an entry stored *now* will expire.
///
/// Uses saturating addition so the result never wraps on a pathological ledger
/// sequence.
pub fn compute_expiry(env: &Env, ttl_ledgers: u32) -> u32 {
    env.ledger().sequence().saturating_add(ttl_ledgers)
}

/// Write `value` under `key` in temporary storage and set its TTL to
/// `ttl_ledgers` ledgers from the current sequence.
///
/// Soroban will auto-evict the entry once the TTL elapses; callers must not
/// rely on explicit deletion for security-sensitive cleanup.
pub fn store_with_ttl<K, V>(env: &Env, key: &K, value: &V, ttl_ledgers: u32)
where
    K: IntoVal<Env, Val>,
    V: IntoVal<Env, Val>,
{
    let storage = env.storage().temporary();
    storage.set(key, value);
    storage.extend_ttl(key, ttl_ledgers, ttl_ledgers);
}

/// Read a value from temporary storage, returning `None` if the entry has
/// been evicted (TTL elapsed) or was never written.
///
/// This is the primary read path for all transient approval and migration
/// entries; callers treat `None` as "expired or absent" without distinction.
pub fn read_if_live<K, V>(env: &Env, key: &K) -> Option<V>
where
    K: IntoVal<Env, Val>,
    V: TryFromVal<Env, Val>,
{
    env.storage().temporary().get(key)
}

/// Extend the TTL of a live temporary entry when its remaining TTL has dropped
/// below `threshold` ledgers, bumping it back up to `extend_to` ledgers.
///
/// Returns `true` if the entry exists (and the bump was applied), `false` if
/// the key is absent (already evicted or never stored).  A `false` return is
/// not an error; callers may use it to detect expired approvals.
pub fn extend_if_below_threshold<K>(env: &Env, key: &K, threshold: u32, extend_to: u32) -> bool
where
    K: IntoVal<Env, Val>,
{
    let storage = env.storage().temporary();
    if !storage.has(key) {
        return false;
    }
    storage.extend_ttl(key, threshold, extend_to);
    true
}

/// Remove a transient entry immediately, regardless of its remaining TTL.
///
/// Used for explicit cleanup (e.g. after an approval is consumed or cancelled)
/// so that stale entries do not linger until auto-eviction.
pub fn remove_transient<K>(env: &Env, key: &K)
where
    K: IntoVal<Env, Val>,
{
    env.storage().temporary().remove(key);
}

/// Returns `true` if a transient entry for `key` is currently live in
/// temporary storage (i.e. has not been evicted and was previously written).
pub fn has_transient<K>(env: &Env, key: &K) -> bool
where
    K: IntoVal<Env, Val>,
{
    env.storage().temporary().has(key)
}
