#![cfg(test)]

use soroban_sdk::{symbol_short, Env};

use crate::{Escrow, EscrowClient};

mod milestone_schedule;
mod timeout_tests;

#[test]
fn hello_round_trips_symbol() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    assert_eq!(client.hello(&symbol_short!("World")), symbol_short!("World"));
}
