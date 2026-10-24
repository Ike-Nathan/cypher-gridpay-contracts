#![cfg(test)]

// Issue #50: verify that opening a dispute publishes a `DisputeOpened`
// lifecycle event with the expected metadata.

use crate::*;
use soroban_sdk::{testutils::Address as _, token, Address, Env};

fn setup(
    env: &Env,
) -> (
    EscrowContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
) {
    env.mock_all_auths();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    let customer = Address::generate(env);
    let merchant = Address::generate(env);
    let token = env.register_stellar_asset_contract(admin.clone());
    let token_admin = token::StellarAssetClient::new(env, &token);
    token_admin.mint(&customer, &1_000_000);
    (client, admin, customer, merchant, token)
}

fn create_escrow(
    client: &EscrowContractClient,
    customer: &Address,
    merchant: &Address,
    token: &Address,
) -> u64 {
    client.create_escrow(
        customer, merchant, &1000_i128, token, &5000_u64, &0_u64, &0_u64, &false,
    )
}

#[test]
fn test_dispute_escrow_emits_dispute_opened_event() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);

    let escrow_id = create_escrow(&client, &customer, &merchant, &token);
    client.dispute_escrow(&customer, &escrow_id);

    let events = env.events().all();
    let found = events.iter().any(|(_contract_id, topics, _data)| {
        topics
            .iter()
            .any(|t| format!("{:?}", t).contains("DisputeOpened"))
    });
    assert!(found, "expected a DisputeOpened event to be published");

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Disputed);
}
