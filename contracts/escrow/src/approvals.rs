use crate::ttl::{PENDING_APPROVAL_BUMP_THRESHOLD, PENDING_APPROVAL_TTL_LEDGERS};
use crate::types::{Contract, ContractStatus, DataKey, Error, Milestone, MilestoneApprovals, ReleaseAuthorization};
use soroban_sdk::{Address, Env, Symbol, Vec};

/// Records the caller's approval for a milestone release in temporary storage.
///
/// Approvals are keyed by `(contract_id, milestone_index)` and live in
/// `env.storage().temporary()` with a TTL of `PENDING_APPROVAL_TTL_LEDGERS`
/// (~7 days). Each call resets the TTL. Duplicate approvals from the same
/// party are rejected.
///
/// For the full approve → check → release → clear flow, including per-mode
/// required approvers and fail-closed expiry guarantees, see
/// `docs/escrow/approvals-and-release.md`.
///
/// # Errors
/// * `ContractNotFound` — contract does not exist
/// * `InvalidState` — contract is not `Funded` or `PartiallyFunded`
/// * `IndexOutOfBounds` — milestone index out of range
/// * `MilestoneAlreadyReleased` — milestone already released
/// * `UnauthorizedRole` — caller not authorized under the contract's mode
/// * `AlreadyApproved` — caller already approved this milestone
pub fn approve_milestone(
    env: &Env,
    contract_id: u32,
    milestone_index: u32,
    caller: &Address,
) -> Result<bool, Error> {
    // Authenticate caller
    caller.require_auth();

    // Verify contract exists and is active
    let contract: Contract = env
        .storage()
        .persistent()
        .get(&DataKey::Contract(contract_id))
        .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

    if contract.status != ContractStatus::Funded
        && contract.status != ContractStatus::PartiallyFunded
    {
        env.panic_with_error(Error::InvalidState);
    }

    // Load milestones vector
    let milestone_key = Symbol::new(env, "milestones");
    let milestones: Vec<Milestone> = env
        .storage()
        .persistent()
        .get(&(DataKey::Contract(contract_id), milestone_key))
        .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

    // Verify milestone index bounds
    if milestone_index >= milestones.len() {
        env.panic_with_error(Error::IndexOutOfBounds);
    }

    // Verify milestone not already released
    let milestone = milestones.get(milestone_index).unwrap();
    if milestone.released {
        env.panic_with_error(Error::MilestoneAlreadyReleased);
    }

    // Determine caller's role
    let is_client = *caller == contract.client;
    let is_freelancer = *caller == contract.freelancer;
    let is_arbiter = contract.arbiter.as_ref().map_or(false, |a| *caller == *a);

    // Verify authorization mode allows this caller to approve
    let allowed = match contract.release_authorization {
        ReleaseAuthorization::ClientOnly => is_client,
        ReleaseAuthorization::ArbiterOnly => is_arbiter,
        ReleaseAuthorization::ClientAndArbiter => is_client || is_arbiter,
        ReleaseAuthorization::MultiSig => is_client || is_freelancer,
    };

    if !allowed {
        env.panic_with_error(Error::UnauthorizedRole);
    }

    // Load existing approvals or initialize empty
    let approval_key = DataKey::MilestoneApprovals(contract_id, milestone_index);
    let mut approvals: MilestoneApprovals =
        env.storage()
            .temporary()
            .get(&approval_key)
            .unwrap_or(MilestoneApprovals {
                client_approved: false,
                freelancer_approved: false,
                arbiter_approved: false,
            });

    // Check for duplicate approval and record
    if is_client {
        if approvals.client_approved {
            env.panic_with_error(Error::AlreadyApproved);
        }
        approvals.client_approved = true;
    } else if is_freelancer {
        if approvals.freelancer_approved {
            env.panic_with_error(Error::AlreadyApproved);
        }
        approvals.freelancer_approved = true;
    } else if is_arbiter {
        if approvals.arbiter_approved {
            env.panic_with_error(Error::AlreadyApproved);
        }
        approvals.arbiter_approved = true;
    }

    // Save and extend TTL
    env.storage().temporary().set(&approval_key, &approvals);
    env.storage().temporary().extend_ttl(
        &approval_key,
        PENDING_APPROVAL_BUMP_THRESHOLD,
        PENDING_APPROVAL_TTL_LEDGERS,
    );

    Ok(true)
}

/// Validates milestone release approvals against contract's authorization mode.
///
/// Pure view helper used by `release_milestone` prior to committing release state.
///
/// # Errors
/// * `InsufficientApprovals` — approvals absent, expired, or below the quorum for this mode
pub fn check_approvals(
    env: &Env,
    contract: &Contract,
    contract_id: u32,
    milestone_index: u32,
) -> Result<bool, Error> {
    let approval_key = DataKey::MilestoneApprovals(contract_id, milestone_index);

    // Try to load approvals from temporary storage
    // If TTL has expired, this will return None
    let approvals: Option<MilestoneApprovals> = env.storage().temporary().get(&approval_key);

    // If no approvals exist (or they expired), fail
    let approvals = approvals.ok_or(Error::InsufficientApprovals)?;

    // Check if required approvals are present based on authorization mode
    let sufficient = match contract.release_authorization {
        ReleaseAuthorization::ClientOnly => approvals.client_approved,
        ReleaseAuthorization::ArbiterOnly => approvals.arbiter_approved,
        ReleaseAuthorization::ClientAndArbiter => {
            approvals.client_approved || approvals.arbiter_approved
        }
        ReleaseAuthorization::MultiSig => {
            approvals.client_approved && approvals.freelancer_approved
        }
    };

    if sufficient {
        Ok(true)
    } else {
        Err(Error::InsufficientApprovals)
    }
}

/// Removes the approval record for a milestone after a successful release.
///
/// Called by `release_milestone` immediately after state is committed.
/// Prevents approval reuse and avoids leaving stale entries in temporary
/// storage until natural TTL expiry.
pub fn clear_approvals(env: &Env, contract_id: u32, milestone_index: u32) {
    let approval_key = DataKey::MilestoneApprovals(contract_id, milestone_index);
    env.storage().temporary().remove(&approval_key);
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn test_approve_milestone_client_only() {
        let env = Env::default();
        env.mock_all_auths();

        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);

        let contract = Contract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter: None,
            status: ContractStatus::Funded,
            total_deposited: 1000,
            funded_amount: 1000,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization: ReleaseAuthorization::ClientOnly,
            reputation_issued: false,
        };

        let contract_id = 1u32;
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        let milestones = Vec::from_array(
            &env,
            [Milestone {
                amount: 1000,
                funded_amount: 0,
                released: false,
                refunded: false,
                work_evidence: None,
                refunded_amount: 0,
            }],
        );
        let milestone_key = Symbol::new(&env, "milestones");
        env.storage().persistent().set(
            &(DataKey::Contract(contract_id), milestone_key),
            &milestones,
        );

        // Client approves
        let result = approve_milestone(&env, contract_id, 0, &client);
        assert!(result.is_ok());

        // Check approvals
        let check = check_approvals(&env, &contract, contract_id, 0);
        assert!(check.is_ok());
    }

    #[test]
    fn test_approve_milestone_multisig() {
        let env = Env::default();
        env.mock_all_auths();

        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);

        let contract = Contract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter: None,
            status: ContractStatus::Funded,
            total_deposited: 1000,
            funded_amount: 1000,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization: ReleaseAuthorization::MultiSig,
            reputation_issued: false,
        };

        let contract_id = 1u32;
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        let milestones = Vec::from_array(
            &env,
            [Milestone {
                amount: 1000,
                funded_amount: 0,
                released: false,
                refunded: false,
                work_evidence: None,
                refunded_amount: 0,
            }],
        );
        let milestone_key = Symbol::new(&env, "milestones");
        env.storage().persistent().set(
            &(DataKey::Contract(contract_id), milestone_key),
            &milestones,
        );

        // Only client approves - insufficient
        let result = approve_milestone(&env, contract_id, 0, &client);
        assert!(result.is_ok());

        let check = check_approvals(&env, &contract, contract_id, 0);
        assert_eq!(check, Err(Error::InsufficientApprovals));

        // Freelancer also approves - now sufficient
        let result = approve_milestone(&env, contract_id, 0, &freelancer);
        assert!(result.is_ok());

        let check = check_approvals(&env, &contract, contract_id, 0);
        assert!(check.is_ok());
    }

    #[test]
    fn test_duplicate_approval_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);

        let contract = Contract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter: None,
            status: ContractStatus::Funded,
            total_deposited: 1000,
            funded_amount: 1000,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization: ReleaseAuthorization::ClientOnly,
            reputation_issued: false,
        };

        let contract_id = 1u32;
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        let milestones = Vec::from_array(
            &env,
            [Milestone {
                amount: 1000,
                funded_amount: 0,
                released: false,
                refunded: false,
                work_evidence: None,
                refunded_amount: 0,
            }],
        );
        let milestone_key = Symbol::new(&env, "milestones");
        env.storage().persistent().set(
            &(DataKey::Contract(contract_id), milestone_key),
            &milestones,
        );

        // First approval succeeds
        let result = approve_milestone(&env, contract_id, 0, &client);
        assert!(result.is_ok());

        // Second approval fails
        let result = approve_milestone(&env, contract_id, 0, &client);
        assert_eq!(result, Err(Error::AlreadyApproved));
    }
}
