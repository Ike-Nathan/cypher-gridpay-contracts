#![cfg(test)]

use crate::*;
use soroban_sdk::testutils::storage::{Instance as _, Persistent as _};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, Symbol};

fn advance_ledgers(env: &Env, ledgers: u32) {
    env.ledger().with_mut(|l| l.sequence_number += ledgers);
}

#[test]
fn test_set_persistent_extends_ttl() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let key = Symbol::new(&env, "ttl_key");

    env.as_contract(&contract_id, || {
        storage::set_persistent(&env, &key, &42u64);
        assert_eq!(
            env.storage().persistent().get_ttl(&key),
            storage::PERSISTENT_BUMP_AMOUNT
        );
    });
}

#[test]
fn test_get_persistent_extends_ttl() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let key = Symbol::new(&env, "ttl_key");

    env.as_contract(&contract_id, || {
        storage::set_persistent(&env, &key, &42u64);
        storage::extend_instance(&env);
    });

    advance_ledgers(&env, 2 * storage::DAY_IN_LEDGERS);

    env.as_contract(&contract_id, || {
        assert!(env.storage().persistent().get_ttl(&key) < storage::PERSISTENT_LIFETIME_THRESHOLD);
        assert_eq!(storage::get_persistent::<_, u64>(&env, &key), Some(42));
        assert_eq!(
            env.storage().persistent().get_ttl(&key),
            storage::PERSISTENT_BUMP_AMOUNT
        );
    });
}

#[test]
fn test_has_persistent_extends_ttl() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let key = Symbol::new(&env, "ttl_key");

    env.as_contract(&contract_id, || {
        storage::set_persistent(&env, &key, &42u64);
        storage::extend_instance(&env);
    });

    advance_ledgers(&env, 2 * storage::DAY_IN_LEDGERS);

    env.as_contract(&contract_id, || {
        assert!(storage::has_persistent(&env, &key));
        assert_eq!(
            env.storage().persistent().get_ttl(&key),
            storage::PERSISTENT_BUMP_AMOUNT
        );
    });
}

#[test]
fn test_missing_persistent_entry_is_not_extended() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let key = Symbol::new(&env, "missing");

    env.as_contract(&contract_id, || {
        assert_eq!(storage::get_persistent::<_, u64>(&env, &key), None);
        assert!(!storage::has_persistent(&env, &key));
    });
}

#[test]
fn test_extend_instance_extends_ttl() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());

    env.as_contract(&contract_id, || {
        storage::extend_instance(&env);
        assert_eq!(
            env.storage().instance().get_ttl(),
            storage::INSTANCE_BUMP_AMOUNT
        );
    });
}

#[test]
fn test_get_refund_extends_instance_ttl() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let refund_id = client.request_refund(
        &Address::generate(&env),
        &1u64,
        &Address::generate(&env),
        &1000i128,
        &1000,
        &Address::generate(&env),
        &String::from_str(&env, "reason"),
        &RefundReasonCode::Other,
        &0_u64,
    );
    client.get_refund(&refund_id);

    let instance_ttl = || env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert_eq!(instance_ttl(), storage::INSTANCE_BUMP_AMOUNT);

    advance_ledgers(&env, 2 * storage::DAY_IN_LEDGERS);
    assert!(instance_ttl() < storage::INSTANCE_LIFETIME_THRESHOLD);

    client.get_refund(&refund_id);
    assert_eq!(instance_ttl(), storage::INSTANCE_BUMP_AMOUNT);
}

#[test]
fn test_archived_customer_refund_extends_ttl() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let customer = Address::generate(&env);
    let key = DataKey::CustomerRefundsArchive(customer.clone(), 0);

    env.as_contract(&contract_id, || {
        storage::set_persistent(&env, &key, &5u64);
        storage::extend_instance(&env);
    });
    advance_ledgers(&env, 2 * storage::DAY_IN_LEDGERS);

    env.as_contract(&contract_id, || {
        env.storage().instance().set(
            &DataKey::CustomerRefundHistoryStart(customer.clone()),
            &1u64,
        );
        assert_eq!(
            RefundContract::get_customer_refund_id_at(&env, &customer, 0),
            Some(5)
        );
        assert_eq!(
            env.storage().persistent().get_ttl(&key),
            storage::PERSISTENT_BUMP_AMOUNT
        );
    });
}
