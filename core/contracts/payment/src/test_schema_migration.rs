#![cfg(test)]

// Issue #88: migrate_schema must run every data transformation before the
// target version is persisted, and must revert the whole transaction when a
// single entry cannot be migrated.

use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String};

fn setup<'a>(
    env: &Env,
) -> (
    PaymentContractClient<'a>,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    env.mock_all_auths();
    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    let customer = Address::generate(env);
    let merchant = Address::generate(env);
    let token = Address::generate(env);
    (client, contract_id, admin, customer, merchant, token)
}

fn create_payment(
    env: &Env,
    client: &PaymentContractClient,
    customer: &Address,
    merchant: &Address,
    token: &Address,
    amount: i128,
) -> u64 {
    client.create_payment(
        customer,
        merchant,
        &amount,
        token,
        &Currency::USDC,
        &0_u64,
        &String::from_str(env, ""),
    )
}

#[test]
fn test_migrate_schema_runs_data_migration_before_bumping_version() {
    let env = Env::default();
    let (client, _contract_id, admin, customer, merchant, token) = setup(&env);

    create_payment(&env, &client, &customer, &merchant, &token, 100);
    create_payment(&env, &client, &customer, &merchant, &token, 200);

    client.migrate_schema(&admin, &2);

    assert_eq!(client.get_schema_version(), 2);
    // Indexes are untouched by an idempotent migration.
    assert_eq!(client.get_payment_count_by_customer(&customer), 2);
    assert_eq!(client.get_payment_count_by_merchant(&merchant), 2);
    assert_eq!(client.get_payments_by_customer(&customer, &10, &0).len(), 2);
}

#[test]
fn test_migrate_schema_rebuilds_missing_payment_indexes() {
    let env = Env::default();
    let (client, contract_id, admin, customer, merchant, token) = setup(&env);

    create_payment(&env, &client, &customer, &merchant, &token, 100);
    create_payment(&env, &client, &customer, &merchant, &token, 200);

    // Simulate legacy state: both payments predate the customer/merchant indexes.
    env.as_contract(&contract_id, || {
        for index in 0..2_u64 {
            env.storage()
                .instance()
                .remove(&DataKey::Customer(CustomerDataKey::Payments(
                    customer.clone(),
                    index,
                )));
            env.storage()
                .instance()
                .remove(&DataKey::Merchant(MerchantDataKey::Payments(
                    merchant.clone(),
                    index,
                )));
        }
        env.storage().instance().set(
            &DataKey::Customer(CustomerDataKey::PaymentCount(customer.clone())),
            &0_u64,
        );
        env.storage().instance().set(
            &DataKey::Merchant(MerchantDataKey::PaymentCount(merchant.clone())),
            &0_u64,
        );
    });

    assert_eq!(client.get_payment_count_by_customer(&customer), 0);

    client.migrate_schema(&admin, &2);

    assert_eq!(client.get_schema_version(), 2);
    assert_eq!(client.get_payment_count_by_customer(&customer), 2);
    assert_eq!(client.get_payment_count_by_merchant(&merchant), 2);
    let restored: Option<u64> = env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .get(&DataKey::Customer(CustomerDataKey::Payments(
                customer.clone(),
                1,
            )))
    });
    assert_eq!(restored, Some(2));

    // No duplicates in either index: the already indexed payment #2 is not
    // appended a second time.
    assert_eq!(client.get_payments_by_customer(&customer, &10, &0).len(), 2);
    assert_eq!(client.get_payments_by_merchant(&merchant, &10, &0).len(), 2);
    assert_eq!(client.get_merchant_payments(&merchant, &0).len(), 2);
}

#[test]
fn test_migrate_schema_reverts_when_payment_record_is_missing() {
    let env = Env::default();
    let (client, contract_id, admin, customer, merchant, token) = setup(&env);

    create_payment(&env, &client, &customer, &merchant, &token, 100);
    create_payment(&env, &client, &customer, &merchant, &token, 200);

    // Corrupted state: the counter references a payment that cannot be read.
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .remove(&DataKey::Payment(PaymentKey::Data(2)));
        // Force a write before the failing entry so the rollback is observable.
        env.storage()
            .instance()
            .remove(&DataKey::Customer(CustomerDataKey::Payments(
                customer.clone(),
                0,
            )));
    });

    let result = client.try_migrate_schema(&admin, &2);
    assert_eq!(
        result,
        Err(Ok(Error::Basic(BasicError::SchemaMigrationFailed)))
    );

    // The version must not be bumped...
    assert_eq!(client.get_schema_version(), 1);
    // ...and every write performed before the failing entry is rolled back.
    let rolled_back = env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .has(&DataKey::Customer(CustomerDataKey::Payments(
                customer.clone(),
                0,
            )))
    });
    assert!(!rolled_back);
}

#[test]
fn test_migrate_schema_reverts_on_payment_id_mismatch() {
    let env = Env::default();
    let (client, contract_id, admin, customer, merchant, token) = setup(&env);

    create_payment(&env, &client, &customer, &merchant, &token, 100);

    // Corrupted record: the stored id does not match the key it is stored under.
    env.as_contract(&contract_id, || {
        let mut payment: Payment = env
            .storage()
            .instance()
            .get(&DataKey::Payment(PaymentKey::Data(1)))
            .unwrap();
        payment.id = 42;
        env.storage()
            .instance()
            .set(&DataKey::Payment(PaymentKey::Data(1)), &payment);
    });

    let result = client.try_migrate_schema(&admin, &2);
    assert_eq!(
        result,
        Err(Ok(Error::Basic(BasicError::SchemaMigrationFailed)))
    );
    assert_eq!(client.get_schema_version(), 1);
}

#[test]
fn test_migrate_schema_skips_unregistered_versions() {
    let env = Env::default();
    let (client, _contract_id, admin, customer, merchant, token) = setup(&env);

    create_payment(&env, &client, &customer, &merchant, &token, 100);

    // Version 3 has no registered data transformation but the v1 -> v2 step
    // still runs, so the target version is written.
    client.migrate_schema(&admin, &3);
    assert_eq!(client.get_schema_version(), 3);
}

#[test]
fn test_migrate_schema_rejects_non_admin_before_running_migrations() {
    let env = Env::default();
    let (client, _contract_id, _admin, customer, merchant, token) = setup(&env);

    create_payment(&env, &client, &customer, &merchant, &token, 100);

    let not_admin = Address::generate(&env);
    let result = client.try_migrate_schema(&not_admin, &2);
    assert_eq!(result, Err(Ok(Error::Basic(BasicError::NotAnAdmin))));
    assert_eq!(client.get_schema_version(), 1);
}

// ── Batched migration (migrate_schema_step) ──────────────────────────────────

/// 100 payments whose customer/merchant indexes were never built (schema 1).
fn legacy_payments(
    env: &Env,
) -> (PaymentContractClient<'_>, Address, Address, Address, Address) {
    let (client, contract_id, admin, customer, merchant, token) = setup(env);
    for i in 0..100 {
        create_payment(env, &client, &customer, &merchant, &token, 100 + i);
    }
    env.as_contract(&contract_id, || {
        let instance = env.storage().instance();
        for index in 0..100_u64 {
            instance.remove(&DataKey::Customer(CustomerDataKey::Payments(customer.clone(), index)));
            instance.remove(&DataKey::Merchant(MerchantDataKey::Payments(merchant.clone(), index)));
        }
        instance.set(&DataKey::Customer(CustomerDataKey::PaymentCount(customer.clone())), &0_u64);
        instance.set(&DataKey::Merchant(MerchantDataKey::PaymentCount(merchant.clone())), &0_u64);
    });
    (client, admin, customer, merchant, token)
}

#[test]
fn test_migrate_schema_step_migrates_100_records_in_5_batches() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = legacy_payments(&env);

    for step in 1..=5_u64 {
        let cursor = client.migrate_schema_step(&admin, &20);
        assert_eq!(cursor.records_migrated, step * 20);
        assert_eq!(cursor.next_record_id, step * 20 + 1);
        assert_eq!(cursor.target_max_id, 100);
        assert_eq!(cursor.completed, step == 5);
        if step < 5 {
            // Not finalized: version unchanged, user writes blocked, reads fine.
            assert_eq!(client.get_schema_version(), 1);
            let blocked = client.try_create_payment(
                &customer,
                &merchant,
                &1,
                &token,
                &Currency::USDC,
                &0_u64,
                &String::from_str(&env, ""),
            );
            assert_eq!(blocked, Err(Ok(Error::Basic(BasicError::MigrationInProgress))));
            assert_eq!(client.get_payment(&1).id, 1);
        }
    }

    assert_eq!(client.get_schema_version(), LATEST_SCHEMA_VERSION);
    assert_eq!(client.get_payment_count_by_customer(&customer), 100);
    assert_eq!(client.get_payment_count_by_merchant(&merchant), 100);
    assert_eq!(client.get_migration_cursor().unwrap().from_version, 1);
    // Writes work again.
    create_payment(&env, &client, &customer, &merchant, &token, 1);
    // Nothing left to migrate.
    assert_eq!(
        client.try_migrate_schema_step(&admin, &20),
        Err(Ok(Error::Basic(BasicError::SchemaAlreadyAtTarget)))
    );
}

#[test]
fn test_migrate_schema_step_caps_the_batch_at_25() {
    let env = Env::default();
    let (client, admin, ..) = legacy_payments(&env);
    let cursor = client.migrate_schema_step(&admin, &1_000);
    assert_eq!(cursor.records_migrated, MAX_MIGRATION_BATCH as u64);
    assert_eq!(cursor.next_record_id, MAX_MIGRATION_BATCH as u64 + 1);
    assert!(!cursor.completed);
    assert_eq!(
        client.try_migrate_schema_step(&admin, &0),
        Err(Ok(Error::Basic(BasicError::InvalidMigrationBatch)))
    );
}

#[test]
fn test_migrate_schema_step_retrying_a_batch_is_idempotent() {
    let env = Env::default();
    let (client, admin, customer, ..) = legacy_payments(&env);
    let contract_id = client.address.clone();
    client.migrate_schema_step(&admin, &20);
    assert_eq!(client.get_payment_count_by_customer(&customer), 20);

    // Re-run the same batch (as if the cursor update had been lost).
    env.as_contract(&contract_id, || {
        let mut cursor: MigrationCursor = env
            .storage()
            .persistent()
            .get(&DataKey::Config(ConfigKey::MigrationCursor))
            .unwrap();
        cursor.next_record_id = 1;
        env.storage()
            .persistent()
            .set(&DataKey::Config(ConfigKey::MigrationCursor), &cursor);
    });
    client.migrate_schema_step(&admin, &20);
    assert_eq!(client.get_payment_count_by_customer(&customer), 20, "no duplicates");

    for _ in 0..4 {
        client.migrate_schema_step(&admin, &25);
    }
    assert!(client.get_migration_cursor().unwrap().completed);
    assert_eq!(client.get_payment_count_by_customer(&customer), 100);
}

#[test]
fn test_migrate_schema_step_requires_admin_and_blocks_monolithic_migration() {
    let env = Env::default();
    let (client, admin, ..) = legacy_payments(&env);
    assert_eq!(
        client.try_migrate_schema_step(&Address::generate(&env), &10),
        Err(Ok(Error::Basic(BasicError::NotAnAdmin)))
    );
    client.migrate_schema_step(&admin, &10);
    assert_eq!(
        client.try_migrate_schema(&admin, &3),
        Err(Ok(Error::Basic(BasicError::MigrationInProgress)))
    );
}
