#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_schema_version_initialized_to_one() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    assert_eq!(client.get_schema_version(), 1);
}

#[test]
fn test_migrate_schema_rejects_already_at_target() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    client.migrate_schema(&admin, &2);
    assert_eq!(client.get_schema_version(), 2);

    let result = client.try_migrate_schema(&admin, &2);
    assert_eq!(result, Err(Ok(Error::Ext(ExtError::SchemaAlreadyAtTarget))));
}

#[test]
fn test_dry_run_migrate_schema_is_idempotent() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
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

fn legacy_refund(env: &Env, id: u64, status: RefundStatus) -> LegacyRefundV1 {
    let token = Address::generate(env);
    LegacyRefundV1 {
        id,
        payment_id: 100 + id,
        merchant: Address::generate(env),
        customer: Address::generate(env),
        amount: 250,
        original_payment_amount: 1_000,
        token: token.clone(),
        original_token: token,
        status,
        requested_at: 42,
        reason: soroban_sdk::String::from_str(env, "legacy refund"),
        approved_at: None,
        rejected_at: None,
        processed_at: None,
        rejected_by: None,
        appeal_deadline: None,
        expires_at: None,
    }
}

#[test]
fn test_migrate_schema_upgrades_legacy_refunds_to_reason_code_other() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    // A current-shape refund created through the public API.
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = Address::generate(&env);
    let current_id = client.request_refund(
        &merchant,
        &1u64,
        &customer,
        &100i128,
        &1_000i128,
        &token,
        &soroban_sdk::String::from_str(&env, "defective"),
        &RefundReasonCode::ProductDefect,
        &0u64,
    );
    assert_eq!(current_id, 1);

    // Two refunds persisted in the pre-reason_code (schema v1) shape, on a
    // deployment that predates schema tracking (no stored version, so it
    // reads as v1).
    let legacy_a = legacy_refund(&env, 2, RefundStatus::Requested);
    let legacy_b = legacy_refund(&env, 3, RefundStatus::Processed);
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&DataKey::Refund(2), &legacy_a);
        env.storage().instance().set(&DataKey::Refund(3), &legacy_b);
        env.storage().instance().set(&DataKey::RefundCounter, &3u64);
        env.storage().instance().remove(&SystemKey::SchemaVersion);
    });
    assert_eq!(client.get_schema_version(), 1);

    // Without migration the legacy shape cannot be decoded as a Refund.
    assert!(client.try_get_refund(&2).is_err());

    client.migrate_schema(&admin, &2);
    assert_eq!(client.get_schema_version(), 2);

    let migrated_a = client.get_refund(&2);
    assert_eq!(migrated_a.reason_code, RefundReasonCode::Other);
    assert_eq!(migrated_a.id, legacy_a.id);
    assert_eq!(migrated_a.payment_id, legacy_a.payment_id);
    assert_eq!(migrated_a.merchant, legacy_a.merchant);
    assert_eq!(migrated_a.customer, legacy_a.customer);
    assert_eq!(migrated_a.amount, legacy_a.amount);
    assert_eq!(migrated_a.status, RefundStatus::Requested);
    assert_eq!(migrated_a.reason, legacy_a.reason);
    assert_eq!(migrated_a.requested_at, legacy_a.requested_at);

    let migrated_b = client.get_refund(&3);
    assert_eq!(migrated_b.reason_code, RefundReasonCode::Other);
    assert_eq!(migrated_b.status, RefundStatus::Processed);

    // Records already in the current shape keep their reason code.
    assert_eq!(
        client.get_refund(&current_id).reason_code,
        RefundReasonCode::ProductDefect
    );
}
