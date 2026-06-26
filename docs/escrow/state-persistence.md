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

## Declared But Not Live

These keys are declared in `types.rs` but no public entrypoint currently uses
them as a complete feature:

- `MilestoneApprovals`
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
