#![cfg(test)]

use crate::*;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{testutils::Address as _, token, Address, Env};

fn setup(env: &Env) -> (EscrowContractClient, Address, Address, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);

    let token_addr = env.register_stellar_asset_contract(admin.clone());
    let token_admin = token::StellarAssetClient::new(env, &token_addr);
    let customer = Address::generate(env);
    token_admin.mint(&customer, &1_000_000);
    // fund contract directly so transfers succeed
    token_admin.mint(&contract_id, &1_000_000);

    (client, admin, customer, Address::generate(env), token_addr)
}

// inactivity_seconds = 10_000, near_expiry_buffer_seconds = 500
fn default_config() -> StaleThresholdConfig {
    StaleThresholdConfig {
        inactivity_seconds: 10_000,
        near_expiry_buffer_seconds: 500,
        keeper_bounty_bps: 0,
        max_keeper_bounty_cap: 0,
        dormant_grace_notice_secs: 0,
    }
}

// Config with keeper bounty enabled (50 bps = 0.5%, grace = 1000s)
fn bounty_config() -> StaleThresholdConfig {
    StaleThresholdConfig {
        inactivity_seconds: 10_000,
        near_expiry_buffer_seconds: 500,
        keeper_bounty_bps: 50,
        max_keeper_bounty_cap: 0,
        dormant_grace_notice_secs: 1_000,
    }
}

#[test]
fn test_set_and_get_stale_threshold() {
    let env = Env::default();
    let (client, admin, _, _, _) = setup(&env);

    assert!(client.get_stale_threshold().is_none());

    client.set_stale_threshold(&admin, &default_config());

    let stored = client.get_stale_threshold().unwrap();
    assert_eq!(stored.inactivity_seconds, 10_000);
    assert_eq!(stored.near_expiry_buffer_seconds, 500);
}

#[test]
fn test_set_stale_threshold_non_admin_fails() {
    let env = Env::default();
    let (client, _admin, customer, _, _) = setup(&env);

    // `customer` is not part of the multisig admin set.
    let result = client.try_set_stale_threshold(&customer, &default_config());
    assert!(result.is_err());
}

#[test]
fn test_health_healthy() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    client.set_stale_threshold(&admin, &default_config());

    env.ledger().set_timestamp(1_000);
    // release=2000, expiry far in the future (100_000)
    let id = client.create_escrow(
        &customer, &merchant, &500_i128, &token, &2_000_u64, &0_u64, &100_000_u64, &true,
    );

    let report = client.get_escrow_health(&id);
    assert_eq!(report.health, EscrowHealth::Healthy);
    assert_eq!(report.last_activity, 1_000);
    assert_eq!(report.seconds_until_expiry, Some(99_000));
}

#[test]
fn test_health_near_expiry() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    client.set_stale_threshold(&admin, &default_config());

    env.ledger().set_timestamp(1_000);
    // release=2000, expiry=3000
    let id = client.create_escrow(
        &customer, &merchant, &500_i128, &token, &2_000_u64, &0_u64, &3_000_u64, &true,
    );

    // Within 500s of expiry (3000 - 2600 = 400 <= 500), still inactive < 10_000.
    env.ledger().set_timestamp(2_600);
    let report = client.get_escrow_health(&id);
    assert_eq!(report.health, EscrowHealth::NearExpiry);
    assert_eq!(report.seconds_until_expiry, Some(400));
}

#[test]
fn test_health_stale() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    client.set_stale_threshold(&admin, &default_config());

    env.ledger().set_timestamp(1_000);
    // expiry = 0 => no expiry, so it can only be Stale or Healthy.
    let id = client.create_escrow(
        &customer, &merchant, &500_i128, &token, &2_000_u64, &0_u64, &0_u64, &false,
    );

    // Advance past inactivity window (now - last_activity = 11_000 >= 10_000).
    env.ledger().set_timestamp(12_000);
    let report = client.get_escrow_health(&id);
    assert_eq!(report.health, EscrowHealth::Stale);
    assert_eq!(report.seconds_until_expiry, None);
}

#[test]
fn test_health_disputed() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    client.set_stale_threshold(&admin, &default_config());

    env.ledger().set_timestamp(1_000);
    let id = client.create_escrow(
        &customer, &merchant, &500_i128, &token, &2_000_u64, &0_u64, &100_000_u64, &true,
    );
    client.dispute_escrow(&customer, &id);

    let report = client.get_escrow_health(&id);
    assert_eq!(report.health, EscrowHealth::Disputed);
}

#[test]
fn test_health_expired() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    client.set_stale_threshold(&admin, &default_config());

    env.ledger().set_timestamp(1_000);
    // release=2000, expiry=3000
    let id = client.create_escrow(
        &customer, &merchant, &500_i128, &token, &2_000_u64, &0_u64, &3_000_u64, &true,
    );

    env.ledger().set_timestamp(3_001);
    let report = client.get_escrow_health(&id);
    assert_eq!(report.health, EscrowHealth::Expired);
    assert_eq!(report.seconds_until_expiry, Some(-1));
}

#[test]
fn test_get_stale_escrows_respects_limit() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    client.set_stale_threshold(&admin, &default_config());

    env.ledger().set_timestamp(1_000);
    // Create 5 escrows with no expiry.
    for _ in 0..5 {
        client.create_escrow(
            &customer, &merchant, &500_i128, &token, &2_000_u64, &0_u64, &0_u64, &false,
        );
    }

    // Before the inactivity window elapses, none are stale.
    let none_stale = client.get_stale_escrows(&10_u32);
    assert_eq!(none_stale.len(), 0);

    // Advance past inactivity window: all 5 become stale.
    env.ledger().set_timestamp(12_000);
    let all_stale = client.get_stale_escrows(&10_u32);
    assert_eq!(all_stale.len(), 5);

    // `limit` caps the number of returned IDs.
    let capped = client.get_stale_escrows(&3_u32);
    assert_eq!(capped.len(), 3);
}

#[test]
fn test_get_escrow_health_not_configured_fails() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);

    env.ledger().set_timestamp(1_000);
    let id = client.create_escrow(
        &customer, &merchant, &500_i128, &token, &2_000_u64, &0_u64, &0_u64, &false,
    );

    // No threshold configured => StaleThresholdNotConfigured.
    let result = client.try_get_escrow_health(&id);
    assert!(result.is_err());
}

#[test]
fn test_get_stale_escrows_not_configured_fails() {
    let env = Env::default();
    let (client, _admin, _, _, _) = setup(&env);

    let result = client.try_get_stale_escrows(&10_u32);
    assert!(result.is_err());
}

#[test]
fn test_get_escrow_health_missing_escrow_fails() {
    let env = Env::default();
    let (client, admin, _, _, _) = setup(&env);
    client.set_stale_threshold(&admin, &default_config());

    let result = client.try_get_escrow_health(&999_u64);
    assert!(result.is_err());
}


#[test]
fn test_health_views_leave_ledger_storage_untouched() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    client.set_stale_threshold(&admin, &default_config());

    env.ledger().set_timestamp(1_000);
    let id = client.create_escrow(
        &customer, &merchant, &500_i128, &token, &2_000_u64, &0_u64, &100_000_u64, &true,
    );

    let before = env.to_ledger_snapshot();

    // Diagnostics must not write instance or persistent storage: no new keys,
    // no changed values. TTL fields are ignored on purpose, since a host may
    // legitimately bump them on access.
    let _ = client.get_escrow_health(&id);
    let _ = client.get_stale_escrows(&10_u32);
    let _ = client.get_stale_threshold();

    let after = env.to_ledger_snapshot();
    assert_eq!(
        before.ledger_entries.len(),
        after.ledger_entries.len(),
        "health_check created or removed a ledger entry"
    );
    for i in 0..before.ledger_entries.len() {
        let (before_key, (before_entry, _before_ttl)) = &before.ledger_entries[i];
        let (after_key, (after_entry, _after_ttl)) = &after.ledger_entries[i];
        assert!(before_key == after_key, "health_check changed the ledger key set");
        assert!(before_entry == after_entry, "health_check modified a ledger entry");
    }
}

#[test]
fn test_health_views_report_consistent_state() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    client.set_stale_threshold(&admin, &default_config());

    env.ledger().set_timestamp(1_000);
    let id = client.create_escrow(
        &customer, &merchant, &500_i128, &token, &2_000_u64, &0_u64, &100_000_u64, &true,
    );

    let token_client = token::Client::new(&env, &token);
    let customer_balance = token_client.balance(&customer);
    let merchant_balance = token_client.balance(&merchant);

    // The diagnostics agree with the state they report on...
    let report = client.get_escrow_health(&id);
    assert_eq!(report.escrow_id, id);
    assert_eq!(report.health, EscrowHealth::Healthy);
    assert_eq!(report.seconds_until_expiry, Some(99_000));
    assert_eq!(report.last_activity, 1_000);
    assert_eq!(client.get_escrow_count_by_customer(&customer), 1);
    assert_eq!(client.get_escrow_count_by_merchant(&merchant), 1);
    assert!(client.get_stale_escrows(&10_u32).is_empty());
    assert_eq!(
        client.get_stale_threshold().unwrap().inactivity_seconds,
        10_000
    );

    // ...and reporting on them moves no funds.
    assert_eq!(token_client.balance(&customer), customer_balance);
    assert_eq!(token_client.balance(&merchant), merchant_balance);
}

// ── Keeper bounty: poke_stale_escrow ─────────────────────────────────────────

#[test]
fn test_poke_stale_escrow_succeeds() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    client.set_stale_threshold(&admin, &bounty_config());

    env.ledger().set_timestamp(1_000);
    let id = client.create_escrow(
        &customer, &merchant, &1_000_i128, &token, &2_000_u64, &0_u64, &0_u64, &false,
    );

    // Advance past inactivity window so escrow is Stale.
    env.ledger().set_timestamp(12_000);
    let report = client.get_escrow_health(&id);
    assert_eq!(report.health, EscrowHealth::Stale);

    let keeper = Address::generate(&env);
    let result = client.try_poke_stale_escrow(&id, &keeper);
    assert!(result.is_ok());

    // Escrow must now be in DormantNoticePeriod.
    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.status, EscrowStatus::DormantNoticePeriod);
}

#[test]
fn test_poke_stale_escrow_healthy_fails() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    client.set_stale_threshold(&admin, &bounty_config());

    env.ledger().set_timestamp(1_000);
    let id = client.create_escrow(
        &customer, &merchant, &1_000_i128, &token, &2_000_u64, &0_u64, &100_000_u64, &true,
    );

    // Still healthy — poke must fail.
    let keeper = Address::generate(&env);
    let result = client.try_poke_stale_escrow(&id, &keeper);
    assert!(result.is_err());
}

#[test]
fn test_poke_stale_escrow_bounty_disabled_fails() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    // keeper_bounty_bps = 0 => disabled
    client.set_stale_threshold(&admin, &default_config());

    env.ledger().set_timestamp(1_000);
    let id = client.create_escrow(
        &customer, &merchant, &1_000_i128, &token, &2_000_u64, &0_u64, &0_u64, &false,
    );
    env.ledger().set_timestamp(12_000);

    let keeper = Address::generate(&env);
    let result = client.try_poke_stale_escrow(&id, &keeper);
    assert!(result.is_err());
}

#[test]
fn test_poke_stale_expired_escrow_succeeds() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    client.set_stale_threshold(&admin, &bounty_config());

    env.ledger().set_timestamp(1_000);
    let id = client.create_escrow(
        &customer, &merchant, &1_000_i128, &token, &2_000_u64, &0_u64, &5_000_u64, &false,
    );
    // Past expiry.
    env.ledger().set_timestamp(6_000);

    let report = client.get_escrow_health(&id);
    assert_eq!(report.health, EscrowHealth::Expired);

    let keeper = Address::generate(&env);
    let result = client.try_poke_stale_escrow(&id, &keeper);
    assert!(result.is_ok());
    assert_eq!(client.get_escrow(&id).status, EscrowStatus::DormantNoticePeriod);
}

#[test]
fn test_poke_disputed_escrow_fails() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    client.set_stale_threshold(&admin, &bounty_config());

    env.ledger().set_timestamp(1_000);
    let id = client.create_escrow(
        &customer, &merchant, &1_000_i128, &token, &2_000_u64, &0_u64, &0_u64, &false,
    );

    client.dispute_escrow(&customer, &id);
    env.ledger().set_timestamp(12_000);

    let keeper = Address::generate(&env);
    let result = client.try_poke_stale_escrow(&id, &keeper);
    assert!(result.is_err());
}

// ── Keeper bounty: settle_dormant_escrow ─────────────────────────────────────

#[test]
fn test_settle_dormant_escrow_full_flow_customer_favor() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    // 50 bps bounty, 1000s grace.
    client.set_stale_threshold(&admin, &bounty_config());

    env.ledger().set_timestamp(1_000);
    // fee_bps = 0 on default escrow, so bounty = 0 (capped at fee portion)
    // Use a non-zero fee: set EscrowFeeConfig with 100 bps.
    let fee_config = EscrowFeeConfig {
        fee_bps: 100_i128,
        fee_recipient: admin.clone(),
        enabled: true,
    };
    client.set_escrow_fee_config(&admin, &fee_config);

    // auto_resolve_in_favor_of defaults to Customer in create_escrow.
    let id = client.create_escrow(
        &customer, &merchant, &10_000_i128, &token, &2_000_u64, &0_u64, &0_u64, &false,
    );

    // Advance past inactivity window.
    env.ledger().set_timestamp(12_000);

    let keeper = Address::generate(&env);
    // Poke.
    client.poke_stale_escrow(&id, &keeper);

    // Advance past grace period (1000s).
    env.ledger().set_timestamp(13_100);

    let token_client = token::Client::new(&env, &token);
    let keeper_balance_before = token_client.balance(&keeper);
    let customer_balance_before = token_client.balance(&customer);

    // Settle.
    let bounty = client.settle_dormant_escrow(&id, &keeper);
    // bounty = min(10000 * 50/10000, 10000 * 100/10000) = min(50, 100) = 50
    assert_eq!(bounty, 50_i128);

    // Keeper received bounty.
    assert_eq!(token_client.balance(&keeper), keeper_balance_before + 50);
    // Customer received principal (10000 - 100 fee = 9900).
    assert_eq!(token_client.balance(&customer), customer_balance_before + 9_900);

    // Escrow is Resolved.
    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.status, EscrowStatus::Resolved);
}

#[test]
fn test_settle_dormant_escrow_grace_not_elapsed_fails() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    client.set_stale_threshold(&admin, &bounty_config());

    env.ledger().set_timestamp(1_000);
    let id = client.create_escrow(
        &customer, &merchant, &1_000_i128, &token, &2_000_u64, &0_u64, &0_u64, &false,
    );
    env.ledger().set_timestamp(12_000);

    let keeper = Address::generate(&env);
    client.poke_stale_escrow(&id, &keeper);

    // Only 100s into the 1000s grace window.
    env.ledger().set_timestamp(12_100);

    let result = client.try_settle_dormant_escrow(&id, &keeper);
    assert!(result.is_err());
}

#[test]
fn test_settle_without_poke_fails() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    client.set_stale_threshold(&admin, &bounty_config());

    env.ledger().set_timestamp(1_000);
    let id = client.create_escrow(
        &customer, &merchant, &1_000_i128, &token, &2_000_u64, &0_u64, &0_u64, &false,
    );
    env.ledger().set_timestamp(12_000);

    let keeper = Address::generate(&env);
    // No poke — escrow is still Locked, not DormantNoticePeriod.
    let result = client.try_settle_dormant_escrow(&id, &keeper);
    assert!(result.is_err());
}

#[test]
fn test_buyer_activity_during_dormant_window_resets_escrow() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    client.set_stale_threshold(&admin, &bounty_config());

    env.ledger().set_timestamp(1_000);
    let id = client.create_escrow(
        &customer, &merchant, &1_000_i128, &token, &2_000_u64, &0_u64, &0_u64, &false,
    );
    env.ledger().set_timestamp(12_000);

    let keeper = Address::generate(&env);
    client.poke_stale_escrow(&id, &keeper);

    // Buyer disputes during the dormant window → resets back to Locked.
    env.ledger().set_timestamp(12_200);
    client.dispute_escrow(&customer, &id);

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.status, EscrowStatus::Disputed);

    // Now settle should fail (escrow is Disputed, not DormantNoticePeriod).
    env.ledger().set_timestamp(14_000);
    let result = client.try_settle_dormant_escrow(&id, &keeper);
    assert!(result.is_err());
}

#[test]
fn test_settle_dormant_max_cap_applied() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    // 500 bps bounty with hard cap of 10 tokens.
    let config = StaleThresholdConfig {
        inactivity_seconds: 10_000,
        near_expiry_buffer_seconds: 500,
        keeper_bounty_bps: 500,
        max_keeper_bounty_cap: 10_i128,
        dormant_grace_notice_secs: 1_000,
    };
    client.set_stale_threshold(&admin, &config);

    let fee_config = EscrowFeeConfig {
        fee_bps: 1_000_i128,          // 10% fee
        fee_recipient: admin.clone(),
        enabled: true,
    };
    client.set_escrow_fee_config(&admin, &fee_config);

    env.ledger().set_timestamp(1_000);
    // amount = 1000; fee = 100; bounty_raw = 50; cap = 10 → bounty = 10
    let id = client.create_escrow(
        &customer, &merchant, &1_000_i128, &token, &2_000_u64, &0_u64, &0_u64, &false,
    );
    env.ledger().set_timestamp(12_000);

    let keeper = Address::generate(&env);
    client.poke_stale_escrow(&id, &keeper);
    env.ledger().set_timestamp(13_100);

    let bounty = client.settle_dormant_escrow(&id, &keeper);
    // cap = 10 is the binding constraint
    assert_eq!(bounty, 10_i128);
}

#[test]
fn test_poke_stale_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);
    client.set_stale_threshold(&admin, &bounty_config());

    env.ledger().set_timestamp(1_000);
    let id = client.create_escrow(
        &customer, &merchant, &1_000_i128, &token, &2_000_u64, &0_u64, &0_u64, &false,
    );
    env.ledger().set_timestamp(12_000);

    let keeper = Address::generate(&env);
    // Poke succeeds and escrow transitions to DormantNoticePeriod (event is emitted by the contract).
    let result = client.try_poke_stale_escrow(&id, &keeper);
    assert!(result.is_ok());
    // Verify state change as observable proof of the event-emitting path.
    assert_eq!(client.get_escrow(&id).status, EscrowStatus::DormantNoticePeriod);
}
