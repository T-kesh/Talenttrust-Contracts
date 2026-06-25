# Escrow Integration Guide

This guide provides a precise, deterministic, and example-driven overview of the TalentTrust Escrow system. It is intended for integrators, auditors, and operators.

## 1. 🔁 Canonical Happy Path (PRIMARY FLOW)

The full lifecycle of a successful escrow contract follows this sequence:

### Step: create
**Function:** `create_contract`  
**Caller:** Client  
**Pre-state:** N/A  
**Post-state:** `Created`  
**Event:** `created { contract_id, client, freelancer, total_amount }`  
**Example:**
```rust
escrow.create_contract(
    &client_addr,
    &freelancer_addr,
    &vec![&env, 500_0000000, 500_0000000], // 2 milestones
    &None, // terms_hash
    &Some(3600) // grace_period
);
```

### Step: create_with_arbiter (dispute-aware)
**Function:** `create_contract_with_arbiter`  
**Caller:** Client  
**Pre-state:** N/A  
**Post-state:** `Created`  
**Notes:** Identical to `create_contract` except that the contract records an
`Option<Address>` arbiter. Passing `Some(arbiter_addr)` enables the dispute
lifecycle (see §3 below); passing `None` is equivalent to `create_contract`.
The arbiter must not equal `client` or `freelancer`.

### Step: deposit
**Function:** `deposit_funds`  
**Caller:** Client  
**Pre-state:** `Created`  
**Post-state:** `Funded`  
**Event:** `deposited { contract_id, amount, payer }`  
**Example:**
```rust
escrow.deposit_funds(&contract_id, &1000_0000000);
```

### Step: approve
**Function:** `approve_milestone`  
**Caller:** Client  
**Pre-state:** `Funded`  
**Post-state:** `Funded` (Milestone marked as approved)  
**Event:** `approved { contract_id, milestone_index }`  
**Example:**
```rust
escrow.approve_milestone(&contract_id, &0);
```

### Step: release
**Function:** `release_milestone`  
**Caller:** Client / Arbiter  
**Pre-state:** `Funded` (and approved)  
**Post-state:** `Funded` or `Completed` (if last milestone)  
**Event:** `released { contract_id, milestone_index, amount }`  
**Example:**
```rust
escrow.release_milestone(&contract_id, &0);
```

### Step: complete
**Trigger:** Final `release_milestone` or `refund_unreleased_milestones`  
**Caller:** N/A (Internal transition)  
**Pre-state:** `Funded`  
**Post-state:** `Completed` or `Refunded`  
**Event:** `completed { contract_id }` or `refunded { contract_id, amount }`  

### Step: reputation
**Function:** `issue_reputation`  
**Caller:** Client  
**Pre-state:** `Completed`  
**Post-state:** `Completed` (Reputation credit consumed)  
**Event:** `rated { contract_id, freelancer, rating }`  
**Example:**
```rust
escrow.issue_reputation(&contract_id, &5);
```

---

## 2. 🔐 Authorization Modes

| Function | Authorized Caller(s) | Rejection Behavior |
|----------|----------------------|-------------------|
| `create_contract` | Any (becomes Client) | N/A |
| `deposit_funds` | Client | `UnauthorizedRole` |
| `approve_milestone` | Client | `UnauthorizedRole` |
| `release_milestone` | Client, Arbiter | `UnauthorizedRole` (also: `InvalidState` when the contract is `Disputed`) |
| `cancel_contract` | Client, Freelancer, Arbiter | `UnauthorizedRole` (depends on state) |
| `refund_unreleased_milestones` | Arbiter | `UnauthorizedRole` |
| `raise_dispute` | Client, Freelancer | `UnauthorizedRole` / `InvalidState` / `DisputeArbiterMissing` |
| `resolve_dispute` | Arbiter (registered on contract) | `UnauthorizedRole` / `InvalidState` / `DisputeNotFound` |
| `resolve_dispute_split` | Arbiter (registered on contract) | `UnauthorizedRole` / `InvalidState`/`DisputeNotFound` / `NonPositiveAmount` / `AccountingInvariantViolated` |
| `finalize_contract` | Client | `UnauthorizedRole` |
| `withdraw_leftover` | Client | `UnauthorizedRole` |
| `issue_reputation` | Client | `UnauthorizedRole` |

**Arbiter Override:** The arbiter can call `release_milestone` or `refund_unreleased_milestones` to resolve disputes or unstick funds. The arbiter is the *sole* authority for `resolve_dispute` and `resolve_dispute_split`.

---

## 3. ⚖️ Dispute Resolution Flow

Contracts created with `Some(arbiter_addr)` support an end-to-end dispute
lifecycle. Direct milestone release is *blocked* once a dispute is raised —
the arbiter is the only party that can move funds out of the dispute state.

### Step: raise_dispute
**Function:** `raise_dispute(contract_id, caller, reason_hash: BytesN<32>) -> bool`  
**Caller:** Client **or** Freelancer  
**Pre-state:** `Funded` (also `PartiallyFunded`)  
**Post-state:** `Disputed`  
**Rejections:**
- caller is not client or freelancer → `UnauthorizedRole`
- contract has no arbiter → `DisputeArbiterMissing`
- contract in any non-Funded/PartiallyFunded state (already disputed,
  completed, refunded, cancelled) → `InvalidState`
- paused or in emergency → `ContractPaused`

```rust
escrow.raise_dispute(
    &contract_id,
    &client_addr,
    &BytesN::from_array(&env, &[0xab; 32]),
);
```

### Step: resolve_dispute (Release / Refund / Cancel)
**Function:** `resolve_dispute(contract_id, caller, resolution: DisputeResolution) -> bool`  
**Caller:** the registered Arbiter only  
**Pre-state:** `Disputed`  
**Post-state:** `Completed` / `Refunded` / `Cancelled` / `Funded` (mixed)  
**Rejections:**
- caller is not the arbiter → `UnauthorizedRole` (or, in production
  before the role check, a Soroban auth error — `require_auth` runs
  before `require_arbiter`)
- contract not in `Disputed` state → `InvalidState`
- dispute metadata missing (i.e. `raise_dispute` was not called) →
  `DisputeNotFound`
- paused or in emergency → `ContractPaused`

```rust
// Release everything to the freelancer (terminal: Completed)
escrow.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::Release);

// Refund everything to the client (terminal: Refunded)
escrow.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::Refund);

// Cancel without moving funds (terminal: Cancelled)
escrow.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::Cancel);
```

The simple-variant payouts are derived deterministically from the contract's
available escrow balance (`total_deposited - released_amount - refunded_amount`)
and the new status is computed from the *post*-application accounting, so
fully-funded `Release` resolves to `Completed` (not `Funded`).

### Step: resolve_dispute_split (Split(client, freelancer))
**Function:** `resolve_dispute_split(contract_id, caller, split: DisputeSplit) -> bool`  
**Caller:** the registered Arbiter only  
**Pre:** contract must be `Disputed`  
**Post:** `Completed` / `Refunded` / `Funded` (mixed)  
**Rejections (in addition to the auth/state guard above):**
- `client_amount < 0` or `freelancer_amount < 0` → `NonPositiveAmount`
- `client_amount + freelancer_amount != available_balance` →
  `AccountingInvariantViolated`

```rust
let split = DisputeSplit {
    client_amount: 100,
    freelancer_amount: 200,    // must sum to (total_deposited - released - refunded)
};
escrow.resolve_dispute_split(&contract_id, &arbiter_addr, &split);
```

The Split invariants are validated *before* any state writes — the
arbiter cannot corrupt the accounting by submitting an inconsistent
split.

### Step: get_dispute
**Function:** `get_dispute(contract_id) -> DisputeMetadata { raised_by, reason_hash, raised_at }`  
**Rejection:** no dispute on record → `DisputeNotFound`

### State machine update

| From | To | Trigger |
|------|----|---------|
| `Funded` / `PartiallyFunded` | `Disputed` | `raise_dispute` |
| `Disputed` | `Completed` | Arbiter `resolve_dispute(Release)`, or `resolve_dispute_split` with `client=0` |
| `Disputed` | `Refunded` | Arbiter `resolve_dispute(Refund)`, or `resolve_dispute_split` with `freelancer=0` |
| `Disputed` | `Cancelled` | Arbiter `resolve_dispute(Cancel)` |
| `Disputed` | `Funded` (mixed) | `resolve_dispute_split` with both components non-zero |

While in `Disputed`, direct `release_milestone` calls are rejected with
`InvalidState` so that the only accounting changes originate from the
arbiter.

---

## 4. 📣 Event Model

Events are critical for off-chain indexers to track the state of escrow contracts.

| Event Name | Topic | Payload Fields | Interpretation |
|------------|-------|----------------|---------------|
| `created` | `(created, contract_id)` | `client, freelancer, timestamp` | New contract initialized in `Created` state. |
| `deposited` | — | `contract_id, amount, payer` | Funds successfully moved into escrow. |
| `approved` | — | `contract_id, milestone_index` | Work verified by client. |
| `released` | `(released, contract_id, milestone_index)` | `amount, timestamp` | Funds moved from escrow to freelancer. |
| `completed` | — | `contract_id` | All milestones paid; reputation credit available. |
| `refunded` | — | `contract_id, amount` | Funds returned to client by arbiter. |
| `rated` | — | `contract_id, freelancer, rating` | Rating recorded; credit consumed. |
| `cancelled` | `(cancelled, contract_id)` | `caller, timestamp` | Contract terminated; remaining funds returned. |
| `finalized` | — | `contract_id` | Contract closed for leftover withdrawals. |
| `withdrawn` | — | `contract_id, amount, caller` | Leftover funds withdrawn by client. |
| `dsp_rais` | `(dsp_rais, contract_id)` | `caller, reason_hash, timestamp` | Dispute raised; contract → `Disputed`. |
| `dsp_resl` | `(dsp_resl, contract_id)` | `caller, resolution_code, client_payout, freelancer_payout, timestamp` | Dispute resolved by arbiter. `resolution_code`: `0`=Release, `1`=Refund, `2`=Cancel, `3`=Split. |
| `audit` | `(audit, contract_id)` | `from_status, to_status, actor, timestamp` | Compact audit log for every state transition. |

*Note: All events include a ledger timestamp for ordering.*

---

## 5. ❌ Failure Modes & Edge Cases

| Scenario | Behavior | Error Returned |
|----------|----------|----------------|
| Double Deposit | Allowed (increments balance) | N/A |
| Double Release | Blocked (milestone already released) | `AlreadyReleased` |
| Unauthorized Release | Blocked (caller is not client/arbiter) | `UnauthorizedRole` |
| Release in `Disputed` | Blocked (only arbiter can move funds in dispute) | `InvalidState` |
| Release in `Cancelled` | Blocked (terminal state) | `InvalidStatusTransition` |
| Cancellation after Release| Allowed only for unreleased milestones | `MilestonesAlreadyReleased` (for client) |
| Over-funding | Allowed (excess can be withdrawn after finalization) | N/A |
| Dispute raised twice | Blocked (contract is already `Disputed`) | `InvalidState` |
| Dispute raised on non-Funded | Blocked (must reach Funded/PartiallyFunded) | `InvalidState` |
| Dispute raised without arbiter | Blocked | `DisputeArbiterMissing` |
| Dispute resolve by non-arbiter | Blocked | `UnauthorizedRole` / Soroban auth error |
| Dispute resolve outside `Disputed` | Blocked | `InvalidState` |
| Split sum ≠ available | Blocked pre-state-write | `AccountingInvariantViolated` |
| Split component < 0 | Blocked pre-state-write | `NonPositiveAmount` |

---

## 6. 🔄 Alternative Flows

### A. Cancellation Flow
**Paths:**
1. `Created` → `Cancelled`: Either Client or Freelancer can trigger.
2. `Funded` → `Cancelled`:
   - Client (if zero milestones released)
   - Freelancer (anytime, funds return to client)
   - Arbiter (dispute resolution)
**Funds:** All unreleased funds are returned to the client (accounting updated).

### B. Refund Flow
**Trigger:** `refund_unreleased_milestones` (Arbiter only)  
**Condition:** Contract in `Funded` or `Disputed` state.  
**Effect:** Specified milestones marked as `refunded`; funds marked as refundable to client.

### C. Dispute Flow (see §3 above)
**Sequence:** `Funded` → `Disputed` → `Arbiter Decision` via `resolve_dispute` / `resolve_dispute_split` → `Completed`/`Refunded`/`Cancelled` / partial `Funded`  
**Initiation:** Either Client or Freelancer calls `raise_dispute` after the contract is funded.

---

## 7. 🧠 State Machine Summary

| From | To | Trigger |
|------|----|---------|
| `Created` | `Funded` | `deposit_funds` |
| `Created` | `Cancelled` | `cancel_contract` |
| `Funded` / `PartiallyFunded` | `Disputed` | `raise_dispute` |
| `Funded` | `Completed` | Final `release_milestone` |
| `Funded` | `Disputed` | `dispute_contract` |
| `Funded` | `Cancelled` | `cancel_contract` |
| `Funded` | `Refunded` | Final `refund_unreleased_milestones` |
| `Disputed`| `Completed` / `Refunded` / `Cancelled` / `Funded` | Arbiter `resolve_dispute` or `resolve_dispute_split` |

---

## 8. 🔍 Integration Examples

### Full Lifecycle Example (Pseudo-code)
```javascript
// 1. Create with an arbiter so disputes can be resolved
const contractId = await escrow.create_contract_with_arbiter(
  client, freelancer, arbiter, [100, 200]
);

// 2. Deposit
await escrow.deposit_funds(contractId, 300);

// 3. Work done... Release Milestone 1
await escrow.release_milestone(contractId, 0);
await escrow.release_milestone(contractId, 1);

// 4. Issue Reputation
await escrow.issue_reputation(contractId, 5);
```

### Dispute Example (Pseudo-code)
```javascript
// Dispute flow
await escrow.raise_dispute(contractId, client, reasonHash);
// ... arbiter decides ...
await escrow.resolve_dispute(contractId, arbiter, "Refund");
// Or, for an arbitrary split:
await escrow.resolve_dispute_split(contractId, arbiter, { client_amount: 100, freelancer_amount: 200 });
```

---

## 9. 🔐 Security Notes

- Only the `protocol_fee_account` can adjust fee rate or withdraw accrued fees.
- Fee account is authenticated with `caller.require_auth()`.
- Fee bounds enforced at 0..=10000.
- All protocol fee operations use persisted state and safe integer arithmetic.
- Dispute resolutions are restricted to the registered arbiter via
  `require_auth()` followed by a `require_arbiter` role check, ensuring the
  production auth flow rejects unauthorised callers with a Soroban auth
  error before reaching the role check.
- Split payouts are validated (`NonPositiveAmount` /
  `AccountingInvariantViolated`) before any state writes, preserving the
  `total_deposited == released_amount + refunded_amount + available_balance`
  invariant on every code path.

## Behaviour on release

On each milestone release:
- Compute fee: `milestone.amount * protocol_fee_bps / 10000`.
- Save fee to milestone object.
- Increment `protocol_fee_accrued`.
- Mark milestone released and contract status completed when all milestones done.
# Escrow Contract Documentation

**Mainnet readiness (limits, events, risks):** [mainnet-readiness.md](mainnet-readiness.md)

This document summarizes the reviewer-facing architecture for `contracts/escrow`.

## Scope

The contract persists:

- escrow lifecycle state for each contract
- participant metadata for the client and freelancer
- milestone release state
- funded and released accounting
- pending and issued reputation aggregates
- protocol governance parameters
- pause and emergency flags

## Public Flows

Core escrow endpoints:

- `create_contract(client, freelancer, milestone_amounts) -> u32`
- `create_contract_with_arbiter(client, freelancer, arbiter, milestone_amounts) -> u32`
- `deposit_funds(contract_id, amount) -> bool`
- `release_milestone(contract_id, milestone_id) -> bool`
- `issue_reputation(contract_id, rating) -> bool`
- `raise_dispute(contract_id, caller, reason_hash) -> bool`
- `resolve_dispute(contract_id, caller, resolution: DisputeResolution) -> bool`
- `resolve_dispute_split(contract_id, caller, split: DisputeSplit) -> bool`
- `get_dispute(contract_id) -> DisputeMetadata`
- `get_contract(contract_id) -> EscrowContractData`
- `get_reputation(freelancer) -> Option<ReputationRecord>`
- `get_pending_reputation_credits(freelancer) -> u32`

Operational controls:

- `initialize(admin) -> bool`
- `pause() -> bool`
- `unpause() -> bool`
- `activate_emergency_pause() -> bool`
- `resolve_emergency() -> bool`
- `is_paused() -> bool`
- `is_emergency() -> bool`

Governance:

- `initialize_protocol_governance(admin, min_milestone_amount, max_milestones, min_reputation_rating, max_reputation_rating) -> bool`
- `update_protocol_parameters(...) -> bool`
- `propose_governance_admin(next_admin) -> bool`
- `accept_governance_admin() -> bool`
- `get_protocol_parameters() -> ProtocolParameters`
- `get_governance_admin() -> Option<Address>`
- `get_pending_governance_admin() -> Option<Address>`

The escrow tests are grouped into dedicated modules:

To prevent out-of-gas or infinite-loop denial of service attacks, the escrow contract enforces creation limits:

- maximum milestone count is capped by `ProtocolParameters.max_milestones` (defaults to 16)
- total escrow amount is bounded by the immutable mainnet cap (`MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS`)

## Lifecycle Model

Supported lifecycle transitions:

- `Created -> Accepted` after freelancer or arbiter accepts the contract terms
- `Accepted -> Funded` after any positive deposit
- `Funded -> Disputed` after `raise_dispute` (requires arbiter configured)
- `Funded -> Completed` after the final unreleased milestone is released
- `Disputed -> Completed | Refunded | Cancelled | Funded` after the registered arbiter calls `resolve_dispute` / `resolve_dispute_split`

Operational invariants:

- client and freelancer addresses are immutable after creation
- milestone amounts are immutable after creation
- each milestone can transition from `released = false` to `released = true` exactly once
- `released_amount` is the sum of released milestone amounts
- `released_milestones` matches the number of released milestone flags
- `reputation_issued` can only become `true` after `Completed`
- `total_deposited == released_amount + refunded_amount + available_balance` on every code path including Split resolutions
- `release_milestone` is blocked while the contract is `Disputed`

## Incident Response

### Emergency Response

1. Detect incident and call `activate_emergency_pause`.
2. Investigate and remediate root cause.
3. Validate mitigations in test/staging.
4. Call `resolve_emergency` to restore service.
5. Publish incident summary for ecosystem transparency.

## Persistence Notes

Each `EscrowContractData` record stores:

- participant addresses
- milestone vector and cached milestone count
- total escrow amount
- funded and released balances
- released milestone count
- contract status
- reputation issuance flag
- creation and update timestamps

Each `DisputeMetadata` record stores:

- `raised_by: Address`
- `reason_hash: BytesN<32>`
- `raised_at: u64`

Detailed storage-key coverage is documented in [state-persistence.md](state-persistence.md).

## Test Coverage

The escrow regression suite is split by concern:

- `flows.rs`: happy-path lifecycle and reputation aggregation
- `lifecycle.rs`: state transition persistence
- `persistence.rs`: storage round-trip assertions
- `security.rs`: failure paths and validation checks
- `governance.rs`: admin and parameter persistence
- `pause_controls.rs` and `emergency_controls.rs`: operational safety controls
- `dispute.rs`: arbiter-guarded raise/resolve paths, Split invariants, state-blocking for `release_milestone`
- `performance.rs`: resource regression ceilings

## Deterministic Lifecycle Events (v1)

Lifecycle operations now emit a standardized event shape to simplify indexing and alerting.

- Topic tuple: `("escrow", "v1", operation, contract_id)`
- Data tuple: `(status, amount, milestone_index, actor, timestamp)`

Operation values:

- `create`
- `deposit`
- `approve`
- `release`
- `cancel`
- `dispute_raise`
- `dispute_resolve`

Schema notes:

- `status`: post-operation `ContractStatus`
- `amount`: operation amount (or `0` when not applicable)
- `milestone_index`: milestone index (or `0` when not applicable)
- `actor`: `Some(Address)` when a caller identity is relevant, otherwise `None`
- `timestamp`: ledger timestamp at emission

Backwards compatibility:

- Previous ad-hoc topics such as `("contract_cancelled", contract_id)` are replaced by the v1 lifecycle schema.
- Indexers should migrate to the new topic/data tuples for deterministic parsing.
