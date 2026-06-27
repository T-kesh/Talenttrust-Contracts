use crate::{
    safe_add_amounts, safe_subtract_amounts, ContractStatus, DataKey, EscrowError, Contract as EscrowContractData,
    ContractSummary, MilestoneSummary, FinalizationRecord, EscrowClient, EscrowArgs, Escrow
};
use soroban_sdk::{symbol_short, Address, Env, Vec, Symbol};

#[soroban_sdk::contractimpl]
impl Escrow {
    pub fn finalize_contract(env: Env, contract_id: u32, finalizer: Address) -> bool {
        Self::require_not_paused(&env);
        finalizer.require_auth();

        let contract: EscrowContractData = env.storage().persistent().get(&DataKey::Contract(contract_id)).unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));
        if contract.status != ContractStatus::Completed && contract.status != ContractStatus::Disputed {
            env.panic_with_error(EscrowError::InvalidStatusTransition);
        }

        if env.storage().persistent().has(&DataKey::Finalization(contract_id)) {
            env.panic_with_error(EscrowError::AlreadyFinalized);
        }

        let summary = Self::summarize_contract(&env, contract_id, &contract);
        let record = FinalizationRecord {
            finalizer: finalizer.clone(),
            timestamp: env.ledger().timestamp(),
            summary,
        };

        env.storage().persistent().set(&DataKey::Finalization(contract_id), &record);
        env.events().publish((symbol_short!("finalized"), contract_id), (finalizer, record.timestamp));
        true
    }

    pub fn get_finalization_record(env: Env, contract_id: u32) -> Option<FinalizationRecord> {
        env.storage().persistent().get(&DataKey::Finalization(contract_id))
    }

    fn summarize_contract(env: &Env, contract_id: u32, contract: &EscrowContractData) -> ContractSummary {
        let m_key = (DataKey::Contract(contract_id), Symbol::new(env, "milestones"));
        let milestones: Vec<crate::Milestone> = env.storage().persistent().get(&m_key).unwrap();
        let mut summary_milestones = Vec::new(env);
        let mut released_count = 0;
        let mut total_amount = 0;

        for (i, m) in milestones.iter().enumerate() {
            total_amount += m.amount;
            if m.released { released_count += 1; }
            summary_milestones.push_back(MilestoneSummary {
                index: i as u32,
                amount: m.amount,
                released: m.released,
                refunded: m.refunded,
            });
        }

        ContractSummary {
            schema_version: 1,
            client: contract.client.clone(),
            freelancer: contract.freelancer.clone(),
            arbiter: contract.arbiter.clone(),
            status: contract.status,
            reputation_issued: false,
            total_amount,
            funded_amount: contract.funded_amount,
            released_amount: contract.released_amount,
            refundable_balance: contract.funded_amount - contract.released_amount - contract.refunded_amount,
            released_milestone_count: released_count,
            milestones: summary_milestones,
        }
    }
}
