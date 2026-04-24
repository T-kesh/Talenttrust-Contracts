# Per-Milestone Funding Tracking

## Overview

The escrow contract now supports **per-milestone funding tracking** to enable clearer accounting, safer partial releases, and more flexible refund operations. This feature tracks the funded amount for each milestone independently, allowing clients to fund milestones incrementally and ensuring releases only occur when sufficient funds are allocated to specific milestones.

## Motivation

Previously, the contract tracked only aggregate funding totals (`total_funded` and `total_released`). This approach had limitations:

1. **Unclear Allocation**: No visibility into which milestones were funded
2. **Partial Funding Ambiguity**: Difficult to determine if a specific milestone could be released
3. **Refund Complexity**: Refunds relied on aggregate calculations without per-milestone context
4. **Accounting Gaps**: No clear audit trail of per-milestone funding decisions

Per-milestone funding tracking addresses these issues by maintaining explicit funding records for each milestone.

## Data Structures

### Enhanced ContractData

```rust
pub struct ContractData {
    pub client: Address,
    pub freelancer: Address,
    pub milestones: Vec<i128>,
    pub total_funded: i128,        // NEW: Total funds deposited
    pub total_released: i128,      // NEW: Total funds released
}
```

### Storage Keys

Per-milestone funding is stored using a dedicated key:

```rust
DataKey::MilestoneFunded(contract_id: u32, milestone_idx: u32) -> i128
```

This allows efficient lookup and update of funding amounts without loading the entire contract state.

## API

### `deposit_funds(contract_id: u32, amount: i128) -> bool`

Deposits funds into a contract. The total deposited cannot exceed the sum of all milestone amounts.

**Parameters:**
- `contract_id`: The contract identifier
- `amount`: The amount to deposit (must be > 0)

**Returns:** `true` on success

**Errors:**
- `InvalidDepositAmount`: If amount ≤ 0 or would exceed total contract value

**Example:**
```rust
client.deposit_funds(&contract_id, &600_i128);
```

### `set_milestone_funded(contract_id: u32, milestone_idx: u32, amount: i128) -> bool`

Sets the funded amount for a specific milestone. This can be called multiple times to update the funding allocation.

**Parameters:**
- `contract_id`: The contract identifier
- `milestone_idx`: The milestone index (0-based)
- `amount`: The funded amount (must be ≥ 0)

**Returns:** `true` on success

**Errors:**
- `InvalidMilestone`: If milestone index is out of bounds
- `InvalidDepositAmount`: If amount < 0

**Example:**
```rust
client.set_milestone_funded(&contract_id, &0, &100_i128);
client.set_milestone_funded(&contract_id, &1, &200_i128);
client.set_milestone_funded(&contract_id, &2, &300_i128);
```

### `get_milestone_funded(contract_id: u32, milestone_idx: u32) -> i128`

Retrieves the funded amount for a specific milestone.

**Parameters:**
- `contract_id`: The contract identifier
- `milestone_idx`: The milestone index (0-based)

**Returns:** The funded amount (0 if not set)

**Example:**
```rust
let funded = client.get_milestone_funded(&contract_id, &0);
assert_eq!(funded, 100_i128);
```

### `release_milestone(contract_id: u32, milestone_idx: u32) -> bool`

Releases a milestone, transferring funds to the freelancer. The milestone must have sufficient funding allocated.

**Parameters:**
- `contract_id`: The contract identifier
- `milestone_idx`: The milestone index (0-based)

**Returns:** `true` on success

**Errors:**
- `InvalidMilestone`: If milestone index is out of bounds
- `InsufficientMilestoneFunding`: If funded amount < milestone amount

**Example:**
```rust
client.release_milestone(&contract_id, &0);
```

## Accounting Invariants

The per-milestone funding system maintains the following invariants:

### 1. Funding Balance Invariant

```
total_available = total_funded - total_released
```

The available balance must equal total deposits minus total releases.

### 2. Per-Milestone Funding Invariant

```
For each milestone i:
  0 ≤ funded_amount[i] ≤ milestone_amount[i]
```

Funded amounts must be non-negative and not exceed the milestone amount.

### 3. Release Consistency Invariant

```
For each released milestone i:
  funded_amount[i] ≥ milestone_amount[i]
```

A milestone can only be released if its funded amount meets or exceeds its amount.

### 4. Total Funding Invariant

```
sum(funded_amount[i] for all i) ≤ total_funded
```

The sum of all per-milestone funding cannot exceed total deposits.

## Usage Patterns

### Pattern 1: Full Funding

Fund all milestones at once after depositing the full amount:

```rust
// Create contract with 3 milestones: 100, 200, 300
let contract_id = client.create_contract(&client, &freelancer, &milestones);

// Deposit full amount
client.deposit_funds(&contract_id, &600_i128);

// Fund all milestones
client.set_milestone_funded(&contract_id, &0, &100_i128);
client.set_milestone_funded(&contract_id, &1, &200_i128);
client.set_milestone_funded(&contract_id, &2, &300_i128);

// Release as work is completed
client.release_milestone(&contract_id, &0);
client.release_milestone(&contract_id, &1);
client.release_milestone(&contract_id, &2);
```

### Pattern 2: Incremental Funding

Fund milestones as deposits are made:

```rust
// Create contract
let contract_id = client.create_contract(&client, &freelancer, &milestones);

// First deposit and fund
client.deposit_funds(&contract_id, &300_i128);
client.set_milestone_funded(&contract_id, &0, &100_i128);
client.set_milestone_funded(&contract_id, &1, &200_i128);

// Second deposit and fund remaining
client.deposit_funds(&contract_id, &300_i128);
client.set_milestone_funded(&contract_id, &2, &300_i128);

// Release milestones
client.release_milestone(&contract_id, &0);
client.release_milestone(&contract_id, &1);
client.release_milestone(&contract_id, &2);
```

### Pattern 3: Partial Funding with Selective Release

Fund only some milestones and release them:

```rust
// Create contract
let contract_id = client.create_contract(&client, &freelancer, &milestones);

// Deposit partial funds
client.deposit_funds(&contract_id, &300_i128);

// Fund only first two milestones
client.set_milestone_funded(&contract_id, &0, &100_i128);
client.set_milestone_funded(&contract_id, &1, &200_i128);

// Release funded milestones
client.release_milestone(&contract_id, &0);
client.release_milestone(&contract_id, &1);

// Later, deposit and fund the third milestone
client.deposit_funds(&contract_id, &300_i128);
client.set_milestone_funded(&contract_id, &2, &300_i128);
client.release_milestone(&contract_id, &2);
```

## Security Considerations

### 1. Funding Validation

- All funding amounts are validated to be non-negative
- Milestone indices are bounds-checked
- Total funding cannot exceed contract value

### 2. Release Authorization

- Releases require explicit per-milestone funding allocation
- Prevents accidental or unauthorized releases
- Ensures funds are available before release

### 3. Storage Isolation

- Per-milestone funding is stored separately from contract state
- Reduces storage footprint of main contract record
- Enables efficient updates without loading full contract

### 4. Atomicity

- All funding operations are atomic
- Either the entire operation succeeds or fails
- No partial state corruption possible

## Test Coverage

The implementation includes 15+ tests covering:

### Basic Operations (5 tests)
- `test_partial_funding_single_milestone`: Partial funding of one milestone
- `test_mixed_funding_multiple_milestones`: Different funding amounts per milestone
- `test_partial_release_with_per_milestone_funding`: Release after partial funding
- `test_incremental_funding_per_milestone`: Funding in stages
- `test_update_milestone_funding`: Updating funding amounts

### Edge Cases (5 tests)
- `test_zero_funding_milestone`: Zero funding allocation
- `test_release_unfunded_milestone`: Attempting to release without funding
- `test_release_without_sufficient_milestone_funding`: Insufficient funding for release
- `test_multiple_contracts_independent_funding`: Funding isolation across contracts
- `test_full_lifecycle_with_per_milestone_funding`: Complete workflow

### Error Handling (2 tests)
- Panic on insufficient funding
- Panic on unfunded release attempts

## Migration Guide

### For Existing Contracts

If upgrading from the previous version:

1. **Update Contract Creation**: No changes required; new contracts automatically support per-milestone funding
2. **Update Deposit Logic**: Existing `deposit_funds` calls work unchanged
3. **Add Funding Allocation**: Call `set_milestone_funded` after deposits to allocate funds
4. **Update Release Logic**: Existing `release_milestone` calls now require per-milestone funding

### Example Migration

**Before:**
```rust
client.deposit_funds(&contract_id, &600_i128);
client.release_milestone(&contract_id, &0);  // Worked without explicit funding
```

**After:**
```rust
client.deposit_funds(&contract_id, &600_i128);
client.set_milestone_funded(&contract_id, &0, &100_i128);
client.release_milestone(&contract_id, &0);  // Now requires explicit funding
```

## Performance Characteristics

- **Deposit**: O(1) - Updates total_funded counter
- **Set Milestone Funded**: O(1) - Direct storage write
- **Get Milestone Funded**: O(1) - Direct storage read
- **Release**: O(1) - Checks funding and updates total_released

## Future Enhancements

Potential improvements for future versions:

1. **Automatic Funding Allocation**: Distribute deposits across milestones automatically
2. **Funding Schedules**: Time-based funding releases
3. **Conditional Funding**: Fund milestones based on completion criteria
4. **Funding Rollback**: Revert funding allocations before release
5. **Funding History**: Audit trail of all funding changes

## References

- [Funding Accounting Invariants](./FUNDING_ACCOUNTING.md)
- [Contract Architecture](./architecture.md)
- [Release Readiness Checklist](./release-readiness-checklist.md)
