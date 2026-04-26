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
    &Some(arbiter_addr),
    &vec![&env, 500_0000000, 500_0000000], // 2 milestones
    &None, // terms_hash
    &Some(3600) // grace_period
);
```

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
| `release_milestone` | Client, Arbiter | `UnauthorizedRole` |
| `cancel_contract` | Client, Freelancer, Arbiter | `UnauthorizedRole` (depends on state) |
| `refund_unreleased_milestones` | Arbiter | `UnauthorizedRole` |
| `finalize_contract` | Client | `UnauthorizedRole` |
| `withdraw_leftover` | Client | `UnauthorizedRole` |
| `issue_reputation` | Client | `UnauthorizedRole` |

**Arbiter Override:** The arbiter can call `release_milestone` or `refund_unreleased_milestones` to resolve disputes or unstick funds.

---

## 3. 📣 Event Model

Events are critical for off-chain indexers to track the state of escrow contracts.

| Event Name | Payload Fields | Interpretation |
|------------|----------------|----------------|
| `created` | `contract_id, client, freelancer, total_amount` | New contract initialized in `Created` state. |
| `deposited` | `contract_id, amount, payer` | Funds successfully moved into escrow. |
| `approved` | `contract_id, milestone_index` | Work verified by client. |
| `released` | `contract_id, milestone_index, amount` | Funds moved from escrow to freelancer. |
| `completed` | `contract_id` | All milestones paid; reputation credit available. |
| `refunded` | `contract_id, amount` | Funds returned to client by arbiter. |
| `rated` | `contract_id, freelancer, rating` | Rating recorded; credit consumed. |
| `cancelled` | `contract_id, caller, status, timestamp` | Contract terminated; remaining funds returned. |
| `finalized` | `contract_id` | Contract closed for leftover withdrawals. |
| `withdrawn` | `contract_id, amount, caller` | Leftover funds withdrawn by client. |

*Note: All events include a ledger timestamp for ordering.*

---

## 4. ❌ Failure Modes & Edge Cases

| Scenario | Behavior | Error Returned |
|----------|----------|----------------|
| Double Deposit | Allowed (increments balance) | N/A |
| Double Release | Blocked (milestone already released) | `AlreadyReleased` |
| Unauthorized Release | Blocked (caller is not client/arbiter) | `UnauthorizedRole` |
| Release in `Created` | Blocked (insufficient funds) | `ContractNotFound` (if wrong ID) |
| Release in `Cancelled` | Blocked (terminal state) | `InvalidStatusTransition` |
| Cancellation after Release| Allowed only for unreleased milestones | `MilestonesAlreadyReleased` (for client) |
| Over-funding | Allowed (excess can be withdrawn after finalization) | N/A |

---

## 5. 🔄 Alternative Flows

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

### C. Dispute Flow
**Sequence:** `Funded` → `Disputed` → `Arbiter Decision` → `Release/Refund`  
**Initiation:** Either party calls `dispute_contract`.  
**Arbiter Authority:** In `Disputed` state, the Arbiter has full authority to release or refund milestones.

---

## 6. 🧠 State Machine Summary

| From | To | Trigger |
|------|----|---------|
| `Created` | `Funded` | `deposit_funds` |
| `Created` | `Cancelled` | `cancel_contract` |
| `Funded` | `Completed` | Final `release_milestone` |
| `Funded` | `Disputed` | `dispute_contract` |
| `Funded` | `Cancelled` | `cancel_contract` |
| `Funded` | `Refunded` | Final `refund_unreleased_milestones` |
| `Disputed`| `Completed` | Arbiter `release_milestone` |
| `Disputed`| `Cancelled` | Arbiter `cancel_contract` |

---

## 7. 🔍 Integration Examples

### Full Lifecycle Example (Pseudo-code)
```javascript
// 1. Create
const contractId = await escrow.create_contract(client, freelancer, null, [100, 200]);

// 2. Deposit
await escrow.deposit_funds(contractId, 300);

// 3. Work done... Approve & Release Milestone 1
await escrow.approve_milestone(contractId, 0);
await escrow.release_milestone(contractId, 0);

// 4. Work done... Release Milestone 2 (auto-completes)
await escrow.release_milestone(contractId, 1);

// 5. Issue Reputation
await escrow.issue_reputation(contractId, 5);
```

---

## 8. 🔐 Security Notes

- **Role Enforcement:** All state-changing functions use `require_auth()` to verify the caller's identity.
- **State Transition Guarantees:** Transitions are governed by an explicit state machine; invalid transitions (e.g., `Completed` → `Cancelled`) are physically impossible.
- **Idempotency:** Operations like `cancel_contract` check current state to prevent duplicate execution.
- **Fund Safety:** Funds are only released to the freelancer address specified at creation or returned to the client.

---

## 9. ⚠️ Operational Notes

- **Stuck States:** If a client refuses to release funds for completed work, the freelancer should trigger a dispute to involve the arbiter.
- **Event Monitoring:** Systems should monitor `released` and `completed` events to trigger off-chain payouts or notifications.
- **Finalization:** Clients must call `finalize_contract` after completion/cancellation to unlock `withdraw_leftover`.
