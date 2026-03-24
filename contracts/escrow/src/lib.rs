#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, contracterror, symbol_short, Address, Env, Map, String,
    Symbol, Vec,
};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Lifecycle state of an escrow contract.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    Created = 0,
    Funded = 1,
    Completed = 2,
    Disputed = 3,
}

/// Storage keys used to address persistent contract data.
#[contracttype]
#[derive(Clone, Debug)]
pub enum DataKey {
    /// Full state for an escrow identified by its numeric ID.
    EscrowState(u32),
    /// Immutable dispute record for an escrow identified by its numeric ID.
    Dispute(u32),
}

/// Typed errors returned by dispute-related contract functions.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisputeError {
    /// The escrow contract was not found in storage.
    NotFound = 1,
    /// The caller is not the client or freelancer of this escrow.
    Unauthorized = 2,
    /// The escrow status does not allow dispute initiation (e.g. `Created`).
    InvalidStatus = 3,
    /// A dispute record already exists for this escrow.
    AlreadyDisputed = 4,
}

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

/// A single payment milestone within an escrow.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Milestone {
    /// Amount in stroops allocated to this milestone.
    pub amount: i128,
    /// Whether the milestone payment has been released to the freelancer.
    pub released: bool,
    pub approved_by: Option<Address>,
    pub approval_timestamp: Option<u64>,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseAuthorization {
    ClientOnly = 0,
    ClientAndArbiter = 1,
    ArbiterOnly = 2,
    MultiSig = 3,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct EscrowContract {
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub milestones: Vec<Milestone>,
    pub status: ContractStatus,
    pub release_auth: ReleaseAuthorization,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Approval {
    None = 0,
    Client = 1,
    Arbiter = 2,
    Both = 3,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct MilestoneApproval {
    pub milestone_id: u32,
    pub approvals: Map<Address, bool>,
    pub required_approvals: u32,
    pub approval_status: Approval,
}

/// Full on-chain state of an escrow contract.
#[contracttype]
#[derive(Clone, Debug)]
pub struct EscrowState {
    /// Address of the client who created and funded the escrow.
    pub client: Address,
    /// Address of the freelancer who will receive milestone payments.
    pub freelancer: Address,
    /// Current lifecycle status of the escrow.
    pub status: ContractStatus,
    /// Ordered list of payment milestones.
    pub milestones: Vec<Milestone>,
}

/// Immutable record created when a dispute is initiated.
/// Written once to persistent storage and never overwritten.
#[contracttype]
#[derive(Clone, Debug)]
pub struct DisputeRecord {
    /// The address (client or freelancer) that initiated the dispute.
    pub initiator: Address,
    /// A short human-readable reason for the dispute.
    pub reason: String,
    /// Ledger timestamp (seconds since Unix epoch) at the moment the dispute was recorded.
    pub timestamp: u64,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct Escrow;

#[contractimpl]
impl Escrow {
    /// Create a new escrow contract with milestone release authorization
    ///
    /// # Arguments
    /// * `client` - Address of the client who funds the escrow
    /// * `freelancer` - Address of the freelancer who receives payments
    /// * `arbiter` - Optional arbiter address for dispute resolution
    /// * `milestone_amounts` - Vector of milestone payment amounts
    /// * `release_auth` - Authorization scheme for milestone releases
    ///
    /// # Returns
    /// Contract ID for the newly created escrow
    ///
    /// # Errors
    /// Panics if:
    /// - Milestone amounts vector is empty
    /// - Any milestone amount is zero or negative
    /// - Client and freelancer addresses are the same
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        arbiter: Option<Address>,
        milestone_amounts: Vec<i128>,
        release_auth: ReleaseAuthorization,
    ) -> u32 {
        // Validate inputs
        if milestone_amounts.is_empty() {
            panic!("At least one milestone required");
        }

        if client == freelancer {
            panic!("Client and freelancer cannot be the same address");
        }

        // Validate milestone amounts
        for i in 0..milestone_amounts.len() {
            let amount = milestone_amounts.get(i).unwrap();
            if amount <= 0 {
                panic!("Milestone amounts must be positive");
            }
        }

        // Create milestones
        let mut milestones = Vec::new(&env);
        for i in 0..milestone_amounts.len() {
            milestones.push_back(Milestone {
                amount: milestone_amounts.get(i).unwrap(),
                released: false,
                approved_by: None,
                approval_timestamp: None,
            });
        }

        // Create contract
        let contract_data = EscrowContract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter,
            milestones,
            status: ContractStatus::Created,
            release_auth,
            created_at: env.ledger().timestamp(),
        };

        // Generate contract ID (in real implementation, this would use proper storage)
        let contract_id = env.ledger().sequence();

        // Store contract data (simplified for this implementation)
        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &contract_data);

        contract_id
    }

    /// Deposit funds into escrow. Only the client may call this.
    ///
    /// # Arguments
    /// * `contract_id` - ID of the escrow contract
    /// * `amount` - Amount to deposit (must equal total milestone amounts)
    ///
    /// # Returns
    /// true if deposit successful
    ///
    /// # Errors
    /// Panics if:
    /// - Caller is not the client
    /// - Contract is not in Created status
    /// - Amount doesn't match total milestone amounts
    pub fn deposit_funds(env: Env, _contract_id: u32, caller: Address, amount: i128) -> bool {
        caller.require_auth();

        // In real implementation, retrieve contract from storage
        // For now, we'll use a simplified approach
        let contract: EscrowContract = env
            .storage()
            .persistent()
            .get(&symbol_short!("contract"))
            .unwrap_or_else(|| panic!("Contract not found"));

        // Verify caller is client
        if caller != contract.client {
            panic!("Only client can deposit funds");
        }

        // Verify contract status
        if contract.status != ContractStatus::Created {
            panic!("Contract must be in Created status to deposit funds");
        }

        // Calculate total required amount
        let mut total_required = 0i128;
        for i in 0..contract.milestones.len() {
            total_required += contract.milestones.get(i).unwrap().amount;
        }

        if amount != total_required {
            panic!("Deposit amount must equal total milestone amounts");
        }

        // Update contract status to Funded
        let mut updated_contract = contract;
        updated_contract.status = ContractStatus::Funded;
        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &updated_contract);

        true
    }

    /// Approve a milestone for release with proper authorization
    ///
    /// # Arguments
    /// * `contract_id` - ID of the escrow contract
    /// * `milestone_id` - ID of the milestone to approve
    ///
    /// # Returns
    /// true if approval successful
    ///
    /// # Errors
    /// Panics if:
    /// - Caller is not authorized to approve
    /// - Contract is not in Funded status
    /// - Milestone ID is invalid
    /// - Milestone already released
    /// - Milestone already approved by this caller
    pub fn approve_milestone_release(
        env: Env,
        _contract_id: u32,
        caller: Address,
        milestone_id: u32,
    ) -> bool {
        caller.require_auth();

        // Retrieve contract
        let mut contract: EscrowContract = env
            .storage()
            .persistent()
            .get(&symbol_short!("contract"))
            .unwrap_or_else(|| panic!("Contract not found"));

        // Verify contract status
        if contract.status != ContractStatus::Funded {
            panic!("Contract must be in Funded status to approve milestones");
        }

        // Validate milestone ID
        if milestone_id >= contract.milestones.len() {
            panic!("Invalid milestone ID");
        }

        let milestone = contract.milestones.get(milestone_id).unwrap();

        // Check if milestone already released
        if milestone.released {
            panic!("Milestone already released");
        }

        // Check authorization based on release_auth scheme
        let is_authorized = match contract.release_auth {
            ReleaseAuthorization::ClientOnly => caller == contract.client,
            ReleaseAuthorization::ArbiterOnly => {
                contract.arbiter.clone().map_or(false, |a| caller == a)
            }
            ReleaseAuthorization::ClientAndArbiter => {
                caller == contract.client || contract.arbiter.clone().map_or(false, |a| caller == a)
            }
            ReleaseAuthorization::MultiSig => {
                // For multi-sig, both client and arbiter must approve
                // This function handles individual approval
                caller == contract.client || contract.arbiter.clone().map_or(false, |a| caller == a)
            }
        };

        if !is_authorized {
            panic!("Caller not authorized to approve milestone release");
        }

        // Check if already approved by this caller
        if milestone
            .approved_by
            .clone()
            .map_or(false, |addr| addr == caller)
        {
            panic!("Milestone already approved by this address");
        }

        // Update milestone approval
        let mut updated_milestone = milestone;
        updated_milestone.approved_by = Some(caller);
        updated_milestone.approval_timestamp = Some(env.ledger().timestamp());

        // Update contract
        contract.milestones.set(milestone_id, updated_milestone);
        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &contract);

        true
    }

    /// Release a milestone payment to the freelancer after proper authorization
    ///
    /// # Arguments
    /// * `contract_id` - ID of the escrow contract
    /// * `milestone_id` - ID of the milestone to release
    ///
    /// # Returns
    /// true if release successful
    ///
    /// # Errors
    /// Panics if:
    /// - Contract is not in Funded status
    /// - Milestone ID is invalid
    /// - Milestone already released
    /// - Insufficient approvals based on authorization scheme
    pub fn release_milestone(
        env: Env,
        _contract_id: u32,
        caller: Address,
        milestone_id: u32,
    ) -> bool {
        caller.require_auth();
        // Retrieve contract
        let mut contract: EscrowContract = env
            .storage()
            .persistent()
            .get(&symbol_short!("contract"))
            .unwrap_or_else(|| panic!("Contract not found"));

        // Verify contract status
        if contract.status != ContractStatus::Funded {
            panic!("Contract must be in Funded status to release milestones");
        }

        // Validate milestone ID
        if milestone_id >= contract.milestones.len() {
            panic!("Invalid milestone ID");
        }

        let milestone = contract.milestones.get(milestone_id).unwrap();

        // Check if milestone already released
        if milestone.released {
            panic!("Milestone already released");
        }

        // Check if milestone has sufficient approvals
        let has_sufficient_approval = match contract.release_auth {
            ReleaseAuthorization::ClientOnly => milestone
                .approved_by
                .clone()
                .map_or(false, |addr| addr == contract.client),
            ReleaseAuthorization::ArbiterOnly => {
                contract.arbiter.clone().map_or(false, |arbiter| {
                    milestone
                        .approved_by
                        .clone()
                        .map_or(false, |addr| addr == arbiter)
                })
            }
            ReleaseAuthorization::ClientAndArbiter => {
                milestone.approved_by.clone().map_or(false, |addr| {
                    addr == contract.client
                        || contract
                            .arbiter
                            .clone()
                            .map_or(false, |arbiter| addr == arbiter)
                })
            }
            ReleaseAuthorization::MultiSig => {
                // For multi-sig, we'd need to track multiple approvals
                // Simplified: require client approval for now
                milestone
                    .approved_by
                    .clone()
                    .map_or(false, |addr| addr == contract.client)
            }
        };

        if !has_sufficient_approval {
            panic!("Insufficient approvals for milestone release");
        }

        // Release milestone
        let mut updated_milestone = milestone;
        updated_milestone.released = true;

        // Update contract
        contract.milestones.set(milestone_id, updated_milestone);

        // Check if all milestones are released
        let all_released = contract.milestones.iter().all(|m| m.released);
        if all_released {
            contract.status = ContractStatus::Completed;
        }

        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &contract);

        // In real implementation, transfer funds to freelancer
        // For now, we'll just mark as released

        true
    }

    /// Issue a reputation credential for the freelancer after contract completion.
    pub fn issue_reputation(_env: Env, _freelancer: Address, _rating: i128) -> bool {
        true
    }

    /// Hello-world style function for testing and CI.
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }

    /// Initiate a dispute on an existing escrow.
    ///
    /// The `initiator` must be either the client or the freelancer of the
    /// escrow. The escrow must be in `Funded` or `Completed` status. A
    /// `DisputeRecord` is written to persistent storage exactly once.
    ///
    /// # Arguments
    /// * `contract_id` – Numeric ID of the escrow to dispute.
    /// * `initiator`   – Address of the party raising the dispute.
    /// * `reason`      – Short human-readable description of the dispute.
    ///
    /// # Errors
    /// * `DisputeError::NotFound`       – No escrow with `contract_id` exists.
    /// * `DisputeError::Unauthorized`   – `initiator` is not client or freelancer.
    /// * `DisputeError::InvalidStatus`  – Escrow is in `Created` status.
    /// * `DisputeError::AlreadyDisputed`– A dispute record already exists.
    pub fn initiate_dispute(
        env: Env,
        contract_id: u32,
        initiator: Address,
        reason: String,
    ) -> Result<(), DisputeError> {
        // 1. Enforce Soroban-level authorization before any state read/write.
        initiator.require_auth();

        // 2. Load escrow state.
        let mut state: EscrowState = env
            .storage()
            .persistent()
            .get(&DataKey::EscrowState(contract_id))
            .ok_or(DisputeError::NotFound)?;

        // 3. Validate caller is a party to this escrow.
        if initiator != state.client && initiator != state.freelancer {
            return Err(DisputeError::Unauthorized);
        }

        // 4. Validate status allows dispute initiation.
        match state.status {
            ContractStatus::Created => return Err(DisputeError::InvalidStatus),
            ContractStatus::Disputed => return Err(DisputeError::AlreadyDisputed),
            ContractStatus::Funded | ContractStatus::Completed => {}
        }

        // 5. Guard against overwriting an existing dispute record.
        if env
            .storage()
            .persistent()
            .has(&DataKey::Dispute(contract_id))
        {
            return Err(DisputeError::AlreadyDisputed);
        }

        // 6. Transition status and persist updated state.
        state.status = ContractStatus::Disputed;
        env.storage()
            .persistent()
            .set(&DataKey::EscrowState(contract_id), &state);

        // 7. Write immutable dispute record.
        let record = DisputeRecord {
            initiator,
            reason,
            timestamp: env.ledger().timestamp(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::Dispute(contract_id), &record);

        Ok(())
    }

    /// Retrieve the dispute record for an escrow, if one exists.
    ///
    /// # Arguments
    /// * `contract_id` – Numeric ID of the escrow to query.
    ///
    /// # Returns
    /// `Some(DisputeRecord)` if a dispute has been initiated, `None` otherwise.
    pub fn get_dispute(env: Env, contract_id: u32) -> Option<DisputeRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::Dispute(contract_id))
    }
}

#[cfg(test)]
mod test;
