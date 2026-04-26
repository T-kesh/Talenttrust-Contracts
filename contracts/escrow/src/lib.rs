#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, String, Symbol, Vec,
};

/// Maximum length (in bytes) allowed for a milestone title string.
pub const MAX_SCHEDULE_TITLE_LEN: u32 = 128;

/// Maximum length (in bytes) allowed for a milestone description string.
pub const MAX_SCHEDULE_DESCRIPTION_LEN: u32 = 512;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    NextContractId,
    Contract(u32),
    Reputation(Address),
    PendingReputationCredits(Address),
    MilestoneSchedule(u32, u32),
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseAuthorization {
    ClientOnly,
    ArbiterOnly,
    ClientAndArbiter,
    MultiSig,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    Created = 0,
    Funded = 1,
    Completed = 2,
    Disputed = 3,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EscrowError {
    InvalidContractId = 1,
    InvalidMilestoneId = 2,
    AmountMustBePositive = 3,
    InvalidRating = 4,
    EmptyMilestones = 5,
    InvalidParticipants = 6,
    InvalidMilestoneAmount = 7,
    InvalidAmount = 8,
    FundingExceedsRequired = 9,
    InvalidState = 10,
    InsufficientEscrowBalance = 11,
    MilestoneAlreadyReleased = 12,
    MilestoneNotFound = 13,
    ReputationAlreadyIssued = 14,
    ContractNotFound = 15,
    ScheduleDueDateInPast = 16,
    ScheduleDatesNotMonotonic = 17,
    ScheduleStringTooLong = 18,
    ScheduleImmutableAfterRelease = 19,
    ScheduleInvalidMilestoneIndex = 20,
}

/// Optional scheduling information attached to a single milestone.
///
/// The `due_date` is now policy-enforced by the contract: approval and release
/// are allowed at the exact deadline, but once ledger time moves strictly past
/// the deadline the contract auto-transitions to `Disputed`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneSchedule {
    pub due_date: Option<u64>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub updated_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    pub amount: i128,
    pub released: bool,
    pub approved_by: Option<Address>,
    pub approval_timestamp: Option<u64>,
    /// Deterministic deadline used for timeout enforcement.
    pub deadline_at: Option<u64>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowContractData {
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub milestones: Vec<Milestone>,
    pub total_amount: i128,
    pub funded_amount: i128,
    pub released_amount: i128,
    pub released_milestones: u32,
    pub status: ContractStatus,
    pub release_auth: ReleaseAuthorization,
    pub reputation_issued: bool,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationRecord {
    pub completed_contracts: u32,
    pub total_rating: i128,
    pub last_rating: i128,
}

#[contract]
pub struct Escrow;

impl Escrow {
    fn next_contract_id(env: &Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::NextContractId)
            .unwrap_or(1)
    }

    fn load_contract(env: &Env, contract_id: u32) -> EscrowContractData {
        env.storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| panic!("contract not found"))
    }

    fn save_contract(env: &Env, contract_id: u32, contract: &EscrowContractData) {
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), contract);
    }

    fn add_pending_reputation_credit(env: &Env, freelancer: &Address) {
        let key = DataKey::PendingReputationCredits(freelancer.clone());
        let current: u32 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(current + 1));
    }

    fn validate_single_schedule(env: &Env, schedule: &MilestoneSchedule) -> Result<(), EscrowError> {
        if let Some(ref title) = schedule.title {
            if title.len() > MAX_SCHEDULE_TITLE_LEN {
                return Err(EscrowError::ScheduleStringTooLong);
            }
        }
        if let Some(ref description) = schedule.description {
            if description.len() > MAX_SCHEDULE_DESCRIPTION_LEN {
                return Err(EscrowError::ScheduleStringTooLong);
            }
        }
        if let Some(due_date) = schedule.due_date {
            if due_date <= env.ledger().timestamp() {
                return Err(EscrowError::ScheduleDueDateInPast);
            }
        }
        Ok(())
    }

    fn validate_schedule_sequence(
        env: &Env,
        schedules: &Vec<Option<MilestoneSchedule>>,
    ) -> Result<(), EscrowError> {
        let mut last_due: Option<u64> = None;
        let mut idx = 0u32;
        while idx < schedules.len() {
            if let Some(schedule) = schedules.get(idx).unwrap() {
                Self::validate_single_schedule(env, &schedule)?;
                if let Some(due_date) = schedule.due_date {
                    if let Some(previous_due) = last_due {
                        if due_date <= previous_due {
                            return Err(EscrowError::ScheduleDatesNotMonotonic);
                        }
                    }
                    last_due = Some(due_date);
                }
            }
            idx += 1;
        }
        Ok(())
    }

    fn ensure_release_actor(contract: &EscrowContractData, caller: &Address) {
        let authorized = match contract.release_auth {
            ReleaseAuthorization::ClientOnly => *caller == contract.client,
            ReleaseAuthorization::ArbiterOnly => contract
                .arbiter
                .as_ref()
                .is_some_and(|arbiter| arbiter == caller),
            ReleaseAuthorization::ClientAndArbiter | ReleaseAuthorization::MultiSig => {
                *caller == contract.client
                    || contract
                        .arbiter
                        .as_ref()
                        .is_some_and(|arbiter| arbiter == caller)
            }
        };
        if !authorized {
            panic!("Caller not authorized to approve milestone release");
        }
    }

    fn ensure_release_approval(contract: &EscrowContractData, milestone: &Milestone) {
        let has_approval = match contract.release_auth {
            ReleaseAuthorization::ClientOnly | ReleaseAuthorization::MultiSig => milestone
                .approved_by
                .as_ref()
                .is_some_and(|addr| *addr == contract.client),
            ReleaseAuthorization::ArbiterOnly => contract.arbiter.as_ref().is_some_and(|arbiter| {
                milestone
                    .approved_by
                    .as_ref()
                    .is_some_and(|addr| *addr == *arbiter)
            }),
            ReleaseAuthorization::ClientAndArbiter => milestone.approved_by.as_ref().is_some_and(
                |addr| {
                    *addr == contract.client
                        || contract
                            .arbiter
                            .as_ref()
                            .is_some_and(|arbiter| *addr == *arbiter)
                },
            ),
        };
        if !has_approval {
            panic!("Insufficient approvals for milestone release");
        }
    }

    fn milestone_expired(env: &Env, milestone: &Milestone) -> bool {
        milestone
            .deadline_at
            .is_some_and(|deadline_at| env.ledger().timestamp() > deadline_at)
    }

    fn ensure_milestone_not_expired(
        env: &Env,
        contract_id: u32,
        contract: &mut EscrowContractData,
        milestone_id: u32,
    ) {
        let milestone = contract.milestones.get(milestone_id).unwrap();
        if Self::milestone_expired(env, &milestone) {
            if contract.status == ContractStatus::Funded {
                contract.status = ContractStatus::Disputed;
                Self::save_contract(env, contract_id, contract);
            }
            panic!("Milestone deadline has expired; contract moved to Disputed");
        }
    }

    fn all_milestones_released(milestones: &Vec<Milestone>) -> bool {
        let mut idx = 0u32;
        while idx < milestones.len() {
            if !milestones.get(idx).unwrap().released {
                return false;
            }
            idx += 1;
        }
        true
    }

    fn store_schedule(
        env: &Env,
        contract_id: u32,
        milestone_idx: u32,
        schedule: &MilestoneSchedule,
    ) {
        let stamped = MilestoneSchedule {
            due_date: schedule.due_date,
            title: schedule.title.clone(),
            description: schedule.description.clone(),
            updated_at: env.ledger().timestamp(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::MilestoneSchedule(contract_id, milestone_idx), &stamped);
    }
}

#[contractimpl]
impl Escrow {
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        arbiter: Option<Address>,
        milestone_amounts: Vec<i128>,
        release_auth: ReleaseAuthorization,
        schedules: Vec<Option<MilestoneSchedule>>,
    ) -> u32 {
        client.require_auth();

        if milestone_amounts.is_empty() {
            panic!("At least one milestone required");
        }
        if client == freelancer {
            panic!("Client and freelancer cannot be the same address");
        }

        if !schedules.is_empty() {
            if schedules.len() != milestone_amounts.len() {
                panic!("schedules length must match milestone_amounts length");
            }
            match Self::validate_schedule_sequence(&env, &schedules) {
                Ok(()) => {}
                Err(EscrowError::ScheduleDatesNotMonotonic) => {
                    panic!("milestone due dates must be strictly increasing");
                }
                Err(_) => panic!("invalid schedule metadata"),
            }
        }

        let mut total_amount = 0i128;
        let mut milestones = Vec::new(&env);
        let mut idx = 0u32;
        while idx < milestone_amounts.len() {
            let amount = milestone_amounts.get(idx).unwrap();
            if amount <= 0 {
                panic!("Milestone amounts must be positive");
            }

            let deadline_at = if schedules.is_empty() {
                None
            } else {
                schedules.get(idx).unwrap().and_then(|schedule| schedule.due_date)
            };

            milestones.push_back(Milestone {
                amount,
                released: false,
                approved_by: None,
                approval_timestamp: None,
                deadline_at,
            });
            total_amount += amount;
            idx += 1;
        }

        let contract_id = Self::next_contract_id(&env);
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &(contract_id + 1));

        let contract = EscrowContractData {
            client,
            freelancer,
            arbiter,
            milestones,
            total_amount,
            funded_amount: 0,
            released_amount: 0,
            released_milestones: 0,
            status: ContractStatus::Created,
            release_auth,
            reputation_issued: false,
            created_at: env.ledger().timestamp(),
        };
        Self::save_contract(&env, contract_id, &contract);

        if !schedules.is_empty() {
            let mut schedule_idx = 0u32;
            while schedule_idx < schedules.len() {
                if let Some(schedule) = schedules.get(schedule_idx).unwrap() {
                    Self::store_schedule(&env, contract_id, schedule_idx, &schedule);
                }
                schedule_idx += 1;
            }
        }

        contract_id
    }

    pub fn deposit_funds(env: Env, contract_id: u32, caller: Address, amount: i128) -> bool {
        caller.require_auth();

        if amount <= 0 {
            panic!("deposit amount must be positive");
        }

        let mut contract = Self::load_contract(&env, contract_id);
        if caller != contract.client {
            panic!("Only client can deposit funds");
        }
        if contract.status != ContractStatus::Created {
            panic!("Contract must be in Created status to deposit funds");
        }
        if contract.funded_amount + amount > contract.total_amount {
            panic!("Deposit amount must equal total milestone amounts");
        }

        contract.funded_amount += amount;
        if contract.funded_amount == contract.total_amount {
            contract.status = ContractStatus::Funded;
        }
        Self::save_contract(&env, contract_id, &contract);
        true
    }

    pub fn approve_milestone_release(
        env: Env,
        contract_id: u32,
        caller: Address,
        milestone_id: u32,
    ) -> bool {
        caller.require_auth();

        let mut contract = Self::load_contract(&env, contract_id);
        if contract.status != ContractStatus::Funded {
            panic!("Contract must be in Funded status to approve milestones");
        }
        if milestone_id >= contract.milestones.len() {
            panic!("Invalid milestone ID");
        }

        Self::ensure_release_actor(&contract, &caller);

        let milestone = contract.milestones.get(milestone_id).unwrap();
        if milestone.released {
            panic!("Milestone already released");
        }
        if milestone.approved_by.as_ref().is_some_and(|addr| *addr == caller) {
            panic!("Milestone already approved by this address");
        }

        Self::ensure_milestone_not_expired(&env, contract_id, &mut contract, milestone_id);

        let mut updated = milestone;
        updated.approved_by = Some(caller);
        updated.approval_timestamp = Some(env.ledger().timestamp());
        contract.milestones.set(milestone_id, updated);
        Self::save_contract(&env, contract_id, &contract);
        true
    }

    pub fn release_milestone(
        env: Env,
        contract_id: u32,
        caller: Address,
        milestone_id: u32,
    ) -> bool {
        caller.require_auth();

        let mut contract = Self::load_contract(&env, contract_id);
        if contract.status != ContractStatus::Funded {
            panic!("Contract must be in Funded status to release milestones");
        }
        if milestone_id >= contract.milestones.len() {
            panic!("Invalid milestone ID");
        }

        Self::ensure_release_actor(&contract, &caller);

        let milestone = contract.milestones.get(milestone_id).unwrap();
        if milestone.released {
            panic!("Milestone already released");
        }

        Self::ensure_milestone_not_expired(&env, contract_id, &mut contract, milestone_id);
        Self::ensure_release_approval(&contract, &milestone);

        if contract.funded_amount - contract.released_amount < milestone.amount {
            panic!("Insufficient escrow balance");
        }

        let amount = milestone.amount;
        let mut updated = milestone;
        updated.released = true;
        contract.milestones.set(milestone_id, updated);
        contract.released_amount += amount;
        contract.released_milestones += 1;

        if Self::all_milestones_released(&contract.milestones) {
            contract.status = ContractStatus::Completed;
            Self::add_pending_reputation_credit(&env, &contract.freelancer);
        }

        Self::save_contract(&env, contract_id, &contract);
        true
    }

    /// Resolves a timeout-driven dispute after schedule metadata has been
    /// updated so that no unreleased milestone is still expired.
    ///
    /// Policy:
    /// - if an arbiter exists, only the arbiter can resolve;
    /// - otherwise the client is the resolver of last resort.
    pub fn resolve_dispute(env: Env, contract_id: u32, caller: Address) -> bool {
        caller.require_auth();

        let mut contract = Self::load_contract(&env, contract_id);
        if contract.status != ContractStatus::Disputed {
            panic!("Contract must be in Disputed status to resolve disputes");
        }

        let authorized = match contract.arbiter.as_ref() {
            Some(arbiter) => caller == *arbiter,
            None => caller == contract.client,
        };
        if !authorized {
            panic!("Caller not authorized to resolve dispute");
        }

        let mut idx = 0u32;
        while idx < contract.milestones.len() {
            let milestone = contract.milestones.get(idx).unwrap();
            if !milestone.released && Self::milestone_expired(&env, &milestone) {
                panic!("dispute cannot be resolved while milestone deadlines remain expired");
            }
            idx += 1;
        }

        contract.status = if Self::all_milestones_released(&contract.milestones) {
            ContractStatus::Completed
        } else {
            ContractStatus::Funded
        };
        Self::save_contract(&env, contract_id, &contract);
        true
    }

    pub fn set_milestone_schedule(
        env: Env,
        contract_id: u32,
        milestone_idx: u32,
        schedule: MilestoneSchedule,
    ) -> bool {
        let mut contract = Self::load_contract(&env, contract_id);
        contract.client.require_auth();

        if milestone_idx >= contract.milestones.len() {
            panic!("milestone index out of range");
        }
        if contract.milestones.get(milestone_idx).unwrap().released {
            panic!("schedule is immutable after milestone release");
        }

        Self::validate_single_schedule(&env, &schedule)
            .unwrap_or_else(|_| panic!("invalid schedule metadata"));

        if milestone_idx > 0 {
            if let Some(previous) = env
                .storage()
                .persistent()
                .get::<_, MilestoneSchedule>(&DataKey::MilestoneSchedule(contract_id, milestone_idx - 1))
            {
                if let (Some(previous_due), Some(new_due)) = (previous.due_date, schedule.due_date) {
                    if new_due <= previous_due {
                        panic!("milestone due dates must be strictly increasing");
                    }
                }
            }
        }

        if milestone_idx + 1 < contract.milestones.len() {
            if let Some(next) = env
                .storage()
                .persistent()
                .get::<_, MilestoneSchedule>(&DataKey::MilestoneSchedule(contract_id, milestone_idx + 1))
            {
                if let (Some(new_due), Some(next_due)) = (schedule.due_date, next.due_date) {
                    if new_due >= next_due {
                        panic!("milestone due dates must be strictly increasing");
                    }
                }
            }
        }

        let mut milestone = contract.milestones.get(milestone_idx).unwrap();
        milestone.deadline_at = schedule.due_date;
        contract.milestones.set(milestone_idx, milestone);
        Self::save_contract(&env, contract_id, &contract);
        Self::store_schedule(&env, contract_id, milestone_idx, &schedule);
        true
    }

    pub fn get_milestone_schedule(
        env: Env,
        contract_id: u32,
        milestone_idx: u32,
    ) -> Option<MilestoneSchedule> {
        env.storage()
            .persistent()
            .get(&DataKey::MilestoneSchedule(contract_id, milestone_idx))
    }

    pub fn evaluate_milestone_timeout(env: Env, contract_id: u32, milestone_id: u32) -> bool {
        let mut contract = Self::load_contract(&env, contract_id);
        if milestone_id >= contract.milestones.len() {
            panic!("Invalid milestone ID");
        }

        let milestone = contract.milestones.get(milestone_id).unwrap();
        let expired = !milestone.released && Self::milestone_expired(&env, &milestone);
        if expired && contract.status == ContractStatus::Funded {
            contract.status = ContractStatus::Disputed;
            Self::save_contract(&env, contract_id, &contract);
        }
        expired
    }

    pub fn issue_reputation(env: Env, contract_id: u32, rating: i128) -> bool {
        let mut contract = Self::load_contract(&env, contract_id);
        if contract.status != ContractStatus::Completed {
            panic!("contract not completed");
        }
        if contract.reputation_issued {
            panic!("reputation already issued");
        }
        if !(1..=5).contains(&rating) {
            panic!("rating out of range");
        }

        let key = DataKey::Reputation(contract.freelancer.clone());
        let mut record: ReputationRecord = env.storage().persistent().get(&key).unwrap_or(
            ReputationRecord {
                completed_contracts: 0,
                total_rating: 0,
                last_rating: 0,
            },
        );
        record.completed_contracts += 1;
        record.total_rating += rating;
        record.last_rating = rating;
        env.storage().persistent().set(&key, &record);

        contract.reputation_issued = true;
        Self::save_contract(&env, contract_id, &contract);
        true
    }

    pub fn get_contract(env: Env, contract_id: u32) -> EscrowContractData {
        Self::load_contract(&env, contract_id)
    }

    pub fn get_reputation(env: Env, freelancer: Address) -> Option<ReputationRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::Reputation(freelancer))
    }

    pub fn get_pending_reputation_credits(env: Env, freelancer: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::PendingReputationCredits(freelancer))
            .unwrap_or(0)
    }

    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }
}

#[cfg(test)]
mod test;
