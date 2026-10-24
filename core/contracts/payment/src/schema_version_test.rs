#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_schema_version_initialized_to_one() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    assert_eq!(client.get_schema_version(), 1);
}

#[test]
fn test_migrate_schema_rejects_already_at_target() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    client.migrate_schema(&admin, &2);
    assert_eq!(client.get_schema_version(), 2);

    let result = client.try_migrate_schema(&admin, &2);
    assert_eq!(
        result,
        Err(Ok(Error::Basic(BasicError::SchemaAlreadyAtTarget)))
    );
}

#[test]
fn test_dry_run_migrate_schema_is_idempotent() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    // Two consecutive dry runs must produce identical reports.
    let first = client.dry_run_migrate_schema(&2);
    let second = client.dry_run_migrate_schema(&2);

    assert_eq!(first, second);
    assert_eq!(first.current_version, 1);
    assert_eq!(first.target_version, 2);
    assert_eq!(first.converted_records, 0);
    assert!(first.dry_run);
    assert!(first.gas_estimate > 0);

    // The dry run must not mutate storage: schema version stays put and a
    // subsequent real migration still succeeds.
    assert_eq!(client.get_schema_version(), 1);
    client.migrate_schema(&admin, &2);
    assert_eq!(client.get_schema_version(), 2);
}

#[test]
fn test_dry_run_migrate_schema_reports_record_count() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = Address::generate(&env);
    let metadata = String::from_str(&env, "");

    client.create_payment(
        &customer,
        &merchant,
        &100,
        &token,
        &Currency::USDC,
        &0,
        &metadata,
    );
    client.create_payment(
        &customer,
        &merchant,
        &200,
        &token,
        &Currency::USDC,
        &0,
        &metadata,
    );

    let report = client.dry_run_migrate_schema(&2);
    assert_eq!(report.current_version, 1);
    assert_eq!(report.target_version, 2);
    assert_eq!(report.converted_records, 2);
    assert_eq!(
        report.gas_estimate,
        MIGRATION_BASE_GAS + MIGRATION_GAS_PER_RECORD * 2
    );
    assert!(report.dry_run);

    // State remains untouched after reporting on existing records.
    assert_eq!(client.get_schema_version(), 1);
}
