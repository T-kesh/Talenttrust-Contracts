# Escrow Storage TTL

This document defines the deterministic, auditable TTL (time-to-live) policy for
**transient** storage entries in the escrow contract. It exists to prevent
unbounded state growth from orphaned pending approvals and pending migrations
that are never resolved by counterparties.

See also: [state-persistence.md](./state-persistence.md) for the **full storage
map** including persistent keys, value types, and bump-on-access policy;
[upgradeable-storage.md](./upgradeable-storage.md) for upgrade semantics.

## Scope

Applies to keys stored in `env.storage().temporary()`. Persistent keys are
catalogued in [state-persistence.md](./state-persistence.md) — their TTL policy
is summarised below and detailed per-key in the reference table.

## Units

All TTL values are denominated in **ledgers**, the Soroban-native unit. One
ledger is ~5 seconds on Stellar mainnet. This avoids any coupling to
wall-clock timestamps and keeps expiry deterministic as a function of
`env.ledger().sequence()`.

| Named constant | Ledgers | Rough duration | Scope |
|---|---:|---:|---|
| `LEDGERS_PER_DAY` | 17 280 | 1 day | — |
| `PERSISTENT_TTL_LEDGERS` | 518 400 | 30 days | Persistent keys |
| `PERSISTENT_BUMP_THRESHOLD` | 120 960 | 7 days | Persistent keys |
| `PENDING_APPROVAL_TTL_LEDGERS` | 120 960 | 7 days | Temporary (`MilestoneApprovals`) |
| `PENDING_APPROVAL_BUMP_THRESHOLD` | 17 280 | 1 day | Temporary (`MilestoneApprovals`) |
| `PENDING_MIGRATION_TTL_LEDGERS` | 362 880 | 21 days | Temporary (`PendingClientMigration`) |
| `PENDING_MIGRATION_BUMP_THRESHOLD` | 51 840 | 3 days | Temporary (`PendingClientMigration`) |

Constants live in
[contracts/escrow/src/ttl.rs](../../contracts/escrow/src/ttl.rs).

## Transient Keys

| DataKey | TTL | Bump threshold | Bumped on read? | Rationale |
|---|---|---|---:|---|
| `MilestoneApprovals(u32, u32)` | 7 days | 1 day | No (set on write only) | Counterparties expected to respond within one business week; short TTL reclaims state on abandonment. |
| `PendingClientMigration(u32)` | 21 days | 3 days | No (set once) | Migrations are rarer and more consequential; reviewers need more lead time. |

> **Note:** `MilestoneApprovals` TTL is managed **manually** via
> `env.storage().temporary().extend_ttl()` — it does **not** go through
> `ttl::store_with_ttl`. All other transient helpers in `ttl.rs` apply only to
> `PendingClientMigration`. See [security notes](#security-notes).

`PendingClientMigration` is per-contract: at most one migration may be pending
per contract at any time.

## TTL Helper API

All transient reads and writes go through the helpers in `contracts/escrow/src/ttl.rs`:

| Function | Description |
|---|---|
| `compute_expiry(env, ttl)` | Returns `sequence + ttl` (saturating). |
| `store_with_ttl(env, key, value, ttl)` | Writes to temporary storage and sets TTL. |
| `read_if_live(env, key)` | Returns `Some(v)` if live, `None` if absent or evicted. |
| `extend_if_below_threshold(env, key, threshold, extend_to)` | Bumps TTL; returns `false` if key absent. |
| `remove_transient(env, key)` | Explicit removal before auto-eviction. |
| `has_transient(env, key)` | Returns `true` if the key is currently live. |

## Expiry Semantics

- Soroban auto-evicts temporary storage entries once their TTL has elapsed.
- `read_if_live` returns `None` for both "never set" and "expired" — callers
  treat both as "no active pending record".
- No on-chain event is emitted at auto-eviction. Off-chain indexers should
  compute eviction by comparing `expires_at_ledger` against the current ledger
  sequence.

## Determinism

Expiry is computed at write time as:

```
expires_at_ledger = env.ledger().sequence() + TTL_LEDGERS
```

Given the same starting sequence and the same TTL constant, two independent
environments produce identical expiry values. This is verified by
`expiry_is_deterministic_across_independent_envs` in the test suite.

## Extending (Bumping) TTL

`extend_if_below_threshold` wraps
`env.storage().temporary().extend_ttl(key, threshold, extend_to)`:

- If remaining TTL is **below** the bump threshold, the entry's TTL is
  extended to the full policy value.
- If the entry is already fresh, the call is a no-op (Soroban only extends,
  never shrinks).
- If the entry is absent or already evicted, the helper returns `false` and
  performs no write.

## Security Notes

- **`MilestoneApprovals` bypasses `store_with_ttl`**. It uses
  `env.storage().temporary().set()` + `extend_ttl` directly in
  `approve_milestone`. This is a one-off pattern; all future transient keys
  should use `ttl::store_with_ttl`.
- `remove_transient` is used for explicit cleanup (e.g. after a migration is
  accepted) so stale entries do not linger until auto-eviction.
- `clear_approvals` removes `MilestoneApprovals` entries after a successful
  milestone release.
- The fail-closed design means a `None` from `read_if_live` always blocks the
  dependent operation, regardless of whether the entry expired or was never
  created.
- **No persistent key** has TTL bumping on read except `Contract(u32)`,
  `(Contract(u32), "milestones")`, and `NextContractId`. See
  [state-persistence.md](./state-persistence.md#security-notes) for the full
  list and associated risks.

## Testing

Tests live in
[contracts/escrow/src/test/ttl_tests.rs](../../contracts/escrow/src/test/ttl_tests.rs).
They call the TTL helpers directly via `env.as_contract` and advance
`LedgerInfo.sequence_number` via `env.ledger().with_mut(...)` to simulate
auto-eviction.

| Test | What it covers |
|---|---|
| `compute_expiry_equals_sequence_plus_ttl` | `compute_expiry` returns correct value for both TTL constants |
| `compute_expiry_saturates_on_overflow` | Saturating addition at `u32::MAX` |
| `ledgers_per_day_constant_is_correct` | All five constants match their documented values |
| `approval_readable_before_expiry` | `read_if_live` returns `Some` one ledger before approval TTL |
| `approval_evicted_after_expiry` | `read_if_live` returns `None` one ledger after approval TTL |
| `migration_readable_before_expiry` | `read_if_live` returns `Some` one ledger before migration TTL |
| `migration_evicted_after_expiry` | `read_if_live` returns `None` one ledger after migration TTL |
| `extend_returns_false_for_absent_key` | `extend_if_below_threshold` returns `false` when key absent |
| `extend_returns_true_and_entry_survives_past_original_expiry` | Bump keeps entry live past original expiry |
| `extend_migration_returns_false_for_absent_key` | Same absent-key check for migration threshold |
| `remove_transient_clears_entry_immediately` | Entry absent after `remove_transient` |
| `remove_transient_is_idempotent` | Second `remove_transient` does not panic |
| `has_transient_false_before_store` | `has_transient` returns `false` before any write |
| `has_transient_true_after_store_false_after_expiry` | `has_transient` tracks live/evicted state |
| `expiry_is_deterministic_across_independent_envs` | Same starting sequence → same expiry in two envs |

## Reviewer Checklist

1. Every new transient key has an entry in the table above.
2. Every write uses `ttl::store_with_ttl` (no direct `.temporary().set` bypass) — unless there is a documented reason (as with `MilestoneApprovals`).
3. Every read path uses `ttl::read_if_live` and handles `None` as "absent or expired".
4. A corresponding TTL test exists when a new transient key is introduced.
5. Every new persistent key is documented in [state-persistence.md](./state-persistence.md) with tier, value type, and TTL policy.
