# Escrow Integration Guide

This guide documents the entrypoints currently implemented by the escrow
contract. Planned features are listed separately and linked to their tracking
issues so integrators can distinguish live API from roadmap.

## Module Map

- `contracts/escrow/src/lib.rs`: contract type, shared API surface, reads, controls, cancellation, reputation, and module wiring.
- `contracts/escrow/src/create_contract.rs`: `create_contract` lifecycle entrypoint.
- `contracts/escrow/src/deposit.rs`: `deposit_funds` lifecycle entrypoint (SAC-aware; pulls tokens from client to escrow).
- `contracts/escrow/src/release_milestone.rs`: inlined in `lib.rs`; transfers tokens from escrow to freelancer net of protocol fee.
- `contracts/escrow/src/refund.rs`: `refund_unreleased_milestones` lifecycle entrypoint.
- `contracts/escrow/src/test/sac_custody.rs`: SAC custody tests for `bind_settlement_token`, `deposit_funds` (SAC path), and `release_milestone` (SAC path).

## Implemented API Surface

Lifecycle and reputation:

- `create_contract(client, freelancer, milestone_amounts, deposit_mode) -> u32`
- `deposit_funds(contract_id, amount) -> bool` *(SAC-aware; pulls tokens from client via `token::Client::transfer`)*
- `release_milestone(contract_id, milestone_index) -> bool` *(SAC-aware; pays freelancer net of protocol fee)*
- `issue_reputation(contract_id, caller, freelancer, rating) -> bool`
- `cancel_contract(contract_id, caller) -> bool`
- `finalize_contract(contract_id, finalizer) -> bool`

SAC settlement-token binding:

- `bind_settlement_token(token) -> bool` *(admin-only, single-use; binds the Stellar Asset Contract used for custody)*
- `get_settlement_token() -> Option<Address>`

Read-only queries:

- `get_contract(contract_id) -> EscrowContractData`
- `get_finalization_record(contract_id) -> Option<FinalizationRecord>`
- `get_reputation(freelancer) -> Option<ReputationRecord>`
- `get_average_rating(freelancer) -> Option<i128>`
- `get_pending_reputation_credits(freelancer) -> u32`
- `get_admin() -> Option<Address>`
- `get_settlement_token() -> Option<Address>`
- `is_paused() -> bool`
- `is_emergency() -> bool`
- `get_mainnet_readiness_info() -> MainnetReadinessInfo`

Operational controls:

- `initialize(admin) -> bool`
- `pause() -> bool`
- `unpause() -> bool`
- `activate_emergency_pause() -> bool`
- `resolve_emergency() -> bool`

## Canonical Happy Path

### 1. Initialize Operational Admin and Bind Settlement Token

```rust
escrow.initialize(&admin);
let sac = /* deployed Stellar Asset Contract address */;
escrow.bind_settlement_token(&sac);
```

`initialize` is single-use, requires `admin.require_auth()`, and stores the
admin used by pause, emergency, fee, and token-binding controls.
`bind_settlement_token` is also single-use (a second call panics with
`SettlementTokenAlreadyBound`) and emits a `(settl_tok, "bound")` audit event.

### 2. Create Contract

```rust
let contract_id = escrow.create_contract(
    &client_addr,
    &freelancer_addr,
    &None,
    &vec![&env, 500_0000000_i128, 500_0000000_i128],
    &ReleaseAuthorization::ClientOnly,
);
```

Creation requires `client.require_auth()`, rejects identical client/freelancer
addresses, rejects empty or non-positive milestones, caps milestone count at
`MAX_MILESTONES`, and caps total escrow value at `MAX_TOTAL_ESCROW_STROOPS`.

### 3. Deposit Funds (SAC debit)

```rust
escrow.deposit_funds(&contract_id, &client_addr, &1000_0000000_i128);
```

`deposit_funds` now performs a real on-chain transfer: the escrow contract
calls `token::Client::transfer(caller, escrow, amount)` against the bound
settlement token BEFORE updating `funded_amount`. If the SAC transfer fails
the contract accounting is unchanged, so a partial-deposit run can never
leave the accounting counter ahead of the actual custody balance.

`ExactTotal` contracts require one exact deposit equal to the milestone total.
`Incremental` contracts allow partial deposits until the milestone total is
reached. Deposits that exceed the required total fail closed with
`InvalidDepositAmount`.

### 4. Release Milestone (SAC payout)

```rust
escrow.release_milestone(&contract_id, &client_addr, &0);
```

`release_milestone` performs a real on-chain payout:

1. All existing pre-conditions (pause gate, auth, approval, role /
   `ReleaseAuthorization`, milestone state, balance) hold.
2. The escrow reads the bound settlement token; if no token has been bound it
   panics with `SettlementTokenNotConfigured`.
3. The escrow reads the protocol fee (set via `set_protocol_fee_bps`); the
   payout to the freelancer is `milestone.amount - fee`, with `fee` retained
   inside the contract via `DataKey::AccumulatedProtocolFees`.
4. `token::Client::transfer(escrow, freelancer, payout)` is invoked BEFORE
   the milestone is marked released and the contract status is updated — so
   a token-transfer failure leaves the contract untouched.

When the final milestone is released, status becomes `Completed` and one
pending reputation credit is added for the freelancer.

### 5. Issue Reputation

```rust
escrow.issue_reputation(&contract_id, &client_addr, &freelancer_addr, &5_i128);
```

Reputation requires `caller.require_auth()`, the caller must be the stored
client, the freelancer argument must match the contract freelancer, the contract
must be `Completed`, rating must be `1..=5`, and each contract can issue
reputation once.

## Custody Lifecycle (SAC token integration)

The escrow holds a real Stellar Asset Contract (SAC) balance for the lifetime
of each contract. The flow is fully on-chain and atomic — every state change
to the contract's accounting counters is paired with the matching SAC
`transfer` call. There is no off-chain reconciliation step.

| Step | Entrypoint | SAC operation | State change |
|---|---|---|---|
| Token binding (admin, single-use) | `bind_settlement_token(sac)` | `—` | `DataKey::SettlementToken = sac` |
| Funding | `deposit_funds(id, client, amount)` | `transfer(client, escrow, amount)` | `contract.funded_amount += amount` |
| Release | `release_milestone(id, caller, idx)` | `transfer(escrow, freelancer, milestone.amount - fee)` | `milestone.released = true`, `contract.released_amount += milestone.amount`, `DataKey::AccumulatedProtocolFees += fee` |
| Refund (planned) | `refund_unreleased_milestones(id, indices)` | `transfer(escrow, client, sum)` | `milestone.refunded = true`, `contract.refunded_amount += sum` |

The pause/emergency gate, fail-closed validation, and TTL bumps from the
existing lifecycle are preserved unchanged on each path.

### Failure semantics

| Failure | Behaviour |
|---|---|
| `bind_settlement_token` called twice | `SettlementTokenAlreadyBound` panic; existing token retained |
| `deposit_funds` with no token bound | `SettlementTokenNotConfigured` panic; funded_amount unchanged |
| `deposit_funds` with insufficient SAC balance | SAC transfer fails; `TokenTransferFailed` panic via Soroban's contract-error return; funded_amount unchanged |
| `release_milestone` with no token bound | `SettlementTokenNotConfigured` panic; milestone state unchanged |
| `release_milestone` with insufficient SAC balance | SAC transfer fails; milestone state unchanged |

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
- `("settl_tok", "bound")` on `bind_settlement_token`
- `("paused", timestamp)` on `pause`
- `("unpaused", timestamp)` on `unpause`
- `("emergency", "activated")` and `("emergency", "resolved")`
- `("audit", contract_id)` for lifecycle state transitions
- `("created", contract_id)` on contract creation
- `("deposited", contract_id)` on deposit (with payload `(caller, amount, funded_amount, total, settlement_token)`)
- `("released", contract_id, milestone_index)` on release (with payload `(freelancer, payout, fee, settlement_token)`)
- `("rep_issd", contract_id)` on reputation issuance
- `("cancelled", contract_id)` on cancellation
- `("finalized", contract_id)` on finalization

The `("deposited", contract_id)` event is emitted on every successful
`deposit_funds` call (previously only status-changing deposits surfaced an
event). It includes the settlement-token address so off-chain indexers can
correlate the audit event with the SAC's own `transfer` event.

The `("released", contract_id, milestone_index)` event's payload now also
includes the gross payout, retained fee, and settlement-token address so
indexers can reconcile the escrow's accounting against the SAC's
`transfer` events without re-deriving fee math.

## Implemented Security Assumptions

- Creation and reputation issue require explicit address authentication.
- Pause and emergency controls are admin-authenticated.
- The settlement token is admin-bound once via `bind_settlement_token`; a
  second call panics with `SettlementTokenAlreadyBound` and is audited via
  the `("settl_tok", "bound")` event.
- Deposits pull real SAC tokens from the client via `token::Client::transfer`
  BEFORE updating `funded_amount`, so a failed transfer leaves accounting
  untouched.
- Deposits cannot exceed the exact milestone total; over-funding is detected
  via `checked_add` and panics with `InvalidDepositAmount`.
- Releases pay the freelancer (less protocol fee) via
  `token::Client::transfer` BEFORE updating milestone/contract state, so a
  failed payout leaves state untouched.
- Releases fail on duplicate milestone release, invalid milestone id, missing
  contract, paused state, missing settlement token, and insufficient funded
  balance.
- Arithmetic for escrow totals, deposits, and releases uses checked helpers
  and panics with `PotentialOverflow` or `InvalidDepositAmount` on overflow.
- Accounting is checked after balance-changing operations; SAC balance and
  accounting counter cannot diverge through any tested path.

## Planned Features

These features are not implemented entrypoints today:

- Two-step admin transfer: planned in
  [#318](https://github.com/Talenttrust/Talenttrust-Contracts/issues/318).
- Protocol fee treasury withdrawal: planned in
  [#314](https://github.com/Talenttrust/Talenttrust-Contracts/issues/314).
  Note: fee accumulation is now wired into `release_milestone` (issue #439).
  A `withdraw_protocol_fees` entrypoint remains unimplemented pending the
  dedicated fee-treasury issue.
- Governed parameter setter/readiness wiring: planned in
  [#323](https://github.com/Talenttrust/Talenttrust-Contracts/issues/323).
- `refund_unreleased_milestones` SAC refund path (the function exists but
  deferring the `token::Client::transfer` to the client until the
  refund-treasury issue picks up tracking).
- `migrate_state` / `StateV1` / `StateV2` migration flow: not implemented;
  tracked by this reconciliation issue
  [#341](https://github.com/Talenttrust/Talenttrust-Contracts/issues/341)
  until a dedicated implementation issue exists.

Any documentation that describes one of these items as available should be
treated as roadmap text, not live integration guidance.
