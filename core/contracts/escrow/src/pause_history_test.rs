#![cfg(test)]

use super::*;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn setup() -> (Env, EscrowContractClient<'static>, Address) {
    let env = Env::default();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);
    env.ledger().set_timestamp(1000);
    (env, client, admin)
}

#[test]
fn pause_function_logs_caller_identity_and_reason() {
    let (env, client, admin) = setup();
    let fn_name = String::from_str(&env, "release_escrow");
    let reason = String::from_str(&env, "incident-42");

    env.ledger().set_timestamp(2500);
    client.pause_function(&admin, &fn_name, &reason);

    let history = client.get_function_pause_history(&fn_name);
    assert_eq!(history.len(), 1);
    let entry = history.get(0).unwrap();
    assert_eq!(entry.function_name, fn_name);
    assert_eq!(entry.paused_by, admin);
    assert_eq!(entry.paused_at, 2500);
    assert_eq!(entry.reason, reason);
    assert_eq!(entry.unpaused_by, None);
    assert_eq!(entry.unpaused_at, None);
}

#[test]
fn pause_function_rejects_empty_reason() {
    let (env, client, admin) = setup();
    let fn_name = String::from_str(&env, "release_escrow");
    let reason = String::from_str(&env, "");

    let res = client.try_pause_function(&admin, &fn_name, &reason);
    assert_eq!(res, Err(Ok(Error::Basic(BasicError::Unauthorized))));
}

#[test]
fn unpause_function_populates_unpause_fields() {
    let (env, client, admin) = setup();
    let fn_name = String::from_str(&env, "release_escrow");
    let reason = String::from_str(&env, "maintenance");

    env.ledger().set_timestamp(2000);
    client.pause_function(&admin, &fn_name, &reason);

    env.ledger().set_timestamp(5000);
    client.unpause_function(&admin, &fn_name);

    let history = client.get_function_pause_history(&fn_name);
    assert_eq!(history.len(), 1);
    let entry = history.get(0).unwrap();
    assert_eq!(entry.paused_at, 2000);
    assert_eq!(entry.unpaused_by, Some(admin.clone()));
    assert_eq!(entry.unpaused_at, Some(5000));
    assert_eq!(entry.reason, reason);
}

#[test]
fn get_pause_history_supports_pagination() {
    let (env, client, admin) = setup();
    let reason = String::from_str(&env, "rolling-pause");

    let names = [
        String::from_str(&env, "fn_a"),
        String::from_str(&env, "fn_b"),
        String::from_str(&env, "fn_c"),
        String::from_str(&env, "fn_d"),
        String::from_str(&env, "fn_e"),
    ];

    let mut ts = 1000u64;
    for n in names.iter() {
        ts += 100;
        env.ledger().set_timestamp(ts);
        client.pause_function(&admin, n, &reason);
    }

    let page1 = client.get_pause_history(&2, &0);
    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get(0).unwrap().function_name, names[0]);
    assert_eq!(page1.get(1).unwrap().function_name, names[1]);

    let page2 = client.get_pause_history(&2, &2);
    assert_eq!(page2.len(), 2);
    assert_eq!(page2.get(0).unwrap().function_name, names[2]);
    assert_eq!(page2.get(1).unwrap().function_name, names[3]);

    let page3 = client.get_pause_history(&2, &4);
    assert_eq!(page3.len(), 1);
    assert_eq!(page3.get(0).unwrap().function_name, names[4]);

    let page_off_end = client.get_pause_history(&5, &10);
    assert_eq!(page_off_end.len(), 0);

    let page_zero_limit = client.get_pause_history(&0, &0);
    assert_eq!(page_zero_limit.len(), 0);
}

#[test]
fn get_function_pause_history_filters_by_function_name() {
    let (env, client, admin) = setup();
    let reason = String::from_str(&env, "ops");
    let fn_a = String::from_str(&env, "fn_a");
    let fn_b = String::from_str(&env, "fn_b");

    env.ledger().set_timestamp(2000);
    client.pause_function(&admin, &fn_a, &reason);
    env.ledger().set_timestamp(2100);
    client.pause_function(&admin, &fn_b, &reason);
    env.ledger().set_timestamp(2200);
    client.unpause_function(&admin, &fn_a);
    env.ledger().set_timestamp(2300);
    client.pause_function(&admin, &fn_a, &reason);

    let history_a = client.get_function_pause_history(&fn_a);
    assert_eq!(history_a.len(), 2);
    assert_eq!(history_a.get(0).unwrap().paused_at, 2000);
    assert_eq!(history_a.get(0).unwrap().unpaused_at, Some(2200));
    assert_eq!(history_a.get(1).unwrap().paused_at, 2300);
    assert_eq!(history_a.get(1).unwrap().unpaused_at, None);

    let history_b = client.get_function_pause_history(&fn_b);
    assert_eq!(history_b.len(), 1);
    assert_eq!(history_b.get(0).unwrap().function_name, fn_b);
}

#[test]
fn pause_function_is_idempotent_while_active() {
    let (env, client, admin) = setup();
    let fn_name = String::from_str(&env, "release_escrow");
    let reason = String::from_str(&env, "first");
    let reason2 = String::from_str(&env, "second");

    env.ledger().set_timestamp(2000);
    client.pause_function(&admin, &fn_name, &reason);
    env.ledger().set_timestamp(3000);
    client.pause_function(&admin, &fn_name, &reason2);

    let history = client.get_function_pause_history(&fn_name);
    assert_eq!(history.len(), 1);
    assert_eq!(history.get(0).unwrap().reason, reason);
    assert_eq!(history.get(0).unwrap().paused_at, 2000);
}

#[test]
fn pause_history_caps_retained_entries_at_limit() {
    let (env, client, admin) = setup();
    let reason = String::from_str(&env, "capacity");

    let names = [
        String::from_str(&env, "fn_00"),
        String::from_str(&env, "fn_01"),
        String::from_str(&env, "fn_02"),
        String::from_str(&env, "fn_03"),
        String::from_str(&env, "fn_04"),
        String::from_str(&env, "fn_05"),
        String::from_str(&env, "fn_06"),
        String::from_str(&env, "fn_07"),
        String::from_str(&env, "fn_08"),
        String::from_str(&env, "fn_09"),
        String::from_str(&env, "fn_10"),
        String::from_str(&env, "fn_11"),
        String::from_str(&env, "fn_12"),
        String::from_str(&env, "fn_13"),
        String::from_str(&env, "fn_14"),
        String::from_str(&env, "fn_15"),
        String::from_str(&env, "fn_16"),
        String::from_str(&env, "fn_17"),
        String::from_str(&env, "fn_18"),
        String::from_str(&env, "fn_19"),
        String::from_str(&env, "fn_20"),
        String::from_str(&env, "fn_21"),
        String::from_str(&env, "fn_22"),
        String::from_str(&env, "fn_23"),
        String::from_str(&env, "fn_24"),
    ];

    let mut ts = 1000u64;
    for n in names.iter() {
        ts += 100;
        env.ledger().set_timestamp(ts);
        client.pause_function(&admin, n, &reason);
    }

    // 25 pause events were recorded, but only the most recent 20 are retained.
    let retained = client.get_pause_history(&25, &0);
    assert_eq!(retained.len(), 20);
    assert_eq!(retained.get(0).unwrap().function_name, names[5]);
    assert_eq!(retained.get(19).unwrap().function_name, names[24]);

    // The five oldest events have been pruned from storage.
    for idx in 0..5 {
        let pruned = client.get_function_pause_history(&names[idx]);
        assert_eq!(pruned.len(), 0);
    }

    // Pagination is relative to the retained window and stops at the cap.
    let newest = client.get_pause_history(&1, &19);
    assert_eq!(newest.len(), 1);
    assert_eq!(newest.get(0).unwrap().function_name, names[24]);

    let past_window = client.get_pause_history(&5, &20);
    assert_eq!(past_window.len(), 0);
}

#[test]
fn unpause_of_pruned_entry_does_not_corrupt_recycled_slot() {
    let (env, client, admin) = setup();
    let reason = String::from_str(&env, "churn");
    let oldest = String::from_str(&env, "fn_oldest");

    env.ledger().set_timestamp(1000);
    client.pause_function(&admin, &oldest, &reason);

    let names = [
        String::from_str(&env, "fn_01"),
        String::from_str(&env, "fn_02"),
        String::from_str(&env, "fn_03"),
        String::from_str(&env, "fn_04"),
        String::from_str(&env, "fn_05"),
        String::from_str(&env, "fn_06"),
        String::from_str(&env, "fn_07"),
        String::from_str(&env, "fn_08"),
        String::from_str(&env, "fn_09"),
        String::from_str(&env, "fn_10"),
        String::from_str(&env, "fn_11"),
        String::from_str(&env, "fn_12"),
        String::from_str(&env, "fn_13"),
        String::from_str(&env, "fn_14"),
        String::from_str(&env, "fn_15"),
        String::from_str(&env, "fn_16"),
        String::from_str(&env, "fn_17"),
        String::from_str(&env, "fn_18"),
        String::from_str(&env, "fn_19"),
        String::from_str(&env, "fn_20"),
    ];

    let mut ts = 1000u64;
    for n in names.iter() {
        ts += 100;
        env.ledger().set_timestamp(ts);
        client.pause_function(&admin, n, &reason);
    }

    // The oldest entry's slot has been recycled by fn_20; unpausing the
    // evicted function must not overwrite the newer entry's unpause fields.
    env.ledger().set_timestamp(9000);
    client.unpause_function(&admin, &oldest);

    let retained = client.get_pause_history(&20, &0);
    assert_eq!(retained.len(), 20);
    let newest = retained.get(19).unwrap();
    assert_eq!(newest.function_name, names[19]);
    assert_eq!(newest.unpaused_by, None);
    assert_eq!(newest.unpaused_at, None);
}
