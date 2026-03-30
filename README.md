# TalentTrust Contracts

Soroban smart contracts for the TalentTrust freelancer escrow protocol on Stellar.

## Repository Scope

- `contracts/escrow`: milestone escrow contract with persisted lifecycle state, participant metadata, governed validation parameters, reputation issuance, and pause controls
- `docs/escrow`: reviewer-focused escrow design, storage notes, and threat assumptions

## Escrow State Persistence

The escrow contract now persists the full payment lifecycle instead of relying on placeholder behavior:

- contract creation requires client authorization and validates immutable milestone inputs
- each escrow record stores the client, freelancer, milestone definitions, funded and released balances, milestone counters, lifecycle status, and timestamps
- deposits accumulate toward the total amount while rejecting overfunding
- milestone releases are one-way state transitions and cannot be replayed
- completed contracts mint a pending reputation credit for the recorded freelancer, and that credit is consumed exactly once when a rating is issued
- protocol governance parameters and pause or emergency flags are persisted separately from escrow records so operational controls survive across calls

Default protocol parameters:

- `min_milestone_amount = 1`
- `max_milestones = 16`
- `min_reputation_rating = 1`
- `max_reputation_rating = 5`

Reviewer-oriented notes live in [docs/escrow/README.md](docs/escrow/README.md), with storage-key details in [docs/escrow/state-persistence.md](docs/escrow/state-persistence.md) and threat analysis in [docs/escrow/security.md](docs/escrow/security.md).

## Security Model

The escrow implementation follows a fail-closed state machine:

- contract creation requires client authorization and rejects invalid participant or milestone metadata before persisting state
- deposits cannot exceed the required escrow total
- releases require the recorded client, a valid unreleased milestone, and enough funded balance to cover the payment
- reputation is gated behind contract completion and is issued once per contract
- governance changes use a one-time initialization plus a two-step admin transfer
- pause and emergency controls block all state-changing escrow operations while active

## Local Verification

```bash
cargo build
cargo fmt --all -- --check
cargo test -p escrow
cargo test test::performance -p escrow
```

## Development

Prerequisites:

- Rust 1.75+
- `rustfmt`
- optional Stellar CLI for deployment workflows

Common commands:

## CI/CD

On every push and pull request to `main`, GitHub Actions:

- Checks formatting (`cargo fmt --all -- --check`)
- Builds the workspace (`cargo build`)
- Runs tests (`cargo test`)

Ensure these pass locally before pushing.

## Upgradeable Storage Planning

- Versioned storage metadata and key namespaces are implemented in `contracts/escrow/src/lib.rs`.
- Dedicated storage planning tests are in:
  - `contracts/escrow/src/test/storage.rs`
  - `contracts/escrow/src/test/flows.rs`
  - `contracts/escrow/src/test/security.rs`
- Contract-specific documentation:
  - `docs/escrow/upgradeable-storage.md`
  - `docs/escrow/security.md`

## License

MIT
