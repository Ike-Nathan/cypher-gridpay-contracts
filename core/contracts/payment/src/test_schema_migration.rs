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
