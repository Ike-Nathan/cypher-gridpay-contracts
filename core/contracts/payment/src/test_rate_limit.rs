#![cfg(test)]
use soroban_sdk::{testutils::Address as _, token, Address, Env, String};

use crate::{Currency, Error, PaymentContract, PaymentContractClient, RateLimitConfig};

/// Test that rapid payment invocations are rate-limited per caller address.
/// Verifies that the sliding-window rate limit reverts with RateLimitExceeded
/// when a caller exceeds the configured maximum payments per window.
#[test]
fn test_rate_limit_on_rapid_invocations() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PaymentContract);
    let client = PaymentContractClient::new(&env, &contract_id);

    let admin = Address::generate(env);
    let customer = Address::generate(env);
    let merchant = Address::generate(env);

    // Initialize contract
    client.initialize(&admin);

    // Set a strict rate limit: max 3 payments per 60-second window
    client.set_rate_limit_config(
        &admin,
        &RateLimitConfig {
            max_payments_per_window: 3,
            window_duration: 60,
            max_payment_amount: 0,
            max_daily_volume: 0,
        },
    );

    // Create a token for payments
    let token_admin = Address::generate(env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_asset = token::StellarAssetClient::new(env, &token_id);
    let token_user = token::Client::new(env, &token_id);

    // Fund customer
    token_asset.mint(&customer, &10000);
    token_user.approve(&customer, &client.address, &10000, &999_999);

    // First 3 payments should succeed
    for i in 0..3 {
        let payment_id = client.create_payment(
            &customer,
            &merchant,
            &100,
            &token_id,
            &Currency::USDC,
            &0,
            &String::from_str(env, ""),
        );
        assert!(payment_id > 0, "Payment {} should succeed", i);
    }

    // 4th payment should be rate-limited
    let result = client.try_create_payment(
        &customer,
        &merchant,
        &100,
        &token_id,
        &Currency::USDC,
        &0,
        &String::from_str(env, ""),
    );
    assert!(result.is_err(), "4th payment should be rate-limited");
}

/// Test that rate limit window resets after the window duration elapses.
#[test]
fn test_rate_limit_window_reset() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PaymentContract);
    let client = PaymentContractClient::new(&env, &contract_id);

    let admin = Address::generate(env);
    let customer = Address::generate(env);
    let merchant = Address::generate(env);

    client.initialize(&admin);

    // Set rate limit: max 2 payments per 10-second window
    client.set_rate_limit_config(
        &admin,
        &RateLimitConfig {
            max_payments_per_window: 2,
            window_duration: 10,
            max_payment_amount: 0,
            max_daily_volume: 0,
        },
    );

    let token_admin = Address::generate(env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_asset = token::StellarAssetClient::new(env, &token_id);
    let token_user = token::Client::new(env, &token_id);

    token_asset.mint(&customer, &10000);
    token_user.approve(&customer, &client.address, &10000, &999_999);

    // Use up the rate limit
    client.create_payment(
        &customer,
        &merchant,
        &100,
        &token_id,
        &Currency::USDC,
        &0,
        &String::from_str(env, ""),
    );
    client.create_payment(
        &customer,
        &merchant,
        &100,
        &token_id,
        &Currency::USDC,
        &0,
        &String::from_str(env, ""),
    );

    // Next payment should be rate-limited
    let result = client.try_create_payment(
        &customer,
        &merchant,
        &100,
        &token_id,
        &Currency::USDC,
        &0,
        &String::from_str(env, ""),
    );
    assert!(result.is_err(), "Payment should be rate-limited");

    // Advance time past the window
    env.ledger().with_mut(|li| {
        li.timestamp += 11;
    });

    // Now the rate limit should have reset
    let payment_id = client.create_payment(
        &customer,
        &merchant,
        &100,
        &token_id,
        &Currency::USDC,
        &0,
        &String::from_str(env, ""),
    );
    assert!(payment_id > 0, "Payment should succeed after window reset");
}
