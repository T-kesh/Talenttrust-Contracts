# Mainnet Readiness

`get_mainnet_readiness_info() -> MainnetReadinessInfo` is implemented in
`contracts/escrow/src/lib.rs` and is read-only.

## Returned Fields

| Field | Meaning |
| --- | --- |
| `initialized` | Mirrors the readiness checklist flag set by `initialize(admin)`. |
| `governed_params_set` | Mirrors the readiness checklist flag. No live entrypoint currently sets it to `true`. |
| `emergency_controls_enabled` | Set to `true` by `activate_emergency_pause` or `resolve_emergency`. |
| `caps_set` | Derived from `MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS > 0`. |
| `protocol_version` | Compile-time `MAINNET_PROTOCOL_VERSION`. |
| `max_escrow_total_stroops` | Compile-time `MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS`. |

## Current Readiness Interpretation

A fresh deployment can set `initialized` by calling `initialize(admin)` and can
exercise emergency controls to set `emergency_controls_enabled`. The
`governed_params_set` field remains `false` until governed parameter entrypoints
are implemented.

Governed parameter setter/readiness wiring is planned in
[#323](https://github.com/Talenttrust/Talenttrust-Contracts/issues/323).

## Residual Risks

- Asset movement is not implemented in `lib.rs`; escrow balances are state
  accounting only.
- Release authorization is not implemented in `release_milestone`.
- Protocol fee accounting and withdrawal are planned in
  [#313](https://github.com/Talenttrust/Talenttrust-Contracts/issues/313) and
  [#314](https://github.com/Talenttrust/Talenttrust-Contracts/issues/314).
