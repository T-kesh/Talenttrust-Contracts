# Storage Layout Reference — TalentTrust Escrow Contract

This document is the **authoritative map** of every `DataKey` variant defined in
`contracts/escrow/src/types.rs` to its storage tier, value type, writer/reader
entrypoints, and TTL (time-to-live) behavior.

**Cross-reference:**
- TTL constants: [`contracts/escrow/src/ttl.rs`](../../contracts/escrow/src/ttl.rs)
- Temporary-storage TTL policy: [`storage-ttl.md`](./storage-ttl.md)
- DataKey source enum: [`contracts/escrow/src/types.rs`](../../contracts/escrow/src/types.rs)

## Participant Indexes

`ClientContracts(Address)` and `FreelancerContracts(Address)` are stored as
`Vec<u32>` in persistent storage. These indexes are **append-only**: every
`create_contract` call appends the new contract id to both the client's and
the freelancer's index vectors. The contract list readers
(`list_contracts_by_participant`) are therefore consistent with contract
creation order.

---

## TTL Constants (from `ttl.rs`)

| Constant | Ledgers | Duration |
|---|---|---|
| `LEDGERS_PER_DAY` | 17 280 | 1 day |
| `PERSISTENT_TTL_LEDGERS` | 518 400 | 30 days |
| `PERSISTENT_BUMP_THRESHOLD` | 120 960 | 7 days |
| `PENDING_APPROVAL_TTL_LEDGERS` | 120 960 | 7 days |
| `PENDING_APPROVAL_BUMP_THRESHOLD` | 17 280 | 1 day |
| `PENDING_MIGRATION_TTL_LEDGERS` | 362 880 | 21 days |
| `PENDING_MIGRATION_BUMP_THRESHOLD` | 51 840 | 3 days |

All persistent keys use `PERSISTENT_TTL_LEDGERS` (30 days) with
`PERSISTENT_BUMP_THRESHOLD` (7 days). Temporary keys use their own constants.

---

## Milestone Released State — Single Source of Truth

`release_milestone` sets `milestone.released = true` inside the persisted
`Vec<Milestone>` stored under `(DataKey::Contract(id), "milestones")`.

`summarize_contract` (called by `finalize_contract`) derives
`released_milestone_count` by iterating that same vector and counting
`ms.released == true`. There is **no** separate `DataKey::MilestoneReleased`
key — that variant was removed in fix [#416] because it was never written,
causing `released_milestone_count` to always report zero in finalization
summaries.

Read and write path are now identical: the milestone vector is the sole
authority for released state.

---

## Storage Tier Legend

- **Persistent** (`env.storage().persistent()`) — survives upgrades; TTL managed
  manually via `extend_ttl`. Soroban evicts persistent entries after their TTL
  expires.
- **Temporary** (`env.storage().temporary()`) — auto-evicted by Soroban after
  TTL elapses; `read_if_live` returns `None` for absent *or* expired entries.
- **Instance** (`env.storage().instance()`) — contract-level storage (not used
  by any current `DataKey` variant).

---

## Storage Map

### Admin / Pause / Emergency

| DataKey | Tier | Value Type | Written By | Read By | TTL Policy |
|---|---|---|---|---|---|
| `Initialized` | Persistent | `bool` | `initialize` | `initialize`, `require_initialized`, `set_protocol_fee_bps`, `propose_governance_admin`, `accept_governance_admin` | No bump on access. Uses default persistent TTL (30 d). |
| `Admin` | Persistent | `Address` | `initialize`, `accept_governance_admin` | `get_admin`, `pause`, `unpause`, `activate_emergency_pause`, `resolve_emergency`, `set_protocol_fee_bps`, `propose_governance_admin`, `accept_governance_admin`, `get_governance_admin` | No bump on access. Uses default persistent TTL (30 d). |
| `Paused` | Persistent | `bool` | `pause`, `unpause`, `activate_emergency_pause`, `resolve_emergency` | `is_paused`, `require_not_paused`, `unpause` (guard) | No bump on access. Uses default persistent TTL (30 d). |
| `Emergency` | Persistent | `bool` | `activate_emergency_pause`, `resolve_emergency` | `is_emergency`, `unpause` (guard) | No bump on access. Uses default persistent TTL (30 d). |

### Contract Storage

| DataKey | Tier | Value Type | Written By | Read By | TTL Policy |
|---|---|---|---|---|---|
| `Contract(u32)` | Persistent | `Contract` | `create_contract`, `deposit_funds`, `release_milestone`, `refund_unreleased_milestones`, `cancel_contract`, `accept_client_migration` | `deposit_funds`, `release_milestone`, `refund_unreleased_milestones`, `get_contract`, `get_refundable_balance`, `cancel_contract`, `issue_reputation`, `approve_milestone`, `load_contract_for_finalization`, `load_contract` (migration) | **Bumped on every read/write** via `extend_contract_ttl`: bump threshold = 7 d, extend to = 30 d. |
| `(Contract(u32), "milestones")` *(composite key)* | Persistent | `Vec<Milestone>` | `create_contract`, `release_milestone`, `refund_unreleased_milestones` | `deposit_funds`, `release_milestone`, `refund_unreleased_milestones`, `get_milestones`, `approve_milestone` | **Bumped on every read/write** via `extend_milestone_ttl`: bump threshold = 7 d, extend to = 30 d. |
| `NextContractId` | Persistent | `u32` | `create_contract`, `bump_next_contract_id` | `next_contract_id`, `create_contract` | **Bumped on every write** via `extend_next_contract_id_ttl`: bump threshold = 7 d, extend to = 30 d. |
| `MilestoneApprovals(u32, u32)` | **Temporary** | `MilestoneApprovals` | `approve_milestone` | `approve_milestone` (load-or-create), `check_approvals`, `get_milestone_approvals` | TTL set on write: **7 d** (PENDING_APPROVAL_TTL_LEDGERS). Bump threshold = **1 d** (PENDING_APPROVAL_BUMP_THRESHOLD). Cleared by `clear_approvals` after release. **No bump on read** — fail-closed if expired. |
| `ClientContracts(Address)` | Persistent | `Vec<u32>` | `create_contract` | `list_contracts_by_participant` | No bump. |
| `FreelancerContracts(Address)` | Persistent | `Vec<u32>` | `create_contract` | `list_contracts_by_participant` | No bump. |

### Reputation

| DataKey | Tier | Value Type | Written By | Read By | TTL Policy |
|---|---|---|---|---|---|
| `ReputationIssued(u32)` | Persistent | `bool` | `issue_reputation` | `issue_reputation` (guard) | No bump. |
| `PendingReputationCredits(Address)` | Persistent | `i128` | `issue_reputation` (decrement), internal completion logic (increment) | `issue_reputation`, `get_pending_reputation_credits` | No bump. |
| `Reputation(Address)` | Persistent | `Reputation` | `issue_reputation` | `issue_reputation`, `get_reputation` | No bump. |

### Client Migration

| DataKey | Tier | Value Type | Written By | Read By | TTL Policy |
|---|---|---|---|---|---|
| `PendingClientMigration(u32)` | **Temporary** | `PendingClientMigration` | `propose_client_migration` | `pending_migration_exists`, `accept_client_migration`, `get_pending_client_migration` | TTL set on write via `store_with_ttl`: **21 d** (PENDING_MIGRATION_TTL_LEDGERS). Cleared by `remove_transient` on accept. **No bump on read.** |

### Protocol / Governance

| DataKey | Tier | Value Type | Written By | Read By | TTL Policy |
|---|---|---|---|---|---|
| `GovernanceAdmin` | *(unused)* | — | — | — | N/A. The governance module uses `DataKey::Admin` instead. |
| `PendingGovernanceAdmin` | *(unused)* | — | — | — | N/A. The governance module uses `DataKey::PendingAdmin` instead. |
| `ProtocolParameters` | *(unused)* | — | — | — | N/A. Declared but never stored. |
| `ProtocolFeeBps` | Persistent | `u32` | `set_protocol_fee_bps` | `set_protocol_fee_bps`, `get_protocol_fee_bps` | No bump. |
| `PendingAdmin` | Persistent | `Address` | `propose_governance_admin` | `accept_governance_admin`, `get_pending_governance_admin` | No bump. Cleared by `remove` on accept. |
| `AccumulatedProtocolFees` | Persistent | `i128` | `release_milestone` (increment) | `release_milestone` (read before increment) | No bump. |
| `GovernedParameters` | *(unused)* | — | — | — | N/A. Declared but never stored. |
| `ReadinessChecklist` | Persistent | `ReadinessChecklist` | `initialize`, `activate_emergency_pause` | `initialize`, `get_mainnet_readiness_info`, `activate_emergency_pause` | No bump. |

### Finalization

| DataKey | Tier | Value Type | Written By | Read By | TTL Policy |
|---|---|---|---|---|---|
| `Finalization(u32)` | Persistent | `FinalizationRecord` | `finalize_contract` | `get_finalization_record` | No bump. Written once (immutable close metadata). |

---

## Bump-on-Access Summary

Only **three** key families receive explicit TTL extension on every access:

| Key(s) | Mechanism | Bump Threshold | Extend To |
|---|---|---|---|
| `Contract(u32)` | `ttl::extend_contract_ttl` | 7 days | 30 days |
| `(Contract(u32), "milestones")` | `ttl::extend_milestone_ttl` | 7 days | 30 days |
| `NextContractId` | `ttl::extend_next_contract_id_ttl` | 7 days | 30 days |
| `MilestoneApprovals(u32, u32)` | `extend_ttl` in `approve_milestone` | 1 day | 7 days |

All other persistent keys (`Initialized`, `Admin`, `Paused`, `Emergency`,
`ReputationIssued`, `PendingReputationCredits`, `Reputation`, `ProtocolFeeBps`,
`PendingAdmin`, `AccumulatedProtocolFees`, `ReadinessChecklist`,
`Finalization`, `ClientContracts`, `FreelancerContracts`) are **never bumped on
read**. They rely on the initial write TTL and are vulnerable to eviction if the
contract goes untouched for 30 days.

---

## Unused / Declared-Only Variants

These four variants are defined in the enum but are **never stored or retrieved**
in production code. They exist for forward-looking schema design:

| DataKey | Notes |
|---|---|
| `GovernanceAdmin` | Superseded by `DataKey::Admin`. |
| `PendingGovernanceAdmin` | Superseded by `DataKey::PendingAdmin`. |
| `ProtocolParameters` | Never written. Governance uses `ProtocolFeeBps` + `GovernedParameters` struct. |
| `GovernedParameters` | The `GovernedParameters` struct is used as a value type but **never stored** under this DataKey variant. |

---

## Security Notes

1. **Missing TTL bumps on read-only keys** — `Admin`, `Initialized`, `Paused`,
   `Emergency`, `ReputationIssued`, `PendingReputationCredits`, `Reputation`,
   `ProtocolFeeBps`, `PendingAdmin`, `AccumulatedProtocolFees`,
   `ReadinessChecklist`, `Finalization`, `ClientContracts`, and
   `FreelancerContracts` are never TTL-extended after the initial write. If the
   entire contract is inactive for >30 days, these keys become eligible for
   Soroban auto-eviction, which could brick admin access and governance.
   Consider a background bump heartbeat or bundling with active key bumps.

2. **`MilestoneApprovals` TTL is write-only** — Approval TTL is set at write
   time in `approve_milestone` but is **not bumped on read** in
   `check_approvals`. Once the 7-day window elapses, approvals auto-evict,
   causing `check_approvals` to return `InsufficientApprovals`. This is
   intentional (fail-closed), but callers must be aware that prolonged approval
   periods require the approver to re-approve.

3. **`PendingClientMigration` TTL is write-only** — Set once at proposal (21 d)
   and never bumped on read. The migration must be accepted before eviction.

4. **Removal hygiene** — `PendingAdmin` and `PendingClientMigration` are
   explicitly removed on successful acceptance. `MilestoneApprovals` entries
   are removed after milestone release via `clear_approvals`. All other keys
   persist indefinitely (until TTL eviction).

---

## Verification: Enum Completeness

The table above covers **every** variant in the `DataKey` enum
(contracts/escrow/src/types.rs). The following test verifies that no variant is
omitted:

```rust
#[test]
fn all_datakey_variants_are_documented() {
    // This test compiles to a single assertion: listing a variant here
    // forces a compilation error if the enum gains new variants.
    // Each variant listed below is documented in state-persistence.md.
    let _ = |dk: DataKey| match dk {
        DataKey::Initialized
        | DataKey::Admin
        | DataKey::Paused
        | DataKey::Emergency
        | DataKey::Contract(_)
        | DataKey::NextContractId
        | DataKey::MilestoneReleased(_, _)
        | DataKey::MilestoneApprovals(_, _)
        | DataKey::ReputationIssued(_)
        | DataKey::PendingReputationCredits(_)
        | DataKey::Reputation(_)
        | DataKey::PendingClientMigration(_)
        | DataKey::GovernanceAdmin
        | DataKey::PendingGovernanceAdmin
        | DataKey::ProtocolParameters
        | DataKey::ProtocolFeeBps
        | DataKey::PendingAdmin
        | DataKey::AccumulatedProtocolFees
        | DataKey::GovernedParameters
        | DataKey::ReadinessChecklist
        | DataKey::Finalization(_) => {}
    };
}
```
