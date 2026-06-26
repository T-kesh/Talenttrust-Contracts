# Milestone Approval and Release Flow

Releasing a milestone is a two-step process. First, authorized parties call
`approve_milestone_release` to record their approval in temporary storage.
Then an authorized caller invokes `release_milestone`, which checks the
approvals, marks the milestone released, and clears the approval record.

**Source files:**
- `contracts/escrow/src/approvals.rs` — `approve_milestone`, `check_approvals`, `clear_approvals`
- `contracts/escrow/src/lib.rs` — `approve_milestone_release`, `release_milestone`, `get_milestone_approvals`
- `contracts/escrow/src/test/release.rs` — integration tests

---

## State diagram

```
[Funded contract, unreleased milestone]
          │
          ▼
  approve_milestone_release(caller)
  ┌──────────────────────────────────────────────┐
  │ authenticate caller                          │
  │ check caller role matches the mode           │
  │ reject duplicate approvals                   │
  │ write MilestoneApprovals to temp storage     │
  │ extend TTL → PENDING_APPROVAL_TTL_LEDGERS    │
  └──────────────────────────────────────────────┘
          │  (repeat until quorum met)
          ▼
  release_milestone(caller)
  ┌──────────────────────────────────────────────┐
  │ authenticate caller                          │
  │ check contract is Funded, not finalized      │
  │ check caller role matches the mode           │
  │ check_approvals → reads temp storage         │
  │   None (expired or never set) → panic        │
  │   insufficient → panic                       │
  │ mark milestone.released = true               │
  │ contract.released_amount += amount           │
  │ accumulate protocol fee if enabled           │
  │ clear_approvals → remove from temp storage   │
  │ if all milestones done → status = Completed  │
  └──────────────────────────────────────────────┘
```

---

## ReleaseAuthorization modes

| Mode               | Who may approve                    | Approval check                                  | Who may release   |
|--------------------|------------------------------------|-------------------------------------------------|-------------------|
| `ClientOnly`       | client                             | `client_approved`                               | client            |
| `ArbiterOnly`      | arbiter                            | `arbiter_approved`                              | arbiter           |
| `ClientAndArbiter` | client or arbiter                  | `client_approved \|\| arbiter_approved`         | client or arbiter |
| `MultiSig`         | client **and** freelancer (both)   | `client_approved && freelancer_approved`        | client or freelancer |

### MultiSig

Both client and freelancer must call `approve_milestone_release` before
release is possible. Either of them may then call `release_milestone`.
The arbiter has no role in `MultiSig` — calling `approve_milestone_release`
as arbiter returns `UnauthorizedRole`.

---

## TTL and fail-closed expiry

Approvals live in `env.storage().temporary()` under
`DataKey::MilestoneApprovals(contract_id, milestone_index)`.

```
PENDING_APPROVAL_TTL_LEDGERS   = 17_280 ledgers/day × 7 = 120_960 (~7 days)
PENDING_APPROVAL_BUMP_THRESHOLD = 17_280 ledgers/day × 1 = 17_280  (~1 day)
```

Each `approve_milestone_release` call resets the TTL. If the window closes
before release, Soroban evicts the entry automatically.

**Fail-closed:** `check_approvals` reads the entry with
`env.storage().temporary().get(...)`. Soroban returns `None` for both
"never written" and "TTL elapsed". Both cases produce `InsufficientApprovals`
and the release panics. An expired approval cannot silently authorize a release.

After a successful release, `clear_approvals` explicitly removes the entry
rather than waiting for natural TTL expiry.

---

## Querying live approvals

`get_milestone_approvals(contract_id, milestone_index) -> Option<MilestoneApprovals>`

Returns `None` when no record exists or TTL has elapsed. `Some` with all
fields `false` and `None` are equally invalid — neither unblocks release.
