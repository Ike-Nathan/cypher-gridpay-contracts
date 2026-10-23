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
    let contract_id = env.register(EscrowContract, ());
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
    let contract_id = env.register(EscrowContract, ());
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
    let contract_id = env.register(EscrowContract, ());
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
    let contract_id = env.register(EscrowContract, ());
    let key = Symbol::new(&env, "missing");

    env.as_contract(&contract_id, || {
        assert_eq!(storage::get_persistent::<_, u64>(&env, &key), None);
        assert!(!storage::has_persistent(&env, &key));
    });
}

#[test]
fn test_extend_instance_extends_ttl() {
    let env = Env::default();
    let contract_id = env.register(EscrowContract, ());

    env.as_contract(&contract_id, || {
        storage::extend_instance(&env);
        assert_eq!(
            env.storage().instance().get_ttl(),
            storage::INSTANCE_BUMP_AMOUNT
        );
    });
}

#[test]
fn test_get_escrow_extends_instance_ttl() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&customer, &10_000);

    let escrow_id = client.create_escrow(
        &customer, &merchant, &500_i128, &token, &0_u64, &0_u64, &0_u64, &false,
    );
    client.get_escrow(&escrow_id);

    let instance_ttl = || env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    // A locked escrow pins the instance (where escrows live) at the
    // CriticalActiveLock tier, not the 30-day bump.
    let pinned = storage::StorageTier::CriticalActiveLock.bump_amount();
    assert_eq!(instance_ttl(), pinned);

    // Run the TTL down below the normal instance threshold.
    advance_ledgers(&env, pinned - storage::INSTANCE_LIFETIME_THRESHOLD + 1);
    assert!(instance_ttl() < storage::INSTANCE_LIFETIME_THRESHOLD);

    client.get_escrow(&escrow_id);
    assert_eq!(instance_ttl(), storage::INSTANCE_BUMP_AMOUNT);
}

// ── Tiered, deadline-aware TTL ───────────────────────────────────────────────

mod tiered_ttl {
    use super::advance_ledgers;
    use crate::storage::{self, StorageTier, DAY_IN_LEDGERS, MIN_SECONDS_PER_LEDGER};
    use crate::*;
    use soroban_sdk::testutils::storage::Instance as _;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{token, vec, Address, Env, Vec};

    const DAY_SECS: u64 = 86_400;

    fn instance_ttl(env: &Env, contract: &Address) -> u32 {
        env.as_contract(contract, || env.storage().instance().get_ttl())
    }

    fn setup(env: &Env) -> (EscrowContractClient<'_>, Address, Address, Address, Address) {
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        // Room for long-dated escrows (the default max is ~1 year).
        env.ledger().set_max_entry_ttl(3 * 6_312_000);
        let contract_id = env.register(EscrowContract, ());
        let client = EscrowContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(&admin);
        let customer = Address::generate(env);
        let token = env.register_stellar_asset_contract_v2(admin.clone()).address();
        token::StellarAssetClient::new(env, &token).mint(&customer, &1_000_000);
        (client, contract_id, admin, customer, token)
    }

    /// Ledgers needed to reach `deadline + grace`, at real 5 s ledgers.
    fn ledgers_at_5s(env: &Env, deadline: u64, grace: u64) -> u32 {
        ((deadline + grace - env.ledger().timestamp()) / 5) as u32
    }

    #[test]
    fn tiers_map_escrow_states() {
        assert_eq!(
            EscrowContract::escrow_storage_tier(&EscrowStatus::Locked),
            StorageTier::CriticalActiveLock
        );
        assert_eq!(
            EscrowContract::escrow_storage_tier(&EscrowStatus::Disputed),
            StorageTier::CriticalActiveLock
        );
        for done in [
            EscrowStatus::Released,
            EscrowStatus::Resolved,
            EscrowStatus::Cancelled,
            EscrowStatus::ExpiredRefunded,
        ] {
            assert_eq!(
                EscrowContract::escrow_storage_tier(&done),
                StorageTier::TerminalAuditRecord
            );
        }
        // Critical entries get the longest bump and highest threshold.
        assert!(StorageTier::CriticalActiveLock.bump_amount() > StorageTier::StandardActive.bump_amount());
        assert!(StorageTier::StandardActive.bump_amount() > StorageTier::TerminalAuditRecord.bump_amount());
        assert!(StorageTier::TerminalAuditRecord.bump_amount() > StorageTier::EphemeralWindow.bump_amount());
    }

    #[test]
    fn ledger_conversion_is_conservative() {
        let env = Env::default();
        env.ledger().set_timestamp(1_000);
        // 4 s per ledger, rounded up, so it is never short of real 5 s ledgers.
        assert_eq!(storage::ledgers_until(&env, 1_400), 100);
        assert_eq!(storage::ledgers_until(&env, 1_401), 101);
        assert_eq!(storage::ledgers_until(&env, 500), 0);
        assert_eq!(MIN_SECONDS_PER_LEDGER, 4);
    }

    #[test]
    fn long_dated_escrow_outlives_its_deadline_plus_dispute_grace() {
        let env = Env::default();
        let (client, contract_id, _, customer, token) = setup(&env);
        let merchant = Address::generate(&env);
        let release = env.ledger().timestamp() + 400 * DAY_SECS; // past the 120-day bump
        let id = client.create_escrow(&customer, &merchant, &500_i128, &token, &release, &0, &0, &false);
        let escrow = env.as_contract(&contract_id, || EscrowContract::get_escrow(&env, id));

        let grace = escrow.escalation_timeout.max(259_200);
        let ttl = instance_ttl(&env, &contract_id);
        assert!(ttl >= ledgers_at_5s(&env, release, grace), "TTL {ttl} ends before the deadline");
        assert!(ttl > StorageTier::CriticalActiveLock.bump_amount());

        // Walk to just past the deadline + grace: the escrow is still there.
        let elapsed_ledgers = ledgers_at_5s(&env, release, grace);
        advance_ledgers(&env, elapsed_ledgers);
        env.ledger().set_timestamp(release + grace);
        assert!(instance_ttl(&env, &contract_id) > 0);
        assert_eq!(client.get_escrow_status(&id), EscrowStatus::Locked);
    }

    #[test]
    fn deadline_past_the_network_maximum_is_capped() {
        let env = Env::default();
        let (client, contract_id, _, customer, token) = setup(&env);
        let release = env.ledger().timestamp() + 20 * 365 * DAY_SECS;
        client.create_escrow(&customer, &Address::generate(&env), &500_i128, &token, &release, &0, &0, &false);
        assert_eq!(instance_ttl(&env, &contract_id), env.storage().max_ttl());
    }

    #[test]
    fn batch_bump_restores_coverage_for_critical_escrows_only() {
        let env = Env::default();
        let (client, contract_id, admin, customer, token) = setup(&env);
        let merchant = Address::generate(&env);
        let release = env.ledger().timestamp() + 300 * DAY_SECS;
        let locked = client.create_escrow(&customer, &merchant, &500_i128, &token, &release, &0, &0, &false);
        let released = client.create_escrow(&customer, &merchant, &500_i128, &token, &0, &0, &0, &false);
        client.release_escrow(&admin, &released, &false);

        // Time passes and the TTL runs down.
        advance_ledgers(&env, 60 * DAY_IN_LEDGERS);
        env.ledger().set_timestamp(env.ledger().timestamp() + 60 * DAY_SECS / 2);
        let before = instance_ttl(&env, &contract_id);

        let bumped = client.batch_bump_critical(&vec![&env, locked, released, 999]);
        assert_eq!(bumped, 1, "only the locked escrow is critical");
        let after = instance_ttl(&env, &contract_id);
        assert!(after > before);
        let escrow = env.as_contract(&contract_id, || EscrowContract::get_escrow(&env, locked));
        assert!(after >= ledgers_at_5s(&env, release, escrow.escalation_timeout.max(259_200)));
    }

    #[test]
    fn batch_bump_rejects_oversized_batches() {
        let env = Env::default();
        let (client, ..) = setup(&env);
        let mut ids = Vec::new(&env);
        for id in 0..(MAX_TTL_BUMP_BATCH as u64 + 1) {
            ids.push_back(id);
        }
        assert_eq!(
            client.try_batch_bump_critical(&ids),
            Err(Ok(Error::Escrow(EscrowError::BatchTooLarge)))
        );
    }
}
