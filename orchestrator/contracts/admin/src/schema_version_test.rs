#![cfg(test)]

// Issue #74: the orchestrator admin contract implements the same
// `get_schema_version` / `migrate_schema` convention as the core contracts,
// as documented in docs/STORAGE_VERSIONING.md.

use super::*;
use soroban_sdk::testutils::Address as _;

fn setup_payment(env: &Env, admin: &Address) -> Address {
    let contract_id = env.register(payments::PaymentContract, ());
    let client = payments::PaymentContractClient::new(env, &contract_id);
    client.initialize(admin);
    contract_id
}

fn setup_escrow(env: &Env, admin: &Address) -> Address {
    let contract_id = env.register(escrow::EscrowContract, ());
    let client = escrow::EscrowContractClient::new(env, &contract_id);
    client.initialize(admin);
    contract_id
}

fn setup_refund(env: &Env, admin: &Address) -> Address {
    let contract_id = env.register(refund::RefundContract, ());
    let client = refund::RefundContractClient::new(env, &contract_id);
    client.initialize(admin);
    contract_id
}

fn setup(
    env: &Env,
) -> (
    AdminContractClient<'_>,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    env.mock_all_auths();
    let contract_id = env.register(AdminContract, ());
    let client = AdminContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let pauser = Address::generate(env);
    let payment_contract = setup_payment(env, &pauser);
    let escrow_contract = setup_escrow(env, &pauser);
    let refund_contract = setup_refund(env, &pauser);
    client.initialize(
        &admin,
        &pauser,
        &payment_contract,
        &escrow_contract,
        &refund_contract,
    );
    (
        client,
        contract_id,
        admin,
        pauser,
        payment_contract,
        refund_contract,
    )
}

#[test]
fn test_schema_version_initialized_to_one() {
    let env = Env::default();
    let (client, _contract_id, _admin, _pauser, _payment, _refund) = setup(&env);

    assert_eq!(client.get_schema_version(), 1);
}

#[test]
fn test_schema_version_defaults_to_one_without_stored_key() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(AdminContract, ());
    let client = AdminContractClient::new(&env, &contract_id);

    // Contract was never initialized, so no version is stored.
    assert_eq!(client.get_schema_version(), 1);
}

#[test]
fn test_migrate_schema_increments_version() {
    let env = Env::default();
    let (client, _contract_id, admin, _pauser, _payment, _refund) = setup(&env);

    client.migrate_schema(&admin, &2);
    assert_eq!(client.get_schema_version(), 2);

    // Versions without a registered data transformation are no-ops.
    client.migrate_schema(&admin, &3);
    assert_eq!(client.get_schema_version(), 3);
}

#[test]
fn test_migrate_schema_rejects_already_at_target() {
    let env = Env::default();
    let (client, _contract_id, admin, _pauser, _payment, _refund) = setup(&env);

    client.migrate_schema(&admin, &2);

    let result = client.try_migrate_schema(&admin, &2);
    assert_eq!(result, Err(Ok(Error::SchemaAlreadyAtTarget)));

    // Downgrades are rejected as well.
    let result = client.try_migrate_schema(&admin, &1);
    assert_eq!(result, Err(Ok(Error::SchemaAlreadyAtTarget)));
    assert_eq!(client.get_schema_version(), 2);
}

#[test]
fn test_migrate_schema_rejects_unauthorized_caller() {
    let env = Env::default();
    let (client, _contract_id, _admin, _pauser, _payment, _refund) = setup(&env);

    let not_admin = Address::generate(&env);
    let result = client.try_migrate_schema(&not_admin, &2);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
    assert_eq!(client.get_schema_version(), 1);
}

#[test]
fn test_migrate_schema_rejects_pauser_without_admin_role() {
    let env = Env::default();
    let (client, _contract_id, _admin, pauser, _payment, _refund) = setup(&env);

    // The pauser may pause contracts but must not migrate the schema.
    let result = client.try_migrate_schema(&pauser, &2);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
    assert_eq!(client.get_schema_version(), 1);
}

#[test]
fn test_migrate_schema_reverts_when_configuration_is_incomplete() {
    let env = Env::default();
    let (client, contract_id, admin, _pauser, _payment, _refund) = setup(&env);

    // Corrupted configuration: the refund contract address is gone.
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::RefundContract);
    });

    let result = client.try_migrate_schema(&admin, &2);
    assert_eq!(result, Err(Ok(Error::SchemaMigrationFailed)));
    // The version is untouched, so the migration can be retried.
    assert_eq!(client.get_schema_version(), 1);

    // Restoring the entry makes the migration succeed.
    let replacement = setup_refund(&env, &Address::generate(&env));
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::RefundContract, &replacement);
    });

    client.migrate_schema(&admin, &2);
    assert_eq!(client.get_schema_version(), 2);
}

#[test]
fn test_migrate_schema_reverts_when_child_contracts_are_duplicated() {
    let env = Env::default();
    let (client, contract_id, admin, _pauser, payment_contract, _refund) = setup(&env);

    // Corrupted configuration: the refund role points at the payment contract.
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::RefundContract, &payment_contract);
    });

    let result = client.try_migrate_schema(&admin, &2);
    assert_eq!(result, Err(Ok(Error::SchemaMigrationFailed)));
    assert_eq!(client.get_schema_version(), 1);
}

#[test]
fn test_migrate_schema_requires_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(AdminContract, ());
    let client = AdminContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    let result = client.try_migrate_schema(&admin, &2);
    assert_eq!(result, Err(Ok(Error::NotInitialized)));
}

#[test]
fn test_emergency_pause_still_works_after_migration() {
    let env = Env::default();
    let (client, _contract_id, admin, pauser, _payment, _refund) = setup(&env);

    client.migrate_schema(&admin, &2);

    let reason = String::from_str(&env, "incident 42");
    client.emergency_pause_all(&pauser, &reason);
    client.emergency_unpause_all(&pauser);
}
