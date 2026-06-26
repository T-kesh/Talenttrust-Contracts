# Storage Layout Reference — TalentTrust Escrow Contract

This document maps the currently implemented `DataKey` storage used by
`contracts/escrow/src/lib.rs`. A fuller key-by-key reference, including
declared-but-unused keys, is tracked in
[#342](https://github.com/Talenttrust/Talenttrust-Contracts/issues/342).

## Persistent-Key TTL Policy (#400)

All critical persistent keys are bumped to 30 days (17 280 ledgers × 30) whenever they
are read or written. The bump is skipped when the key is absent (no-op guard).

| Key | TTL Extend-to | Bump Threshold | Bumped by |
| --- | --- | --- | --- |
| `Admin` | 30 days | 7 days | `initialize`, `get_admin`, `set_protocol_fee_bps` |
| `GovernanceAdmin` | 30 days | 7 days | governance admin reads/writes |
| `ProtocolFeeBps` | 30 days | 7 days | `set_protocol_fee_bps`, `release_milestone` (fee read) |
| `AccumulatedProtocolFees` | 30 days | 7 days | `release_milestone` (fee write) |
| `Finalization(id)` | 30 days | 7 days | `finalize_contract`, `get_finalization_record` |
| `Reputation(addr)` | 30 days | 7 days | `issue_reputation`, `get_reputation` |
| `PendingReputationCredits(addr)` | 30 days | 7 days | `issue_reputation` |
| `Contract(id)` / milestones | 30 days | 7 days | all lifecycle entrypoints |
| `NextContractId` | 30 days | 7 days | `create_contract` |

Constants live in `contracts/escrow/src/ttl.rs`:
- `PERSISTENT_TTL_LEDGERS = LEDGERS_PER_DAY * 30` (≈ 30 days)
- `PERSISTENT_BUMP_THRESHOLD = LEDGERS_PER_DAY * 7` (≈ 7 days)

## Live Storage Keys

These participant indexes are **append-only**: every `create_contract` appends the new id to the appropriate index vectors.
The contract list readers (`list_contracts_by_participant`) are therefore consistent with contract creation order.



| Key | Value | Written by |
| --- | --- | --- |
| `Initialized` | `bool` | `initialize` |
| `Admin` | `Address` | `initialize` |
| `Paused` | `bool` | `pause`, `unpause`, emergency controls |
| `Emergency` | `bool` | emergency controls |
| `Contract(id)` | `EscrowContractData` | create/deposit/release/reputation/cancel |
| `NextContractId` | `u32` | `create_contract` |
| `ReputationIssued(id)` | `bool` | `issue_reputation` |
| `PendingReputationCredits(address)` | `u32` | final release, `issue_reputation` |
| `Reputation(address)` | `ReputationRecord` | `issue_reputation` |
| `Finalization(id)` | `FinalizationRecord` | `finalize_contract` |
| `ReadinessChecklist` | `ReadinessChecklist` | initialize and emergency controls |
| `ClientContracts(address)` | `Vec<u32>` | create_contract |
| `FreelancerContracts(address)` | `Vec<u32>` | create_contract |

## Approval Expiry & TTL Eviction (`MilestoneApprovals`)

`MilestoneApprovals(contract_id, milestone_index)` is stored in **temporary**
storage by `approve_milestone_release`. Unlike the persistent keys above,
temporary entries carry a finite, ledger-denominated lifetime and are
auto-evicted by Soroban once it elapses.

| Property | Value | Source |
| --- | --- | --- |
| Storage class | `temporary` | `approvals::approve_milestone` |
| Lifetime on write | `PENDING_APPROVAL_TTL_LEDGERS` (`17_280 × 7` ≈ 7 days) | `ttl.rs` |
| Bump threshold | `PENDING_APPROVAL_BUMP_THRESHOLD` (`17_280` ≈ 1 day) | `ttl.rs` |

**Semantics enforced by the contract and covered by
`src/test/approval_expiry.rs`:**

- **Inclusive boundary.** An approval recorded at ledger `S` stays live through
  ledger `S + PENDING_APPROVAL_TTL_LEDGERS` (the final live ledger). At that
  exact boundary `get_milestone_approvals` still returns `Some(..)` and a
  release succeeds.
- **Eviction.** One ledger past the boundary, Soroban evicts the entry;
  `get_milestone_approvals` returns `None`.
- **Fail-closed release.** Because `check_approvals` treats a missing entry as
  `InsufficientApprovals`, `release_milestone` is rejected after expiry — an
  expired approval never authorizes a release. This is the core security
  invariant.
- **Re-approval.** After expiry the caller may approve again; this writes a
  fresh entry with a full TTL and re-enables release.
- **Activity extends TTL (bump path).** Each approval call `set`s the entry and
  `extend_ttl`s it back to the full window. An approval recorded while the entry
  sits below `PENDING_APPROVAL_BUMP_THRESHOLD` of expiry therefore renews the
  lifetime, so ongoing activity keeps a pending approval alive.
- **Independent per-milestone expiry.** Each `(contract_id, milestone_index)`
  carries its own TTL anchored to its own approval ledger; an older milestone's
  approval can expire while a newer one remains live.
- **No partial-approval resurrection (MultiSig).** Client and freelancer
  approvals share one record. If a partial approval expires, a later co-signer's
  approval starts a *fresh* record holding only that co-signer's flag — the
  expired signature is not resurrected, so release stays blocked until a live
  quorum is re-established.

On successful release, `clear_approvals` removes the entry immediately to
prevent approval reuse, independent of TTL.

## Declared But Not Live

These keys are declared in `types.rs` but no public entrypoint currently uses
them as a complete feature:

- `PendingClientMigration`
- `ProtocolFeeBps`
- `AccumulatedProtocolFees`

Protocol fee implementation is tracked in
[#313](https://github.com/Talenttrust/Talenttrust-Contracts/issues/313) and
[#314](https://github.com/Talenttrust/Talenttrust-Contracts/issues/314).

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

### 3. Reputation Auditing States
* **`PendingReputation(Address)` / `ReputationIssued(u32)`**
    * **Description:** Bookkeeping indices capturing un-issued tokens and completion certificates for network participants.
    * **Storage Lifespan:** `Persistent`. Preserved explicitly to guarantee deterministic chronological processing when users harvest pending system values.

- Contract ids are monotonically assigned from `NextContractId`.
- Milestone amounts and participant addresses are immutable after creation.
- `total_deposited`, `released_amount`, and `refunded_amount` are checked after
  balance-changing operations.
- A milestone release flag can move from absent/false to true only once.
- Reputation issuance is guarded by `ReputationIssued(contract_id)`.
