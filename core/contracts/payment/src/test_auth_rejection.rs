#![cfg(test)]
use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{AdminContract, AdminContractClient, Error, PaymentContract, PaymentContractClient};

#[test]
fn test_initialize_requires_auth() {
    let env = Env::default();
    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    let result = client.try_initialize(&admin);
    assert!(result.is_err(), "initialize should reject without auth");
}

#[test]
fn test_set_fee_config_requires_auth() {
    let env = Env::default();
    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);

    let result = client.try_set_fee_config(
        &admin,
        &crate::FeeConfig {
            fee_bps: 100,
            min_fee: 0,
            max_fee: 0,
            treasury: admin.clone(),
            fee_token: Address::generate(&env),
            active: true,
        },
    );
    assert!(result.is_err(), "set_fee_config should reject without auth");
}

#[test]
fn test_add_admin_requires_auth() {
    let env = Env::default();
    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);

    let new_admin = Address::generate(&env);
    let result = client.try_add_admin(&admin, &new_admin);
    assert!(result.is_err(), "add_admin should reject without auth");
}

#[test]
fn test_sweep_fees_requires_auth() {
    let env = Env::default();
    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);

    let result = client.try_sweep_platform_fees(&admin);
    assert!(result.is_err(), "sweep_fees should reject without auth");
}

#[test]
fn test_pause_requires_auth() {
    let env = Env::default();
    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);

    let result = client.try_pause(&admin);
    assert!(result.is_err(), "pause should reject without auth");
}

#[test]
fn test_admin_contract_initialize_requires_auth() {
    let env = Env::default();
    let contract_id = env.register(AdminContract, ());
    let client = AdminContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let pauser = Address::generate(&env);
    let payment = Address::generate(&env);
    let escrow = Address::generate(&env);
    let refund = Address::generate(&env);

    let result = client.try_initialize(&admin, &pauser, &payment, &escrow, &refund);
    assert!(result.is_err(), "admin initialize should reject without auth");
}

#[test]
fn test_admin_contract_pause_requires_auth() {
    let env = Env::default();
    let contract_id = env.register(AdminContract, ());
    let client = AdminContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let pauser = Address::generate(&env);
    let payment = Address::generate(&env);
    let escrow = Address::generate(&env);
    let refund = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &pauser, &payment, &escrow, &refund);

    let result = client.try_pause(&admin);
    assert!(result.is_err(), "admin pause should reject without auth");
}
