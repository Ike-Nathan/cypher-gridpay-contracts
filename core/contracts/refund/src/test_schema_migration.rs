#![cfg(test)]

// Issue #88: migrate_schema must run every data transformation before the
// target version is persisted, and must revert the whole transaction when a
// single entry cannot be migrated.

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

fn setup<'a>(
    env: &Env,
) -> (
    RefundContractClient<'a>,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    env.mock_all_auths();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    let merchant = Address::generate(env);
    let customer = Address::generate(env);
    let token = Address::generate(env);
    (client, contract_id, admin, merchant, customer, token)
}

fn request_refund(
    env: &Env,
    client: &RefundContractClient,
    merchant: &Address,
    customer: &Address,
    token: &Address,
    payment_id: u64,
    amount: i128,
) -> u64 {
    client.request_refund(
        merchant,
        &payment_id,
        customer,
        &amount,
        &amount,
        token,
        &String::from_str(env, "damaged item"),
        &RefundReasonCode::ProductDefect,
        &env.ledger().timestamp(),
    )
}

#[test]
fn test_migrate_schema_runs_data_migration_before_bumping_version() {
    let env = Env::default();
    let (client, _contract_id, admin, merchant, customer, token) = setup(&env);

    request_refund(&env, &client, &merchant, &customer, &token, 1, 1000);
    request_refund(&env, &client, &merchant, &customer, &token, 2, 2000);

    client.migrate_schema(&admin, &2);

    assert_eq!(client.get_schema_version(), 2);
    // An idempotent migration leaves the existing indexes untouched.
    assert_eq!(client.get_customer_refund_count_public(&customer), 2);
    assert_eq!(
        client.get_refund_count_by_status(&RefundStatus::Requested),
        2
    );
    assert_eq!(
        client.get_customer_refund_history(&customer, &10, &0).len(),
        2
    );
}

#[test]
fn test_migrate_schema_rebuilds_missing_status_and_history_indexes() {
    let env = Env::default();
    let (client, contract_id, admin, merchant, customer, token) = setup(&env);

    request_refund(&env, &client, &merchant, &customer, &token, 1, 1000);
    request_refund(&env, &client, &merchant, &customer, &token, 2, 2000);

    // Simulate legacy state: the status index and the customer history index
    // were never written for these refunds.
    env.as_contract(&contract_id, || {
        for index in 0..2_u64 {
            env.storage()
                .instance()
                .remove(&DataKey::RefundStatusIndex(index + 1));
            env.storage()
                .instance()
                .remove(&DataKey::RefundsByStatus(RefundStatus::Requested, index));
            env.storage()
                .instance()
                .remove(&DataKey::CustomerRefunds(customer.clone(), index));
        }
        env.storage()
            .instance()
            .set(&DataKey::RefundStatusCount(RefundStatus::Requested), &0_u64);
        env.storage()
            .instance()
            .set(&DataKey::CustomerRefundCount(customer.clone()), &0_u64);
    });

    client.migrate_schema(&admin, &2);

    assert_eq!(client.get_schema_version(), 2);
    assert_eq!(
        client.get_refund_count_by_status(&RefundStatus::Requested),
        2
    );
    assert_eq!(
        client
            .get_refunds_by_status(&RefundStatus::Requested, &10, &0)
            .len(),
        2
    );
    assert_eq!(client.get_customer_refund_count_public(&customer), 2);
    let history = client.get_customer_refund_history(&customer, &10, &0);
    assert_eq!(history.len(), 2);
    // No duplicates were appended for the already indexed refunds. The history
    // is returned newest-first, so the last migrated refund comes first.
    assert_eq!(history.get(0).unwrap().id, 2);
    assert_eq!(history.get(1).unwrap().id, 1);
}

#[test]
fn test_migrate_schema_backfills_rejection_timestamp() {
    let env = Env::default();
    let (client, contract_id, admin, merchant, customer, token) = setup(&env);

    let refund_id = request_refund(&env, &client, &merchant, &customer, &token, 1, 1000);
    client.reject_refund(&admin, &refund_id, &String::from_str(&env, "out of stock"));
    // Let the appeal window elapse so the refund is finalized as rejected.
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 604_800 + 1);
    client.finalize_denial(&refund_id);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .remove(&SystemKey::RefundRejectedAt(refund_id));
    });

    client.migrate_schema(&admin, &2);

    let rejected_at: Option<u64> = env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .get(&SystemKey::RefundRejectedAt(refund_id))
    });
    assert_eq!(rejected_at, client.get_refund(&refund_id).rejected_at);
}

#[test]
fn test_migrate_schema_reverts_when_refund_record_is_missing() {
    let env = Env::default();
    let (client, contract_id, admin, merchant, customer, token) = setup(&env);

    request_refund(&env, &client, &merchant, &customer, &token, 1, 1000);
    request_refund(&env, &client, &merchant, &customer, &token, 2, 2000);

    // Corrupted state: the counter references a refund that cannot be read.
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Refund(2));
        // Force a write before the failing entry so the rollback is observable.
        env.storage()
            .instance()
            .remove(&DataKey::CustomerRefunds(customer.clone(), 0));
    });

    let result = client.try_migrate_schema(&admin, &2);
    assert_eq!(result, Err(Ok(Error::Ext(ExtError::SchemaMigrationFailed))));

    // The version must not be bumped...
    assert_eq!(client.get_schema_version(), 1);
    // ...and every write performed before the failing entry is rolled back.
    let rolled_back = env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .has(&DataKey::CustomerRefunds(customer.clone(), 0))
    });
    assert!(!rolled_back);
}

#[test]
fn test_migrate_schema_reverts_on_refund_id_mismatch() {
    let env = Env::default();
    let (client, contract_id, admin, merchant, customer, token) = setup(&env);

    request_refund(&env, &client, &merchant, &customer, &token, 1, 1000);

    // Corrupted record: the stored id does not match the key it is stored under.
    env.as_contract(&contract_id, || {
        let mut refund: Refund = env.storage().instance().get(&DataKey::Refund(1)).unwrap();
        refund.id = 42;
        env.storage().instance().set(&DataKey::Refund(1), &refund);
    });

    let result = client.try_migrate_schema(&admin, &2);
    assert_eq!(result, Err(Ok(Error::Ext(ExtError::SchemaMigrationFailed))));
    assert_eq!(client.get_schema_version(), 1);
}

#[test]
fn test_migrate_schema_rejects_non_admin_before_running_migrations() {
    let env = Env::default();
    let (client, _contract_id, _admin, merchant, customer, token) = setup(&env);

    request_refund(&env, &client, &merchant, &customer, &token, 1, 1000);

    let not_admin = Address::generate(&env);
    let result = client.try_migrate_schema(&not_admin, &2);
    assert_eq!(result, Err(Ok(Error::Core(CoreError::Unauthorized))));
    assert_eq!(client.get_schema_version(), 1);
}
