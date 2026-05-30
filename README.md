# TalentTrust Contracts

Soroban smart contracts for the TalentTrust freelancer escrow protocol on Stellar.

## Repository Scope

- **Escrow contract** (`contracts/escrow`): Holds funds in escrow, supports milestone-based payments and reputation credential issuance.
- **Planned escrow fee model**: Configurable protocol fee accounting is not implemented in `contracts/escrow/src/lib.rs`; fee deduction is tracked in [#313](https://github.com/Talenttrust/Talenttrust-Contracts/issues/313) and fee withdrawal in [#314](https://github.com/Talenttrust/Talenttrust-Contracts/issues/314).

Reviewer-oriented notes live in [docs/escrow/README.md](docs/escrow/README.md), with storage-key details in [docs/escrow/state-persistence.md](docs/escrow/state-persistence.md) and threat analysis in [docs/escrow/SECURITY.md](docs/escrow/SECURITY.md).

## Security Model

The escrow implementation follows a fail-closed state machine:

- contract creation requires client authorization and rejects invalid participant or milestone metadata before persisting state
- deposits cannot exceed the required escrow total
- releases require a valid unreleased milestone and enough funded balance to cover the payment; caller authorization is not yet implemented for `release_milestone`
- reputation is gated behind contract completion and is issued once per contract
- finalization records immutable close metadata for completed or disputed contracts and blocks later contract-specific mutations
- one-time admin initialization protects pause and emergency controls; two-step admin transfer is planned in [#318](https://github.com/Talenttrust/Talenttrust-Contracts/issues/318)
- pause and emergency controls block all state-changing escrow operations while active

Planned protocol-fee, governance-transfer, and migration features are explicitly labeled in the escrow docs until their entrypoints land.

```bash
# Run tests (includes 95%+ coverage negative path testing for escrow)
cargo test

# Run escrow performance/gas baseline tests only
cargo test test::performance

# Check formatting
cargo fmt --all -- --check
cargo test -p escrow
cargo test test::performance -p escrow
```

## Escrow Emergency Controls

The escrow contract supports critical-incident response with admin-managed controls:

- `initialize(admin)` (one-time setup)
- `pause()` and `unpause()`
- `activate_emergency_pause()` and `resolve_emergency()`
- `is_paused()` and `is_emergency()`

When paused, all mutating escrow operations (`create_contract`, `deposit_funds`,
`release_milestone`, `issue_reputation`, `cancel_contract`) are blocked with
`ContractPaused`. Read-only queries are never blocked.

See [docs/escrow/emergency-controls.md](docs/escrow/emergency-controls.md) for
the full flag semantics, event model, and security properties.

## Contributing

1. Fork the repo and create a branch from `main`.
2. Make changes; keep tests, lints, and formatting passing:
   - `cargo fmt --all`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test`
   - `cargo build`
3. Open a pull request. CI runs `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build`, and `cargo test` on push/PR to `main`.

## Contract status transition guardrails

Prerequisites:

- Rust 1.75+
- `rustfmt`
- optional Stellar CLI for deployment workflows

Common commands:

## Escrow Closure Finalization

`finalize_contract(contract_id, finalizer)` records immutable close metadata for
contracts in `Completed` or `Disputed` status. The finalizer must be the stored
client, freelancer, or assigned arbiter and must authorize the call. After
finalization, subsequent contract-specific mutating calls fail with
`AlreadyFinalized`.

## CI/CD

On every push and pull request to `main`, GitHub Actions:

- Checks formatting (`cargo fmt --all -- --check`)
- Lints with warnings denied (`cargo clippy --workspace --all-targets -- -D warnings`)
- Builds the workspace (`cargo build`)
- Runs tests (`cargo test`)

Ensure these pass locally before pushing.

## Escrow Performance and Security

- Performance/gas baseline tests for key flows are in `contracts/escrow/src/test/performance.rs`.
- Functional and failure-path coverage is split by module:
  - `contracts/escrow/src/test/flows.rs`
  - `contracts/escrow/src/test/security.rs`
- Contract-specific reviewer docs:
  - `docs/escrow/performance-baselines.md`
  - `docs/escrow/SECURITY.md`

## License

MIT
