//! Deterministic TTL / expiration policy for transient and persistent storage.
//!
//! All TTL values are denominated in ledgers (Soroban-native, ~5s per ledger
//! on Stellar mainnet). Pending approvals and pending migrations are stored
//! in `env.storage().temporary()`; Soroban auto-evicts entries whose TTL has
//! elapsed, so `read_if_live` returns `None` for both "never set" and
//! "expired".

use soroban_sdk::{Env, IntoVal, Symbol, TryFromVal, Val};

use crate::DataKey;

pub const LEDGERS_PER_DAY: u32 = 17_280;

pub const PENDING_APPROVAL_TTL_LEDGERS: u32 = LEDGERS_PER_DAY * 7;
pub const PENDING_APPROVAL_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY;

#[allow(dead_code)]
pub const PENDING_MIGRATION_TTL_LEDGERS: u32 = LEDGERS_PER_DAY * 21;
#[allow(dead_code)]
pub const PENDING_MIGRATION_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY * 3;

/// Persistent-storage TTL constants (contract data lives for ~90 days).
pub const CONTRACT_TTL_LEDGERS: u32 = LEDGERS_PER_DAY * 90;
pub const CONTRACT_TTL_THRESHOLD: u32 = LEDGERS_PER_DAY * 30;
pub const MILESTONE_TTL_LEDGERS: u32 = LEDGERS_PER_DAY * 90;
pub const MILESTONE_TTL_THRESHOLD: u32 = LEDGERS_PER_DAY * 30;
pub const NEXT_CONTRACT_ID_TTL_LEDGERS: u32 = LEDGERS_PER_DAY * 90;
pub const NEXT_CONTRACT_ID_TTL_THRESHOLD: u32 = LEDGERS_PER_DAY * 30;

/// Extend TTL for the contract stored at `contract_id`.
pub fn extend_contract_ttl(env: &Env, contract_id: u32) {
    let key = DataKey::Contract(contract_id);
    env.storage()
        .persistent()
        .extend_ttl(&key, CONTRACT_TTL_THRESHOLD, CONTRACT_TTL_LEDGERS);
}

/// Extend TTL for the milestone vector stored under contract `contract_id`.
pub fn extend_milestone_ttl(env: &Env, contract_id: u32) {
    let key = Symbol::new(env, "milestones");
    let compound = (DataKey::Contract(contract_id), key);
    env.storage().persistent().extend_ttl(
        &compound,
        MILESTONE_TTL_THRESHOLD,
        MILESTONE_TTL_LEDGERS,
    );
}

/// Extend TTL for both the contract and its milestones in one call.
pub fn extend_contract_and_milestones_ttl(env: &Env, contract_id: u32) {
    extend_contract_ttl(env, contract_id);
    extend_milestone_ttl(env, contract_id);
}

/// Extend TTL for the `NextContractId` counter.
pub fn extend_next_contract_id_ttl(env: &Env) {
    let key = DataKey::NextContractId;
    env.storage().persistent().extend_ttl(
        &key,
        NEXT_CONTRACT_ID_TTL_THRESHOLD,
        NEXT_CONTRACT_ID_TTL_LEDGERS,
    );
}

#[allow(dead_code)]
pub fn compute_expiry(env: &Env, ttl_ledgers: u32) -> u32 {
    env.ledger().sequence().saturating_add(ttl_ledgers)
}

#[allow(dead_code)]
pub fn store_with_ttl<K, V>(env: &Env, key: &K, value: &V, ttl_ledgers: u32)
where
    K: IntoVal<Env, Val>,
    V: IntoVal<Env, Val>,
{
    let storage = env.storage().temporary();
    storage.set(key, value);
    storage.extend_ttl(key, ttl_ledgers, ttl_ledgers);
}

#[allow(dead_code)]
pub fn read_if_live<K, V>(env: &Env, key: &K) -> Option<V>
where
    K: IntoVal<Env, Val>,
    V: TryFromVal<Env, Val>,
{
    env.storage().temporary().get(key)
}

#[allow(dead_code)]
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

#[allow(dead_code)]
pub fn remove_transient<K>(env: &Env, key: &K)
where
    K: IntoVal<Env, Val>,
{
    env.storage().temporary().remove(key);
}

#[allow(dead_code)]
pub fn has_transient<K>(env: &Env, key: &K) -> bool
where
    K: IntoVal<Env, Val>,
{
    env.storage().temporary().has(key)
}
