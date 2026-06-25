# Escrow Integration Guide

This guide documents the entrypoints currently implemented by the escrow
contract. Planned features are listed separately and linked to their tracking
issues so integrators can distinguish live API from roadmap.

## Module Map

- `contracts/escrow/src/lib.rs`: contract type, shared API surface, reads, controls, cancellation, reputation, and module wiring.
- `contracts/escrow/src/create_contract.rs`: `create_contract` lifecycle entrypoint.
- `contracts/escrow/src/deposit.rs`: `deposit_funds` lifecycle entrypoint.
- `contracts/escrow/src/release.rs`: `release_milestone` lifecycle entrypoint.
- `contracts/escrow/src/refund.rs`: `refund_unreleased_milestones` lifecycle entrypoint.

## Implemented API Surface

Lifecycle and reputation:

- `create_contract(client, freelancer, milestone_amounts, deposit_mode) -> u32`
- `deposit_funds(contract_id, amount) -> bool`
- `release_milestone(contract_id, milestone_index) -> bool`
- `issue_reputation(contract_id, caller, freelancer, rating) -> bool`
- `cancel_contract(contract_id, caller) -> bool`
- `finalize_contract(contract_id, finalizer) -> bool`

Read-only queries:

- `get_contract(contract_id) -> EscrowContractData`
- `get_milestones(contract_id) -> Vec<Milestone>`
- `get_refundable_balance(contract_id) -> i128`
- `get_milestone_approvals(contract_id, milestone_index) -> Option<MilestoneApprovals>`
- `get_finalization_record(contract_id) -> Option<FinalizationRecord>`
- `get_reputation(freelancer) -> Option<ReputationRecord>`
- `get_average_rating(freelancer) -> Option<i128>`
- `get_pending_reputation_credits(freelancer) -> u32`
- `get_admin() -> Option<Address>`
- `is_paused() -> bool`
- `is_emergency() -> bool`
- `get_mainnet_readiness_info() -> MainnetReadinessInfo`

### Read-only getter semantics

The read getters below are stable, side-effect-free paths that indexers and
off-chain callers rely on. They share three properties:

1. **Not-found**: every getter that takes a `contract_id` panics the contract
   with `ContractNotFound` when `contract_id` was never allocated. The
   Soroban-generated `try_*` wrappers surface this as
   `Err(Ok(ContractNotFound))` for off-chain callers and do not mutate any
   other persistent state.
2. **Pure read**: invoking any of these getters on a valid `contract_id` does
   not mutate balances, status, milestones, or per-milestone flags.
   Accounting-only fields (`funded_amount`, `released_amount`,
   `refunded_amount`) and per-milestone `released`/`refunded` flags are
   bitwise-stable across arbitrary numbers of repeated calls.
3. **TTL on read (persistent only)**: on a successful read the contract
   extends the persistent TTL of the entry being read from (`Contract(id)`,
   `(Contract(id), "milestones")`) to `PERSISTENT_TTL_LEDGERS` (30 days).
   This keeps idle but live contracts in storage without rebuilding them. The
   `get_milestone_approvals` getter reads from temporary storage and is
   therefore exempt from this rule; it is governed by
   `PENDING_APPROVAL_TTL_LEDGERS` and the host's auto-eviction.

Per-getter details:

- `get_contract(contract_id)` returns the full `EscrowContractData`
  (participants, arbiter, status, funded/released/refunded amounts,
  release_authorization). Reads persist the contract entry's TTL. Panics
  `ContractNotFound` for an unknown id.
- `get_milestones(contract_id)` returns the milestones vector in creation
  order. Reads persist the milestones entry's TTL. Panics `ContractNotFound`
  for an unknown id.
- `get_refundable_balance(contract_id)` returns
  `funded_amount - released_amount - refunded_amount`. The result must be
  non-negative by construction; panic-on-overflow is enforced on
  contributing arithmetic at every mutating entrypoint. Reads persist the
  contract entry's TTL. Panics `ContractNotFound` for an unknown id.
- `get_milestone_approvals(contract_id, milestone_index)` returns `Some`
  only if a non-expired approval record for that milestone exists in
  temporary storage. Returns `None` when no approval has been recorded or
  when the contract id is unknown. Does not extend persistent TTL because
  approvals live in temporary storage bounded by
  `PENDING_APPROVAL_TTL_LEDGERS`.

These properties are locked in by tests under
`contracts/escrow/src/test/persistence.rs` (issue #475).

Operational controls:

- `initialize(admin) -> bool`
- `pause() -> bool`
- `unpause() -> bool`
- `activate_emergency_pause() -> bool`
- `resolve_emergency() -> bool`

## Canonical Happy Path

### 1. Initialize Operational Admin

```rust
escrow.initialize(&admin);
```

`initialize` is single-use, requires `admin.require_auth()`, and stores the
admin used by pause and emergency controls.

### 2. Create Contract

```rust
let contract_id = escrow.create_contract(
    &client_addr,
    &freelancer_addr,
    &vec![&env, 500_0000000_i128, 500_0000000_i128],
    &DepositMode::ExactTotal,
);
```

Creation requires `client.require_auth()`, rejects identical client/freelancer
addresses, rejects empty or non-positive milestones, caps milestone count at
`MAX_MILESTONES`, and caps total escrow value at `MAX_TOTAL_ESCROW_STROOPS`.

### 3. Deposit Funds

```rust
escrow.deposit_funds(&contract_id, &1000_0000000_i128);
```

`ExactTotal` contracts require one exact deposit equal to the milestone total.
`Incremental` contracts allow partial deposits until the milestone total is
reached. Deposits that exceed the required total fail closed.

### 4. Release Milestones

```rust
escrow.release_milestone(&contract_id, &0);
```

Current implementation note: `release_milestone` does not yet authenticate the
client or an arbiter. It validates the contract id, milestone index, unreleased
state, available funded balance, and paused state, then marks the milestone as
released. This authorization gap is intentionally documented here until the auth
fix lands.

When the final milestone is released, status becomes `Completed` and one pending
reputation credit is added for the freelancer.

### 5. Issue Reputation

```rust
escrow.issue_reputation(&contract_id, &client_addr, &freelancer_addr, &5_i128);
```

Reputation requires `caller.require_auth()`, the caller must be the stored
client, the freelancer argument must match the contract freelancer, the contract
must be `Completed`, rating must be `1..=5`, and each contract can issue
reputation once.

## Cancellation

```rust
escrow.cancel_contract(&contract_id, &caller);
```

Cancellation requires `caller.require_auth()`. The caller must be the stored
client or freelancer. It is blocked after `Completed` and blocked if the
contract is already `Cancelled`.

## Finalization

```rust
escrow.finalize_contract(&contract_id, &finalizer);
```

Finalization requires `finalizer.require_auth()`. The finalizer must be the
stored client, freelancer, or assigned arbiter. It is allowed only while the
contract status is `Completed` or `Disputed`.

The contract writes one immutable `FinalizationRecord` containing the finalizer,
ledger timestamp, and a `ContractSummary` snapshot. After the record exists,
contract-specific mutating calls reject with `AlreadyFinalized`.

## Pause and Emergency Controls

`pause`, `unpause`, `activate_emergency_pause`, and `resolve_emergency` require
the stored admin's authorization. While paused or in emergency, mutating
lifecycle calls fail with `ContractPaused`; read-only queries remain available.
`unpause` fails while emergency mode is active.

## Events

Implemented events:

- `("init", "admin_set")` on `initialize`
- `("paused", timestamp)` on `pause`
- `("unpaused", timestamp)` on `unpause`
- `("emergency", "activated")` and `("emergency", "resolved")`
- `("audit", contract_id)` for lifecycle state transitions
- `("created", contract_id)` on contract creation
- `("released", contract_id, milestone_index)` on release
- `("rep_issd", contract_id)` on reputation issuance
- `("cancelled", contract_id)` on cancellation
- `("finalized", contract_id)` on finalization

There is no dedicated deposit event in the current implementation unless the
deposit changes contract status and therefore emits an audit event. Structured
deposit and fee events are planned in
[#336](https://github.com/Talenttrust/Talenttrust-Contracts/issues/336).

## Implemented Security Assumptions

- Creation and reputation issue require explicit address authentication.
- Pause and emergency controls are admin-authenticated.
- Deposits cannot exceed the exact milestone total.
- Releases fail on duplicate milestone release, invalid milestone id, missing
  contract, paused state, and insufficient funded balance.
- Arithmetic for escrow totals, deposits, and releases uses checked helpers and
  panics with `PotentialOverflow` on overflow.
- Accounting is checked after balance-changing operations.
- The contract stores accounting state only; token custody and token transfers
  are not implemented in `lib.rs` and must be handled by an audited integration.
- Storage uses persistent keys. TTL constants exist for planned pending approval
  and migration flows, but no current public entrypoint writes those pending
  records.

## Planned Features

These features are not implemented entrypoints today:

- Two-step admin transfer: planned in
  [#318](https://github.com/Talenttrust/Talenttrust-Contracts/issues/318).
- Protocol fee deduction on release: planned in
  [#313](https://github.com/Talenttrust/Talenttrust-Contracts/issues/313).
- Protocol fee treasury withdrawal: planned in
  [#314](https://github.com/Talenttrust/Talenttrust-Contracts/issues/314).
- Governed parameter setter/readiness wiring: planned in
  [#323](https://github.com/Talenttrust/Talenttrust-Contracts/issues/323).
- Structured deposit and fee events: planned in
  [#336](https://github.com/Talenttrust/Talenttrust-Contracts/issues/336).
- Storage-key reference for declared-but-unused keys, including pending client
  migration and protocol fee keys: planned in
  [#342](https://github.com/Talenttrust/Talenttrust-Contracts/issues/342).
- `migrate_state` / `StateV1` / `StateV2` migration flow: not implemented;
  tracked by this reconciliation issue
  [#341](https://github.com/Talenttrust/Talenttrust-Contracts/issues/341)
  until a dedicated implementation issue exists.

Any documentation that describes one of these items as available should be
treated as roadmap text, not live integration guidance.
