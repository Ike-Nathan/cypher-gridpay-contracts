#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};
use test_utils::{create_mock_token, generate_parties, setup_test_env};

use crate::{EscrowContract, EscrowContractClient};

fn create_contract(env: &Env) -> EscrowContractClient {
    let contract_id = env.register_contract(None, EscrowContract);
    EscrowContractClient::new(env, &contract_id)
}

#[test]
fn test_setup_test_env_provides_funded_parties() {
    let test_env = setup_test_env(1_000);
    let parties = &test_env.parties;

    assert_eq!(test_env.token.client.balance(&parties.payer), 1_000);
    assert_eq!(test_env.token.client.balance(&parties.payee), 1_000);
    assert_eq!(test_env.token.client.balance(&parties.arbiter), 1_000);
}

#[test]
fn test_create_mock_token_registers_asset() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token = create_mock_token(&env, &admin);

    token.admin_client.mint(&admin, &500);
    assert_eq!(token.client.balance(&admin), 500);
}

#[test]
fn test_generate_parties_are_distinct() {
    let env = Env::default();
    let parties = generate_parties(&env);

    assert_ne!(parties.admin, parties.payer);
    assert_ne!(parties.payer, parties.payee);
    assert_ne!(parties.payee, parties.arbiter);
}

#[test]
fn test_escrow_flow_uses_shared_fixtures() {
    let test_env = setup_test_env(10_000);
    let env = &test_env.env;
    let parties = &test_env.parties;
    let token = &test_env.token;

    let client = create_contract(env);
    let escrow_id = client.create_escrow(
        &parties.payer,
        &parties.payee,
        &parties.arbiter,
        &token.address,
        &5_000,
    );

    client.fund_escrow(&escrow_id, &parties.payer);
    assert_eq!(token.client.balance(&parties.payer), 5_000);

    client.release_escrow(&escrow_id, &parties.payer);
    assert_eq!(token.client.balance(&parties.payee), 15_000);
}

#[test]
fn test_template_creation_rejects_invalid_hold_bounds_and_empty_allowlist() {
    let env = Env::default();
    env.mock_all_auths();
    let client = create_contract(&env);
    let owner = Address::generate(&env);
    let token = Address::generate(&env);
    let allowed_tokens = Vec::from_array(&env, [token.clone()]);
    let empty_allowlist = Vec::new(&env);
    let description = String::from_str(&env, "Test template");

    assert!(client
        .try_create_template_with_policy(
            &owner,
            &token,
            &1_000,
            &10,
            &description,
            &20,
            &10,
            &allowed_tokens,
            &0,
        )
        .is_err());
    assert!(client
        .try_create_template_with_policy(
            &owner,
            &token,
            &1_000,
            &10,
            &description,
            &0,
            &20,
            &empty_allowlist,
            &0,
        )
        .is_err());
}

#[test]
fn test_template_upgrade_preserves_existing_escrow_snapshot() {
    let env = Env::default();
    env.mock_all_auths();
    let client = create_contract(&env);
    let owner = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = Address::generate(&env);
    let allowed_tokens = Vec::from_array(&env, [token.clone()]);
    let description = String::from_str(&env, "Version one");
    let template_id = client.create_template_with_policy(
        &owner,
        &token,
        &1_000,
        &10,
        &description,
        &5,
        &20,
        &allowed_tokens,
        &0,
    );
    let escrow_id = client.create_escrow_from_template(&customer, &template_id);
    let escrow_before = client.get_escrow(&escrow_id);
    assert_eq!(
        escrow_before.hold_period_until,
        env.ledger().timestamp() + 5
    );

    let mut new_params = client.get_template(&template_id);
    new_params.amount = 2_000;
    new_params.release_delay_seconds = 15;
    new_params.description = String::from_str(&env, "Version two");
    let new_template_id = client.upgrade_escrow_template(&owner, &template_id, &new_params);

    let escrow_after = client.get_escrow(&escrow_id);
    let historical = client.get_template(&template_id);
    let current = client.get_template(&new_template_id);
    assert_eq!(escrow_after.amount, escrow_before.amount);
    assert_eq!(escrow_after.token, escrow_before.token);
    assert_eq!(
        escrow_after.release_timestamp,
        escrow_before.release_timestamp
    );
    assert_eq!(historical.version, 1);
    assert_eq!(historical.superseded_by_template_id, Some(new_template_id));
    assert_eq!(current.version, 2);
    assert_eq!(current.amount, 2_000);
}
