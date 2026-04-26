# Escrow Contract Specification

This document defines the technical interface and state machine of the TalentTrust Escrow contract.

## 1. Data Structures

### `ContractStatus` (Enum)
- `Created`: Initial state after contract initialization.
- `Funded`: Funds have been deposited by the client.
- `Completed`: All milestones have been released; project finished.
- `Disputed`: Active dispute resolution in progress.
- `Cancelled`: Contract terminated; remaining funds returned to client.
- `Refunded`: Funds returned to client without completion.

### `Milestone` (Struct)
- `amount`: `i128` (Stroops)
- `released`: `bool`
- `refunded`: `bool`

---

## 2. Core Functions

### `create_contract`
Creates a new escrow agreement.
- **Arguments:**
  - `client`: `Address`
  - `freelancer`: `Address`
  - `arbiter`: `Option<Address>`
  - `milestone_amounts`: `Vec<i128>`
  - `terms_hash`: `Option<Bytes>`
  - `grace_period_seconds`: `Option<u64>`
- **Returns:** `u32` (contract_id)
- **Authorization:** `client.require_auth()`

### `deposit_funds`
Deposits the total required amount for the contract.
- **Arguments:**
  - `contract_id`: `u32`
  - `amount`: `i128`
- **Returns:** `bool`
- **Authorization:** `client.require_auth()`

### `approve_milestone`
Signals client approval of a specific milestone's deliverables.
- **Arguments:**
  - `contract_id`: `u32`
  - `milestone_index`: `u32`
- **Returns:** `bool`
- **Authorization:** `client.require_auth()`

### `release_milestone`
Transfers milestone funds to the freelancer.
- **Arguments:**
  - `contract_id`: `u32`
  - `milestone_index`: `u32`
- **Returns:** `bool`
- **Authorization:** `client.require_auth()` or `arbiter.require_auth()`

### `issue_reputation`
Records a freelancer rating after contract completion.
- **Arguments:**
  - `contract_id`: `u32`
  - `rating`: `u32` (1-5)
- **Returns:** `bool`
- **Authorization:** `client.require_auth()`

---

## 3. State Machine Summary

| Current Status | Action | Next Status | Note |
|----------------|--------|-------------|------|
| `Created` | `deposit_funds` | `Funded` | Requires exact or over-funding |
| `Created` | `cancel_contract` | `Cancelled` | Client or Freelancer |
| `Funded` | `release_milestone` (final) | `Completed` | All milestones must be released |
| `Funded` | `dispute_contract` | `Disputed` | Either party |
| `Funded` | `cancel_contract` | `Cancelled` | Restricted if milestones released |
| `Disputed` | `resolve_dispute` | `Completed` / `Cancelled` | Arbiter only |

---

## 4. Error Codes

| Code | Name | Description |
|------|------|-------------|
| 1 | `InvalidParticipant` | Client and Freelancer are the same address, or arbiter overlaps. |
| 2 | `EmptyMilestones` | No milestones provided during creation. |
| 3 | `InvalidMilestoneAmount` | Milestone amount is zero or negative. |
| 4 | `InvalidDepositAmount` | Deposit amount is zero or negative. |
| 5 | `InvalidMilestone` | Milestone index out of bounds. |
| 6 | `UnauthorizedRole` | Caller does not have permission for the action. |
| 7 | `InvalidStatusTransition` | Attempted action is not allowed in current state. |
| 8 | `AlreadyCancelled` | Contract is already in `Cancelled` state. |
| 9 | `ContractNotFound` | Provided `contract_id` does not exist. |
| 10 | `MilestonesAlreadyReleased`| Cancellation blocked because funds were already released. |

---

## 5. Security Notes

- **Fail-Closed:** Any unauthorized or invalid call results in an immediate panic, reverting all state changes.
- **Authorization:** Every state-changing function enforces `require_auth()` on the appropriate participant.
- **Immutability:** Once a contract is `Completed` or `Cancelled`, its core parameters and released funds cannot be modified.
