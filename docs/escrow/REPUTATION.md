# Reputation Credential Issuance

The Escrow contract issues reputation credentials (ratings) to freelancers after a contract reaches `Completed` status.

## Validation Rules

1. **Client authorization:** Only the contract client may call `issue_reputation`. Unauthorized callers fail with `UnauthorizedRole`.
2. **Freelancer match:** The supplied freelancer address must match the contract's stored freelancer. Mismatches fail with `FreelancerMismatch`.
3. **Contract completion gating:** Reputation can only be issued after the contract is `Completed`. Non-completed contracts fail with `NotCompleted`.
4. **Rating bounds:** Ratings must be between `1` and `5` inclusive. Values outside this range fail with `InvalidRating`.
5. **Duplicate issuance protection:** Reputation may only be issued once per contract. Subsequent attempts fail with `ReputationAlreadyIssued`.

## Reputation Aggregation

Successful issuance updates the freelancer's aggregate `ReputationRecord`:
- `completed_contracts` increments by `1`
- `total_rating` increases by the rating value
- `last_rating` is set to the most recent rating

Pending reputation credits are also decremented on success.

## Test Coverage

The escrow test suite now includes dedicated coverage for the `issue_reputation` negative paths in `contracts/escrow/src/test/reputation.rs`.
- unauthorized caller
- freelancer mismatch
- non-completed contract
- invalid rating bounds
- duplicate issuance
- verified reputation aggregation and pending credit decrement on success

## Security Assumptions

- **Access Control:** `issue_reputation` requires client authentication.
- **Contract Completion:** Only `Completed` contracts are eligible for reputation issuance.
- **Duplicate issuance guard:** Repeat issuance is blocked by a stored `ReputationIssued` flag.
- **Aggregate consistency:** Reputation totals and pending credits are updated atomically.
