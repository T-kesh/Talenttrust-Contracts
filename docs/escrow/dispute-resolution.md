# Dispute Resolution

The escrow contract now exposes an explicit dispute lifecycle for arbiter-backed contracts:

- `create_contract_with_arbiter(...)` creates a contract that can later be disputed.
- `raise_dispute(contract_id, caller)` moves a funded or partially funded contract into `Disputed`.
- `resolve_dispute(contract_id, arbiter, resolution)` closes the dispute and allocates the remaining escrow balance.

## Contract Requirements

- Only contracts created with `create_contract_with_arbiter` can enter the dispute flow.
- The arbiter must be distinct from both the client and the freelancer.
- Disputes are only valid from `Funded` or `PartiallyFunded`.
- Resolution is only valid from `Disputed`.

## Public API

### `create_contract_with_arbiter`

```rust
pub fn create_contract_with_arbiter(
    env: Env,
    client: Address,
    freelancer: Address,
    arbiter: Address,
    milestone_amounts: Vec<i128>,
    deposit_mode: DepositMode,
) -> u32
```

Creates a new escrow contract and stores the assigned arbiter in contract state.

### `raise_dispute`

```rust
pub fn raise_dispute(env: Env, contract_id: u32, caller: Address) -> bool
```

Rules:

- `caller.require_auth()` is enforced before any state mutation.
- `caller` must equal `contract.client` or `contract.freelancer`.
- `contract.arbiter` must be set.
- `contract.status` must be `Funded` or `PartiallyFunded`.
- On success, the contract transitions to `Disputed`.
- The contract emits both an audit event and a `dispute` event.

### `resolve_dispute`

```rust
pub fn resolve_dispute(
    env: Env,
    contract_id: u32,
    arbiter: Address,
    resolution: DisputeResolution,
) -> bool
```

Rules:

- `arbiter.require_auth()` is enforced before any state mutation.
- `arbiter` must equal the stored `contract.arbiter`.
- `contract.status` must be `Disputed`.
- Resolution applies only to the available balance:

```text
available_balance = total_deposited - released_amount - refunded_amount
```

- On success, the contract transitions to:
  - `Refunded` when the client receives the full deposited amount
  - `Completed` for any mixed or freelancer-positive outcome
- The contract emits both an audit event and a `dsp_res` event.

## Resolution Types

```rust
pub enum DisputeResolution {
    FullRefund,
    PartialRefund,
    FullPayout,
    Split(i128, i128),
}
```

### `FullRefund`

- Client receives 100% of the remaining balance.
- Freelancer receives 0%.

### `PartialRefund`

- Client receives 70% of the remaining balance.
- Freelancer receives 30% of the remaining balance.
- Integer rounding favors the client:

```text
freelancer = floor(available_balance * 30 / 100)
client = available_balance - freelancer
```

### `FullPayout`

- Freelancer receives 100% of the remaining balance.
- Client receives 0%.

### `Split(client_amount, freelancer_amount)`

- Custom absolute payouts chosen by the arbiter.
- Both amounts must be non-negative.
- Their sum must equal the remaining balance exactly.

## Accounting Invariants

Resolution is fail-closed:

- `released_amount` is incremented only by the freelancer payout.
- `refunded_amount` is incremented only by the client payout.
- Resolution panics unless:

```text
released_amount + refunded_amount == total_deposited
```

- The core invariant is rechecked after every dispute transition:

```text
total_deposited == released_amount + refunded_amount + available_balance
```

## Security Notes

- Unauthorized callers cannot raise or resolve disputes.
- Arbiter-less contracts cannot enter the dispute lifecycle.
- `release_milestone` is blocked while a contract is `Disputed`.
- Dispute actions are blocked while the contract is paused or in emergency mode.
- No separate dispute storage record is introduced, so there is no new TTL surface to manage.
- Resolution logic uses checked arithmetic and rejects invalid custom splits.

## Test Coverage

The active unit tests cover:

- client and freelancer dispute raising
- partially funded dispute entry
- missing-arbiter rejection
- non-party and non-arbiter rejection
- full refund, fixed 70/30 partial refund, and custom split resolution
- disputed-state release blocking
- pause blocking for both dispute entrypoints

## Validation Note

`cargo check -p escrow --tests` passes on the current workspace.

Full `cargo test -p escrow` is currently blocked by the local Windows GNU linker limit for the contract `cdylib` export table, and the installed MSVC toolchain does not have `link.exe` available on this machine.
