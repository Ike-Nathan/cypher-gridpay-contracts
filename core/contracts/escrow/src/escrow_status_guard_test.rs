#![cfg(test)]

// Status-guard tests for dispute_escrow / release_escrow.
//
// These live in their own module because the legacy `test.rs` suite is
// currently excluded from the build (`// mod test;` in lib.rs) and no longer
// compiles against the current contract API.

use crate::*;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{testutils::Address as _, token, Address, Env};

fn setup(env: &Env) -> (EscrowContractClient<'_>, Address, Address, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);

    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin = token::StellarAssetClient::new(env, &token_addr);
    let customer = Address::generate(env);
    token_admin.mint(&customer, &10_000);

    (client, admin, customer, Address::generate(env), token_addr)
}

fn create_releasable_escrow(
    env: &Env,
    client: &EscrowContractClient<'_>,
    customer: &Address,
    merchant: &Address,
    token: &Address,
) -> u64 {
    env.ledger().set_timestamp(1000);
    let escrow_id = client.create_escrow(
        customer, merchant, &1000_i128, token, &1500_u64, &0_u64, &0_u64, &false,
    );
    env.ledger().set_timestamp(2000);
    escrow_id
}

#[test]
fn test_dispute_rejected_after_release() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    let escrow_id = create_releasable_escrow(&env, &client, &customer, &merchant, &token);

    client.release_escrow(&admin, &escrow_id, &false);
    assert_eq!(client.get_escrow(&escrow_id).status, EscrowStatus::Released);

    let result = client.try_dispute_escrow(&customer, &escrow_id);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::InvalidStatus))));

    let result = client.try_dispute_escrow(&merchant, &escrow_id);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::InvalidStatus))));

    // Status must be unchanged by the rejected dispute attempts.
    assert_eq!(client.get_escrow(&escrow_id).status, EscrowStatus::Released);
}

#[test]
fn test_dispute_rejected_after_refund() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = create_releasable_escrow(&env, &client, &customer, &merchant, &token);

    client.refund_escrow(&customer, &escrow_id);
    assert_eq!(client.get_escrow(&escrow_id).status, EscrowStatus::Resolved);

    let result = client.try_dispute_escrow(&customer, &escrow_id);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::InvalidStatus))));
}

#[test]
fn test_dispute_rejected_when_already_disputed() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = create_releasable_escrow(&env, &client, &customer, &merchant, &token);

    client.dispute_escrow(&customer, &escrow_id);
    assert_eq!(client.get_escrow(&escrow_id).status, EscrowStatus::Disputed);

    let result = client.try_dispute_escrow(&merchant, &escrow_id);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::InvalidStatus))));
}

#[test]
fn test_release_marks_completed_before_payout() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    let escrow_id = create_releasable_escrow(&env, &client, &customer, &merchant, &token);
    let token_client = token::Client::new(&env, &token);

    client.release_escrow(&admin, &escrow_id, &false);

    assert_eq!(client.get_escrow(&escrow_id).status, EscrowStatus::Released);
    assert_eq!(token_client.balance(&merchant), 1000);
}

#[test]
fn test_double_release_rejected() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    let escrow_id = create_releasable_escrow(&env, &client, &customer, &merchant, &token);
    let token_client = token::Client::new(&env, &token);

    client.release_escrow(&admin, &escrow_id, &false);
    let merchant_balance = token_client.balance(&merchant);
    let contract_balance = token_client.balance(&client.address);

    let result = client.try_release_escrow(&admin, &escrow_id, &false);
    assert_eq!(
        result,
        Err(Ok(Error::Escrow(EscrowError::AlreadyProcessed)))
    );

    // No additional funds moved on the rejected second release.
    assert_eq!(token_client.balance(&merchant), merchant_balance);
    assert_eq!(token_client.balance(&client.address), contract_balance);
    assert_eq!(client.get_escrow(&escrow_id).status, EscrowStatus::Released);
}
