# Escrow Contract

Rust/Soroban escrow contract for TalentTrust freelancer milestones.

## Implemented Features

- Create a contract between a client and a freelancer.
- Define milestone amounts at creation time.
- Track exact-total or incremental deposits in contract state.
- Release milestones from funded balance.
- Mark the contract `Completed` after the final milestone release.
- Issue one reputation rating for the freelancer after completion.
- Cancel non-completed contracts by the stored client or freelancer.
- Finalize completed or disputed contracts with immutable close metadata.
- Pause and emergency controls managed by a single initialized admin.

## Current Public Entrypoints

- `initialize(admin) -> bool`
- `get_admin() -> Option<Address>`
- `pause() -> bool`
- `unpause() -> bool`
- `is_paused() -> bool`
- `activate_emergency_pause() -> bool`
- `resolve_emergency() -> bool`
- `is_emergency() -> bool`
- `get_mainnet_readiness_info() -> MainnetReadinessInfo`
- `create_contract(client, freelancer, milestone_amounts, deposit_mode) -> u32`
- `deposit_funds(contract_id, amount) -> bool`
- `release_milestone(contract_id, milestone_index) -> bool`
- `issue_reputation(contract_id, caller, freelancer, rating) -> bool`
- `cancel_contract(contract_id, caller) -> bool`
- `finalize_contract(contract_id, finalizer) -> bool`
- `get_contract(contract_id) -> EscrowContractData`
- `get_finalization_record(contract_id) -> Option<FinalizationRecord>`
- `get_reputation(freelancer) -> Option<ReputationRecord>`
- `get_pending_reputation_credits(freelancer) -> u32`

## Important Integration Notes

- `release_milestone` currently validates milestone state and available balance,
  but it does not authenticate a client or arbiter caller.
- The contract tracks escrow balances in state only. Token custody and transfers
  are not implemented in `contracts/escrow/src/lib.rs`.
- `withdraw_leftover`, protocol fee accounting, protocol
  fee withdrawal, two-step admin transfer, dispute/refund flows, and
  `migrate_state` are not live entrypoints.

## Planned Features

- Two-step admin transfer:
  [#318](https://github.com/Talenttrust/Talenttrust-Contracts/issues/318)
- Protocol fee deduction and withdrawal:
  [#313](https://github.com/Talenttrust/Talenttrust-Contracts/issues/313),
  [#314](https://github.com/Talenttrust/Talenttrust-Contracts/issues/314)
- Final contract closure metadata:
  [#320](https://github.com/Talenttrust/Talenttrust-Contracts/issues/320)
- `migrate_state` / `StateV1` / `StateV2` flow:
  [#341](https://github.com/Talenttrust/Talenttrust-Contracts/issues/341)
