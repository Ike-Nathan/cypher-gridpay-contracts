#![cfg(test)]

use crate::*;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{testutils::Address as _, token, vec, Address, BytesN, Env};

fn setup(env: &Env) -> (EscrowContractClient, Address, Address, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    let customer = Address::generate(env);
    let merchant = Address::generate(env);
    let token = env.register_stellar_asset_contract(admin.clone());
    let token_admin = token::StellarAssetClient::new(env, &token);
    token_admin.mint(&customer, &1_000_000);
    (client, admin, customer, merchant, token)
}

// ── SCHEMA VERSION ────────────────────────────────────────────────────────────

#[test]
fn test_schema_version_initialized_to_one() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _customer, _merchant, _token) = setup(&env);

    assert_eq!(client.get_schema_version(), 1);
}

#[test]
fn test_schema_version_increments_after_migration() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    client.create_escrow(
        &customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );
    client.begin_migration(&admin);
    client.migrate_escrow(&admin, &1);
    client.complete_migration(&admin);

    assert_eq!(client.get_schema_version(), 2);
}

#[test]
fn test_begin_migration_rejects_when_schema_already_at_target() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    client.create_escrow(
        &customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );
    client.begin_migration(&admin);
    client.migrate_escrow(&admin, &1);
    client.complete_migration(&admin);

    let result = client.try_begin_migration(&admin);
    assert_eq!(
        result,
        Err(Ok(Error::Basic(BasicError::SchemaAlreadyAtTarget)))
    );
}

#[test]
fn test_initiate_clawback_rejects_zero_delay() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    let escrow_id = client.create_escrow(
        &customer, &merchant, &1000_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );
    let reason_hash = BytesN::from_array(&env, &[1u8; 32]);

    let result = client.try_initiate_clawback(&admin, &escrow_id, &reason_hash, &0_u64);
    assert_eq!(
        result,
        Err(Ok(Error::Escrow(EscrowError::ClawbackDelayTooShort)))
    );
}

#[test]
fn test_sub_account_fee_bps_override() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    client.set_escrow_fee_config(
        &admin,
        &EscrowFeeConfig {
            fee_bps: 500,
            fee_recipient: admin.clone(),
            enabled: true,
        },
    );

    let escrow_id = client.create_escrow(
        &customer, &merchant, &1000_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );
    let label = BytesN::from_array(&env, &[1u8; 32]);

    let fee_free_id = client.create_sub_account(&merchant, &escrow_id, &label, &200, &Some(0));
    let premium_id = client.create_sub_account(
        &merchant,
        &escrow_id,
        &BytesN::from_array(&env, &[2u8; 32]),
        &300,
        &Some(1000),
    );
    let inherited_id = client.create_sub_account(
        &merchant,
        &escrow_id,
        &BytesN::from_array(&env, &[3u8; 32]),
        &100,
        &None,
    );

    assert_eq!(
        client
            .get_sub_account(&escrow_id, &fee_free_id)
            .unwrap()
            .fee_bps_override,
        Some(0)
    );
    assert_eq!(
        client
            .get_sub_account(&escrow_id, &premium_id)
            .unwrap()
            .fee_bps_override,
        Some(1000)
    );
    assert!(client
        .get_sub_account(&escrow_id, &inherited_id)
        .unwrap()
        .fee_bps_override
        .is_none());

    client.set_sub_account_fee_override(&merchant, &escrow_id, &inherited_id, &Some(250));
    assert_eq!(
        client
            .get_sub_account(&escrow_id, &inherited_id)
            .unwrap()
            .fee_bps_override,
        Some(250)
    );
}

// ── MIGRATION FLOW ────────────────────────────────────────────────────────────

#[test]
fn test_begin_migration_sets_status() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    // Create a couple of escrows first
    client.create_escrow(
        &customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );
    client.create_escrow(
        &customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );

    env.ledger().set_timestamp(100);
    client.begin_migration(&admin);

    let status = client.get_migration_status();
    assert!(status.in_progress);
    assert_eq!(status.total_count, 2);
    assert_eq!(status.migrated_count, 0);
    assert_eq!(status.started_at, 100);
    assert!(status.completed_at.is_none());
}

#[test]
fn test_migrate_escrow_single() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    let id = client.create_escrow(
        &customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );
    client.begin_migration(&admin);
    client.migrate_escrow(&admin, &id);

    let status = client.get_migration_status();
    assert_eq!(status.migrated_count, 1);
}

#[test]
fn test_complete_migration_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    let id1 = client.create_escrow(
        &customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );
    let id2 = client.create_escrow(
        &customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );

    env.ledger().set_timestamp(200);
    client.begin_migration(&admin);
    client.migrate_escrow(&admin, &id1);
    client.migrate_escrow(&admin, &id2);

    env.ledger().set_timestamp(300);
    client.complete_migration(&admin);

    let status = client.get_migration_status();
    assert!(!status.in_progress);
    assert_eq!(status.completed_at, Some(300));
}

#[test]
fn test_migrate_escrow_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    let id1 = client.create_escrow(
        &customer, &merchant, &100_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );
    let id2 = client.create_escrow(
        &customer, &merchant, &200_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );
    let id3 = client.create_escrow(
        &customer, &merchant, &300_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );

    client.begin_migration(&admin);

    let ids = vec![&env, id1, id2, id3];
    let count = client.migrate_escrow_batch(&admin, &ids);
    assert_eq!(count, 3);

    let status = client.get_migration_status();
    assert_eq!(status.migrated_count, 3);
}

// ── BLOCKED CREATION DURING MIGRATION ────────────────────────────────────────

#[test]
fn test_create_escrow_blocked_during_migration() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    client.begin_migration(&admin);

    let result = client.try_create_escrow(
        &customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );
    assert_eq!(result, Err(Ok(Error::Basic(BasicError::ContractPaused))));
}

#[test]
fn test_create_escrow_allowed_after_migration_complete() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    // No escrows yet — begin and immediately complete
    client.begin_migration(&admin);
    client.complete_migration(&admin);

    // Should succeed now
    let id = client.create_escrow(
        &customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );
    assert_eq!(id, 1);
}

// ── DOUBLE-MIGRATION GUARD ────────────────────────────────────────────────────

#[test]
fn test_double_migrate_returns_already_migrated() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    let id = client.create_escrow(
        &customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );
    client.begin_migration(&admin);
    client.migrate_escrow(&admin, &id);

    let result = client.try_migrate_escrow(&admin, &id);
    assert_eq!(result, Err(Ok(Error::Basic(BasicError::AlreadyMigrated))));
}

#[test]
fn test_batch_skips_already_migrated() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    let id1 = client.create_escrow(
        &customer, &merchant, &100_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );
    let id2 = client.create_escrow(
        &customer, &merchant, &200_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );

    client.begin_migration(&admin);
    client.migrate_escrow(&admin, &id1); // migrate id1 first

    // Batch includes id1 again — should skip it, only count id2
    let ids = vec![&env, id1, id2];
    let count = client.migrate_escrow_batch(&admin, &ids);
    assert_eq!(count, 1);

    let status = client.get_migration_status();
    assert_eq!(status.migrated_count, 2); // 1 from single + 1 from batch
}

// ── COMPLETE FAILS IF NOT ALL MIGRATED ───────────────────────────────────────

#[test]
fn test_complete_migration_fails_if_not_all_migrated() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    client.create_escrow(
        &customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );
    client.create_escrow(
        &customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );

    client.begin_migration(&admin);
    // Only migrate one of two
    client.migrate_escrow(&admin, &1);

    let result = client.try_complete_migration(&admin);
    assert!(result.is_err());
}

// ── MIGRATE WITHOUT BEGIN ─────────────────────────────────────────────────────

#[test]
fn test_migrate_without_begin_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    let id = client.create_escrow(
        &customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );

    let result = client.try_migrate_escrow(&admin, &id);
    assert_eq!(
        result,
        Err(Ok(Error::Basic(BasicError::MigrationNotStarted)))
    );
}

// ── UNAUTHORIZED ADMIN ────────────────────────────────────────────────────────

#[test]
fn test_begin_migration_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _customer, _merchant, _token) = setup(&env);

    let not_admin = Address::generate(&env);
    let result = client.try_begin_migration(&not_admin);
    assert_eq!(result, Err(Ok(Error::Basic(BasicError::NotAnAdmin))));
}

// ── Batched migration (migrate_schema_step) ──────────────────────────────────

fn create_escrows(
    env: &Env,
    n: u64,
) -> (EscrowContractClient<'_>, Address, Address, Address, Address) {
    let (client, admin, customer, merchant, token) = setup(env);
    for _ in 0..n {
        client.create_escrow(
            &customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
        );
    }
    (client, admin, customer, merchant, token)
}

fn migrated(env: &Env, client: &EscrowContractClient, escrow_id: u64) -> bool {
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Dispute(DisputeKey::EscrowMigrated(escrow_id)))
            .unwrap_or(false)
    })
}

#[test]
fn test_migrate_schema_step_migrates_100_escrows_in_5_batches() {
    let env = Env::default();
    env.cost_estimate().budget().reset_unlimited();
    let (client, admin, customer, merchant, token) = create_escrows(&env, 100);

    for step in 1..=5_u64 {
        let cursor = client.migrate_schema_step(&admin, &20);
        assert_eq!(cursor.records_migrated, step * 20);
        assert_eq!(cursor.next_record_id, step * 20 + 1);
        assert_eq!(cursor.target_max_id, 100);
        assert_eq!(cursor.completed, step == 5);
        assert!(migrated(&env, &client, step * 20));
        if step < 5 {
            assert!(!migrated(&env, &client, step * 20 + 1));
            assert_eq!(client.get_schema_version(), 1);
            // User writes are blocked; reads keep working.
            let blocked = client.try_create_escrow(
                &customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
            );
            assert_eq!(blocked, Err(Ok(Error::Basic(BasicError::MigrationInProgress))));
            assert_eq!(client.get_escrow_status(&1), EscrowStatus::Locked);
        }
    }

    assert_eq!(client.get_schema_version(), 2);
    for id in 1..=100 {
        assert!(migrated(&env, &client, id));
    }
    client.create_escrow(
        &customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );
    assert_eq!(
        client.try_migrate_schema_step(&admin, &20),
        Err(Ok(Error::Basic(BasicError::SchemaAlreadyAtTarget)))
    );
}

#[test]
fn test_migrate_schema_step_caps_batch_and_retries_idempotently() {
    let env = Env::default();
    env.cost_estimate().budget().reset_unlimited();
    let (client, admin, ..) = create_escrows(&env, 60);

    let cursor = client.migrate_schema_step(&admin, &100);
    assert_eq!(cursor.records_migrated, MAX_MIGRATION_BATCH as u64);
    assert_eq!(
        client.try_migrate_schema_step(&admin, &0),
        Err(Ok(Error::Basic(BasicError::InvalidMigrationBatch)))
    );

    // Replay from the start: already-migrated escrows are skipped, and the
    // cursor still finishes cleanly.
    env.as_contract(&client.address, || {
        let key = DataKey::Config(ConfigKey::MigrationCursor);
        let mut cursor: MigrationCursor = env.storage().persistent().get(&key).unwrap();
        cursor.next_record_id = 1;
        env.storage().persistent().set(&key, &cursor);
    });
    while !client.get_migration_cursor().unwrap().completed {
        client.migrate_schema_step(&admin, &25);
    }
    assert_eq!(client.get_schema_version(), 2);
    assert!((1..=60).all(|id| migrated(&env, &client, id)));
}

#[test]
fn test_migrate_schema_step_requires_admin() {
    let env = Env::default();
    let (client, ..) = create_escrows(&env, 3);
    assert_eq!(
        client.try_migrate_schema_step(&Address::generate(&env), &10),
        Err(Ok(Error::Basic(BasicError::NotAnAdmin)))
    );
}
