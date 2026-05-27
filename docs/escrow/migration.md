# Client Migration

## Overview

Client migration allows the existing escrow client role to be safely reassigned to a new address without changing contract state until the proposed client explicitly accepts the migration.

The implementation uses transient temporary storage under `DataKey::PendingClientMigration(contract_id)` with a Soroban TTL. This ensures proposals expire automatically after `PENDING_MIGRATION_TTL_LEDGERS` ledgers and cannot be accepted once stale.

## Public functions

- `propose_client_migration(env, contract_id, current_client, new_client) -> bool`
  - Requires `current_client` authorization.
  - Rejects when `new_client` equals the freelancer or the current client.
  - Rejects when the contract status is `Completed`, `Cancelled`, `Refunded`, or `Disputed`.
  - Stores the migration request in temporary storage with TTL.
  - Emits `client_migration_proposed`.

- `accept_client_migration(env, contract_id, new_client) -> bool`
  - Requires `new_client` authorization.
  - Loads the live pending migration from temporary storage.
  - Rejects if the proposal is missing or expired.
  - Rejects if the caller does not match the proposed client.
  - Atomically updates `EscrowContractData.client`.
  - Removes the transient migration request.
  - Emits `client_migration_accepted`.

- `has_pending_client_migration(env, contract_id) -> bool`
  - Returns whether a live pending migration exists.

- `get_pending_client_migration(env, contract_id) -> PendingClientMigration`
  - Returns the live pending migration record or panics if none exists.

## Security and invariants

- `EscrowContractData.client` is updated only in `accept_client_migration`.
- No contract mutation occurs during the proposal phase.
- Expired proposals are not accepted because `read_if_live` returns `None` once the TTL has elapsed.
- Paused or emergency states block both proposal and acceptance.
- Unauthorized callers cannot forge or accept migration proposals.

## Example

```rust
let contract_id = client.create_contract(&client_addr, &freelancer_addr, &milestones, &DepositMode::ExactTotal);
client.propose_client_migration(&contract_id, &client_addr, &new_client_addr);
assert!(client.has_pending_client_migration(&contract_id));
let pending = client.get_pending_client_migration(&contract_id);
assert_eq!(pending.proposed_client, new_client_addr);
client.accept_client_migration(&contract_id, &new_client_addr);
let contract = client.get_contract(&contract_id);
assert_eq!(contract.client, new_client_addr);
```
