#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, String,
};

#[test]
fn test_refund_cooldown_blocks_second_request() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = Address::generate(&env);

    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    client.set_refund_cooldown_config(
        &admin,
        &RefundCooldownConfig {
            cooldown_seconds: 3600,
            enabled: true,
            max_attempts: 0,
            backoff_multiplier: 1,
        },
    );

    env.ledger().set_timestamp(1000);
    client.request_refund(
        &merchant,
        &1u64,
        &customer,
        &100i128,
        &1000i128,
        &token,
        &String::from_str(&env, "first"),
        &RefundReasonCode::Other,
        &1000u64,
    );

    env.ledger().set_timestamp(2000);
    let result = client.try_request_refund(
        &merchant,
        &2u64,
        &customer,
        &100i128,
        &1000i128,
        &token,
        &String::from_str(&env, "second"),
        &RefundReasonCode::Other,
        &1000u64,
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        Error::Core(CoreError::RefundCooldownActive)
    );
}

#[test]
fn test_refund_cooldown_allows_after_window() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = Address::generate(&env);

    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    client.set_refund_cooldown_config(
        &admin,
        &RefundCooldownConfig {
            cooldown_seconds: 3600,
            enabled: true,
            max_attempts: 0,
            backoff_multiplier: 1,
        },
    );

    env.ledger().set_timestamp(1000);
    client.request_refund(
        &merchant,
        &1u64,
        &customer,
        &100i128,
        &1000i128,
        &token,
        &String::from_str(&env, "first"),
        &RefundReasonCode::Other,
        &1000u64,
    );

    env.ledger().set_timestamp(5000);
    client.request_refund(
        &merchant,
        &2u64,
        &customer,
        &100i128,
        &1000i128,
        &token,
        &String::from_str(&env, "second"),
        &RefundReasonCode::Other,
        &1000u64,
    );
}

const DAY: u64 = 24 * 60 * 60;

fn request_on_payment(
    client: &RefundContractClient,
    env: &Env,
    merchant: &Address,
    customer: &Address,
    token: &Address,
    payment_id: u64,
) -> Result<u64, Error> {
    match client.try_request_refund(
        merchant,
        &payment_id,
        customer,
        &100,
        &1_000,
        token,
        &String::from_str(env, "please refund"),
        &RefundReasonCode::Other,
        &env.ledger().timestamp(),
    ) {
        Ok(Ok(id)) => Ok(id),
        Err(Ok(e)) => Err(e),
        other => panic!("unexpected result: {:?}", other),
    }
}

/// Denies a refund on `payment_id` and finalizes the denial. Returns the
/// time the denial became final (the cooldown anchor).
fn deny_and_finalize(
    client: &RefundContractClient,
    env: &Env,
    admin: &Address,
    refund_id: u64,
) -> u64 {
    client.reject_refund(admin, &refund_id, &String::from_str(env, "denied"));
    let deadline = client.get_refund(&refund_id).appeal_deadline.unwrap();
    env.ledger().set_timestamp(deadline);
    client.finalize_denial(&refund_id);
    deadline
}

#[test]
fn test_payment_rejection_cooldown_blocks_then_allows() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = Address::generate(&env);

    assert_eq!(client.get_payment_rejection_cooldown(), DAY); // default 24h

    let refund_id = request_on_payment(&client, &env, &merchant, &customer, &token, 42).unwrap();
    let denied_at = deny_and_finalize(&client, &env, &admin, refund_id);
    assert_eq!(client.get_last_rejected_attempt(&42), Some(denied_at));

    // Within the cooldown: rejected, even from a fresh customer address.
    for offset in [0, DAY / 2, DAY - 1] {
        env.ledger().set_timestamp(denied_at + offset);
        assert_eq!(
            request_on_payment(&client, &env, &merchant, &customer, &token, 42),
            Err(Error::Core(CoreError::RefundCooldownActive))
        );
        assert_eq!(
            request_on_payment(
                &client,
                &env,
                &merchant,
                &Address::generate(&env),
                &token,
                42
            ),
            Err(Error::Core(CoreError::RefundCooldownActive))
        );
    }

    // Other payments are unaffected by this payment's cooldown.
    request_on_payment(&client, &env, &merchant, &customer, &token, 43).unwrap();

    // Once the cooldown has elapsed the payment can be refunded again.
    env.ledger().set_timestamp(denied_at + DAY);
    request_on_payment(&client, &env, &merchant, &customer, &token, 42).unwrap();
}

#[test]
fn test_payment_rejection_cooldown_is_configurable() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = Address::generate(&env);

    client.set_payment_rejection_cooldown(&admin, &3_600);
    let refund_id = request_on_payment(&client, &env, &merchant, &customer, &token, 7).unwrap();
    let denied_at = deny_and_finalize(&client, &env, &admin, refund_id);

    env.ledger().set_timestamp(denied_at + 3_599);
    assert_eq!(
        request_on_payment(&client, &env, &merchant, &customer, &token, 7),
        Err(Error::Core(CoreError::RefundCooldownActive))
    );
    env.ledger().set_timestamp(denied_at + 3_600);
    request_on_payment(&client, &env, &merchant, &customer, &token, 7).unwrap();

    assert_eq!(
        client.try_set_payment_rejection_cooldown(&merchant, &0),
        Err(Ok(Error::Core(CoreError::Unauthorized)))
    );
}

#[test]
fn test_expired_request_does_not_start_payment_cooldown() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = Address::generate(&env);
    client.set_refund_ttl_config(&admin, &3_600);

    // A request that merely went stale was never denied, so no cooldown.
    let refund_id = request_on_payment(&client, &env, &merchant, &customer, &token, 9).unwrap();
    env.ledger().set_timestamp(1_000 + 3_600);
    client.expire_stale_refund(&refund_id);
    assert_eq!(client.get_last_rejected_attempt(&9), None);
    request_on_payment(&client, &env, &merchant, &customer, &token, 9).unwrap();
}

/// Issue #4: After each finalized denial the customer's effective cooldown
/// doubles (multiplier = 2).  After a successful refund the counter resets.
#[test]
fn test_exponential_backoff_grows_with_each_denial() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = Address::generate(&env);

    // Base cooldown = 1 hour, multiplier = 2, no cap.
    const BASE: u64 = 3_600;
    client.set_refund_cooldown_config(
        &admin,
        &RefundCooldownConfig {
            cooldown_seconds: BASE,
            enabled: true,
            max_attempts: 0,
            backoff_multiplier: 2,
        },
    );

    // --- Attempt 1: request → deny → finalize ---
    // First attempt: effective cooldown = 3600 (no previous denials yet).
    env.ledger().set_timestamp(1_000);
    let rid1 = request_on_payment(&client, &env, &merchant, &customer, &token, 100).unwrap();
    let denied_at_1 = deny_and_finalize(&client, &env, &admin, rid1);

    // Immediately after denial: blocked.
    env.ledger().set_timestamp(denied_at_1 + BASE - 1);
    assert_eq!(
        request_on_payment(&client, &env, &merchant, &customer, &token, 100),
        Err(Error::Core(CoreError::RefundCooldownActive))
    );
    // After base window: allowed (1 denial so exponent = 0, still base).
    env.ledger().set_timestamp(denied_at_1 + BASE);
    let rid2 = request_on_payment(&client, &env, &merchant, &customer, &token, 101).unwrap();

    // --- Attempt 2: deny again → effective cooldown = 3600 × 2 = 7200 ---
    let denied_at_2 = deny_and_finalize(&client, &env, &admin, rid2);

    env.ledger().set_timestamp(denied_at_2 + 2 * BASE - 1);
    assert_eq!(
        request_on_payment(&client, &env, &merchant, &customer, &token, 101),
        Err(Error::Core(CoreError::RefundCooldownActive))
    );
    env.ledger().set_timestamp(denied_at_2 + 2 * BASE);
    let rid3 = request_on_payment(&client, &env, &merchant, &customer, &token, 102).unwrap();

    // --- Attempt 3: deny again → effective cooldown = 3600 × 4 = 14400 ---
    let denied_at_3 = deny_and_finalize(&client, &env, &admin, rid3);

    env.ledger().set_timestamp(denied_at_3 + 4 * BASE - 1);
    assert_eq!(
        request_on_payment(&client, &env, &merchant, &customer, &token, 102),
        Err(Error::Core(CoreError::RefundCooldownActive))
    );
    // After 4× window the customer can try again.
    env.ledger().set_timestamp(denied_at_3 + 4 * BASE);
    request_on_payment(&client, &env, &merchant, &customer, &token, 103).unwrap();
}

/// Issue #4: Capping backoff at max_attempts prevents unbounded growth.
#[test]
fn test_exponential_backoff_respects_max_attempts_cap() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = Address::generate(&env);

    // max_attempts = 2 → effective cooldown is capped at base × 2^1 = 7200.
    const BASE: u64 = 3_600;
    client.set_refund_cooldown_config(
        &admin,
        &RefundCooldownConfig {
            cooldown_seconds: BASE,
            enabled: true,
            max_attempts: 2,
            backoff_multiplier: 2,
        },
    );

    // Denial 1 → 2 → cap reached: 3rd attempt must still be within 2×BASE.
    env.ledger().set_timestamp(1_000);
    let r1 = request_on_payment(&client, &env, &merchant, &customer, &token, 200).unwrap();
    let t1 = deny_and_finalize(&client, &env, &admin, r1);
    env.ledger().set_timestamp(t1 + BASE); // after 1st window
    let r2 = request_on_payment(&client, &env, &merchant, &customer, &token, 201).unwrap();
    let t2 = deny_and_finalize(&client, &env, &admin, r2);
    env.ledger().set_timestamp(t2 + 2 * BASE); // after 2nd window (cap)
    let r3 = request_on_payment(&client, &env, &merchant, &customer, &token, 202).unwrap();
    let t3 = deny_and_finalize(&client, &env, &admin, r3);

    // 3 denials but cap=2 → effective still 2×BASE, not 4×BASE.
    env.ledger().set_timestamp(t3 + 2 * BASE - 1);
    assert_eq!(
        request_on_payment(&client, &env, &merchant, &customer, &token, 202),
        Err(Error::Core(CoreError::RefundCooldownActive))
    );
    env.ledger().set_timestamp(t3 + 2 * BASE);
    request_on_payment(&client, &env, &merchant, &customer, &token, 203).unwrap();
}
