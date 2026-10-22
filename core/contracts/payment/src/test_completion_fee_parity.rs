#![cfg(test)]
//! Regression tests for issues #560, #561 and #562: every payment-completion
//! entry point must run the same fee deduction and post-completion logic as
//! `do_complete_payment`, and the public fee preview must match what is
//! actually deducted.
//!
//! Also covers issue #195: time-windowed `FeeWaiver` quotas with promotional
//! campaign budget caps, partial budget exhaustion and per-address usage caps.

use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token, Address, Env, IntoVal, Map, String, Symbol, TryFromVal, Val, Vec,
};

use crate::{
    BatchPaymentEntry, Currency, FeeConfig, FinalityConfig, LoyaltyConfig, PaymentContract,
    PaymentContractClient, PaymentStatus, RiskFeeConfig,
};

fn setup() -> (
    Env,
    PaymentContractClient<'static>,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    client.initialize(&admin);
    (env, client, admin, token_addr, contract_id)
}

fn mint_and_approve(
    env: &Env,
    token_addr: &Address,
    spender: &Address,
    who: &Address,
    amount: i128,
) {
    token::StellarAssetClient::new(env, token_addr).mint(who, &amount);
    token::Client::new(env, token_addr).approve(who, spender, &amount, &100_000);
}

fn standard_fee_config(token_addr: &Address, admin: &Address) -> FeeConfig {
    FeeConfig {
        fee_bps: 100, // 1%
        min_fee: 0,
        max_fee: 0,
        treasury: admin.clone(),
        fee_token: token_addr.clone(),
        active: true,
    }
}

// ── #562 ──────────────────────────────────────────────────────────────────────

#[test]
fn execute_large_payment_deducts_platform_fee() {
    let (env, client, admin, token_addr, contract_id) = setup();
    let admin2 = Address::generate(&env);
    client.add_admin(&admin, &admin2);
    client.update_required_signatures(&admin, &2);

    client.set_fee_config(&admin, &standard_fee_config(&token_addr, &admin));
    client.set_large_payment_threshold(&admin, &1_000);

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    mint_and_approve(&env, &token_addr, &contract_id, &customer, 10_000);

    let payment_id = client.create_payment(
        &customer,
        &merchant,
        &10_000,
        &token_addr,
        &Currency::USDC,
        &0,
        &String::from_str(&env, ""),
    );

    // Route through the multisig flow.
    client.propose_large_payment(&merchant, &payment_id);
    client.approve_large_payment(&admin, &payment_id);
    client.execute_large_payment(&payment_id);

    let token_client = token::Client::new(&env, &token_addr);
    // 1% fee on 10_000 = 100; merchant receives the net.
    assert_eq!(token_client.balance(&merchant), 9_900);
    assert_eq!(client.get_accumulated_fees(), 100);
    assert_eq!(
        client.get_payment(&payment_id).status,
        PaymentStatus::Completed
    );
    assert!(client.get_large_payment_proposal(&payment_id).executed);
}

#[test]
fn execute_large_payment_respects_finality_delay() {
    let (env, client, admin, token_addr, contract_id) = setup();
    let admin2 = Address::generate(&env);
    client.add_admin(&admin, &admin2);
    client.update_required_signatures(&admin, &2);

    client.set_large_payment_threshold(&admin, &1_000);
    client.configure_finality_delay(
        &admin,
        &FinalityConfig {
            delay_seconds: 3_600,
            min_amount_threshold: 1_000,
            active: true,
        },
    );

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    mint_and_approve(&env, &token_addr, &contract_id, &customer, 10_000);

    let payment_id = client.create_payment(
        &customer,
        &merchant,
        &10_000,
        &token_addr,
        &Currency::USDC,
        &0,
        &String::from_str(&env, ""),
    );
    client.propose_large_payment(&merchant, &payment_id);
    client.approve_large_payment(&admin, &payment_id);
    client.execute_large_payment(&payment_id);

    // Funds are held, not settled instantly.
    let token_client = token::Client::new(&env, &token_addr);
    assert_eq!(token_client.balance(&merchant), 0);
    let settlements = client.get_pending_settlements(&merchant, &10, &0);
    assert_eq!(settlements.len(), 1);
    assert_eq!(settlements.get(0).unwrap().payment_id, payment_id);
}

// ── #561 ──────────────────────────────────────────────────────────────────────

#[test]
fn batch_optimized_respects_finality_delay_and_accrues_loyalty() {
    let (env, client, admin, token_addr, contract_id) = setup();

    client.set_fee_config(&admin, &standard_fee_config(&token_addr, &admin));
    client.configure_finality_delay(
        &admin,
        &FinalityConfig {
            delay_seconds: 3_600,
            min_amount_threshold: 5_000,
            active: true,
        },
    );
    client.configure_loyalty(
        &admin,
        &LoyaltyConfig {
            points_per_unit: 100,
            redemption_rate: 1,
            expiry_seconds: 1_000_000,
            active: true,
        },
    );

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    mint_and_approve(&env, &token_addr, &contract_id, &customer, 20_000);

    // One large entry (held by finality delay) and one small entry (settles now).
    // Kept below the 10_000 volume tier-upgrade threshold so the fee stays 1%.
    let entries = Vec::from_array(
        &env,
        [
            BatchPaymentEntry {
                customer: customer.clone(),
                merchant: merchant.clone(),
                amount: 6_000,
                token: token_addr.clone(),
                currency: Currency::USDC,
                expiration_duration: 0,
                metadata: String::from_str(&env, ""),
            },
            BatchPaymentEntry {
                customer: customer.clone(),
                merchant: merchant.clone(),
                amount: 1_000,
                token: token_addr.clone(),
                currency: Currency::USDC,
                expiration_duration: 0,
                metadata: String::from_str(&env, ""),
            },
        ],
    );

    let results = client.create_payment_batch_optimized(&admin, &entries);
    assert_eq!(results.len(), 2);
    assert!(results.get(0).unwrap().success);
    assert!(results.get(1).unwrap().success);

    // The large payment is held as a pending settlement (finality delay honored).
    let settlements = client.get_pending_settlements(&merchant, &10, &0);
    assert_eq!(settlements.len(), 1);
    assert_eq!(settlements.get(0).unwrap().amount, 5_940); // 6_000 - 1% fee

    // The small payment settled immediately, net of the 1% fee.
    let token_client = token::Client::new(&env, &token_addr);
    assert_eq!(token_client.balance(&merchant), 990);

    // Loyalty points accrued for the settled payment (1_000 / 100 = 10).
    let balance = client.get_loyalty_balance(&customer).unwrap();
    assert_eq!(balance.points, 10);

    // Fees from both entries were collected (60 + 10).
    assert_eq!(client.get_accumulated_fees(), 70);
}

// ── #560 ──────────────────────────────────────────────────────────────────────

#[test]
fn calculate_fee_matches_deducted_fee_including_risk_surcharge() {
    let (env, client, admin, token_addr, contract_id) = setup();

    client.set_fee_config(&admin, &standard_fee_config(&token_addr, &admin));
    client.set_risk_fee_config(
        &admin,
        &RiskFeeConfig {
            base_fee_bps: 100,
            large_amount_threshold: 1_000,
            large_amount_surcharge_bps: 50,  // 0.5%
            new_customer_surcharge_bps: 100, // 1%
            high_risk_currency_surcharge: 200,
        },
    );

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    mint_and_approve(&env, &token_addr, &contract_id, &customer, 10_000);

    let payment_id = client.create_payment(
        &customer,
        &merchant,
        &10_000,
        &token_addr,
        &Currency::USDC,
        &0,
        &String::from_str(&env, ""),
    );

    // Preview: base 1% + large-amount 0.5% + new-customer 1% = 2.5% of 10_000.
    let quoted = client.calculate_fee(&10_000, &merchant, &customer, &Currency::USDC);
    assert_eq!(quoted, 250);

    client.complete_payment(&admin, &payment_id);

    let token_client = token::Client::new(&env, &token_addr);
    assert_eq!(token_client.balance(&merchant), 10_000 - quoted);
    assert_eq!(client.get_accumulated_fees(), quoted);
}

#[test]
fn calculate_fee_has_no_risk_surcharge_when_unconfigured() {
    let (env, client, admin, token_addr, _contract_id) = setup();
    client.set_fee_config(&admin, &standard_fee_config(&token_addr, &admin));

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    // No RiskFeeConfig set → only the 1% base fee is quoted.
    let quoted = client.calculate_fee(&10_000, &merchant, &customer, &Currency::BTC);
    assert_eq!(quoted, 100);
}

// ── #195: time-windowed fee waiver quotas with campaign budget caps ───────────

/// Grants a full (100%) fee waiver belonging to `campaign_id`, capped at
/// `budget` aggregate waived fees and `max_uses` waived payments per address.
fn grant_campaign(
    env: &Env,
    client: &PaymentContractClient,
    admin: &Address,
    merchant: &Address,
    campaign_id: u64,
    budget: i128,
    max_uses: u32,
    valid_from: u64,
    valid_until: u64,
) {
    client.grant_fee_waiver(
        admin,
        merchant,
        &10_000,
        &campaign_id,
        &budget,
        &max_uses,
        &valid_from,
        &valid_until,
        &String::from_str(env, "summer promo"),
    );
}

fn create_and_complete(
    env: &Env,
    client: &PaymentContractClient,
    admin: &Address,
    customer: &Address,
    merchant: &Address,
    token_addr: &Address,
    amount: i128,
) -> u64 {
    let payment_id = client.create_payment(
        customer,
        merchant,
        &amount,
        token_addr,
        &Currency::USDC,
        &0,
        &String::from_str(env, ""),
    );
    client.complete_payment(admin, &payment_id);
    payment_id
}

/// Decodes every `FeeWaiverApplied` event published by the payment contract as
/// `(payment_id, campaign_id, waived_amount, remaining_budget)`.
fn fee_waiver_applied_events(env: &Env, contract_id: &Address) -> Vec<(u64, u64, i128, i128)> {
    let name = Symbol::new(env, "fee_waiver_applied");
    let all = env.events().all();
    let mut found = Vec::new(env);
    for i in 0..all.len() {
        let (source, topics, data) = all.get(i).unwrap();
        if source != *contract_id {
            continue;
        }
        if Symbol::try_from_val(env, &topics.first().unwrap()).unwrap() != name {
            continue;
        }
        let payload = Map::<Val, Val>::try_from_val(env, &data).unwrap();
        let field = |key: &str| -> Val {
            payload.get(Symbol::new(env, key).into_val(env)).unwrap()
        };
        found.push_back((
            u64::try_from_val(env, &field("payment_id")).unwrap(),
            u64::try_from_val(env, &field("campaign_id")).unwrap(),
            i128::try_from_val(env, &field("waived_amount")).unwrap(),
            i128::try_from_val(env, &field("remaining_budget")).unwrap(),
        ));
    }
    found
}

#[test]
fn fee_waiver_partial_budget_exhaustion_charges_remaining_fee() {
    let (env, client, admin, token_addr, contract_id) = setup();
    client.set_fee_config(&admin, &standard_fee_config(&token_addr, &admin));

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    mint_and_approve(&env, &token_addr, &contract_id, &customer, 30_000);

    // 1% fee = 100 per payment, but the campaign may only waive 150 in total.
    grant_campaign(&env, &client, &admin, &merchant, 7, 150, 10, 0, 10_000_000);

    // Payment 1: fee 100 is below the 150 budget → fully waived, 50 left.
    assert_eq!(
        client.calculate_fee(&10_000, &merchant, &customer, &Currency::USDC),
        0
    );
    let p1 = create_and_complete(&env, &client, &admin, &customer, &merchant, &token_addr, 10_000);
    let events = fee_waiver_applied_events(&env, &contract_id);
    assert!(events.iter().any(|e| e.0 == p1 && e.1 == 7 && e.2 == 100 && e.3 == 50));

    let token_client = token::Client::new(&env, &token_addr);
    assert_eq!(token_client.balance(&merchant), 10_000);
    assert_eq!(client.get_accumulated_fees(), 0);
    let waiver = client.get_fee_waiver(&merchant).unwrap();
    assert_eq!(waiver.campaign_id, 7);
    assert_eq!(waiver.accumulated_waived_fee, 100);
    assert_eq!(waiver.max_total_waived_fee, 150);

    // Payment 2: only 50 of budget remains → partial waiver, the customer pays
    // the 50 difference.
    assert_eq!(
        client.calculate_fee(&10_000, &merchant, &customer, &Currency::USDC),
        50
    );
    let p2 = create_and_complete(&env, &client, &admin, &customer, &merchant, &token_addr, 10_000);
    let events = fee_waiver_applied_events(&env, &contract_id);
    assert!(events.iter().any(|e| e.0 == p2 && e.1 == 7 && e.2 == 50 && e.3 == 0));

    assert_eq!(token_client.balance(&merchant), 10_000 + 9_950);
    assert_eq!(client.get_accumulated_fees(), 50);
    // Budget exactly consumed → the campaign deactivates automatically.
    assert!(client.get_fee_waiver(&merchant).is_none());

    // Payment 3: campaign gone → the full fee is charged again.
    assert_eq!(
        client.calculate_fee(&10_000, &merchant, &customer, &Currency::USDC),
        100
    );
    let p3 = create_and_complete(&env, &client, &admin, &customer, &merchant, &token_addr, 10_000);
    let events = fee_waiver_applied_events(&env, &contract_id);
    assert!(!events.iter().any(|e| e.0 == p3));

    assert_eq!(token_client.balance(&merchant), 10_000 + 9_950 + 9_900);
    assert_eq!(client.get_accumulated_fees(), 150);
}

#[test]
fn fee_waiver_honours_validity_window_boundaries() {
    let (env, client, admin, token_addr, contract_id) = setup();
    client.set_fee_config(&admin, &standard_fee_config(&token_addr, &admin));

    env.ledger().set_timestamp(1_000);

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    mint_and_approve(&env, &token_addr, &contract_id, &customer, 30_000);

    // Active only between t=2_000 (inclusive) and t=5_000 (inclusive).
    grant_campaign(&env, &client, &admin, &merchant, 3, 10_000, 5, 2_000, 5_000);

    // Before `valid_from`: the waiver is not yet active, the fee is charged.
    env.ledger().set_timestamp(1_500);
    assert!(client.get_fee_waiver(&merchant).is_none());
    assert_eq!(
        client.calculate_fee(&10_000, &merchant, &customer, &Currency::USDC),
        100
    );
    create_and_complete(&env, &client, &admin, &customer, &merchant, &token_addr, 10_000);

    let token_client = token::Client::new(&env, &token_addr);
    assert_eq!(token_client.balance(&merchant), 9_900);
    assert_eq!(client.get_accumulated_fees(), 100);

    // Exactly at `valid_until`: still active (inclusive boundary).
    env.ledger().set_timestamp(5_000);
    assert!(client.get_fee_waiver(&merchant).is_some());
    assert_eq!(
        client.calculate_fee(&10_000, &merchant, &customer, &Currency::USDC),
        0
    );
    create_and_complete(&env, &client, &admin, &customer, &merchant, &token_addr, 10_000);

    assert_eq!(token_client.balance(&merchant), 9_900 + 10_000);
    assert_eq!(client.get_accumulated_fees(), 100);

    // One second past `valid_until`: expired, the fee is charged again.
    env.ledger().set_timestamp(5_001);
    assert!(client.get_fee_waiver(&merchant).is_none());
    assert_eq!(
        client.calculate_fee(&10_000, &merchant, &customer, &Currency::USDC),
        100
    );
    create_and_complete(&env, &client, &admin, &customer, &merchant, &token_addr, 10_000);

    assert_eq!(token_client.balance(&merchant), 9_900 + 10_000 + 9_900);
    assert_eq!(client.get_accumulated_fees(), 200);
}

#[test]
fn fee_waiver_caps_uses_per_address() {
    let (env, client, admin, token_addr, contract_id) = setup();
    client.set_fee_config(&admin, &standard_fee_config(&token_addr, &admin));

    let customer = Address::generate(&env);
    let other_customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    mint_and_approve(&env, &token_addr, &contract_id, &customer, 30_000);
    mint_and_approve(&env, &token_addr, &contract_id, &other_customer, 10_000);

    // 1_000 budget, but at most 2 waived payments per customer address.
    grant_campaign(&env, &client, &admin, &merchant, 11, 1_000, 2, 0, 10_000_000);

    let token_client = token::Client::new(&env, &token_addr);

    // Uses 1 and 2 for `customer` are waived.
    create_and_complete(&env, &client, &admin, &customer, &merchant, &token_addr, 10_000);
    create_and_complete(&env, &client, &admin, &customer, &merchant, &token_addr, 10_000);
    assert_eq!(token_client.balance(&merchant), 20_000);
    assert_eq!(client.get_accumulated_fees(), 0);

    // The per-address cap is exhausted → the next payment pays the full fee.
    assert_eq!(
        client.calculate_fee(&10_000, &merchant, &customer, &Currency::USDC),
        100
    );
    create_and_complete(&env, &client, &admin, &customer, &merchant, &token_addr, 10_000);
    assert_eq!(token_client.balance(&merchant), 20_000 + 9_900);
    assert_eq!(client.get_accumulated_fees(), 100);

    // A different address is unaffected by the exhausted counter.
    assert_eq!(
        client.calculate_fee(&10_000, &merchant, &other_customer, &Currency::USDC),
        0
    );
    let p4 = create_and_complete(
        &env,
        &client,
        &admin,
        &other_customer,
        &merchant,
        &token_addr,
        10_000,
    );
    let events = fee_waiver_applied_events(&env, &contract_id);
    assert!(events.iter().any(|e| e.0 == p4 && e.1 == 11 && e.2 == 100 && e.3 == 700));

    assert_eq!(token_client.balance(&merchant), 20_000 + 9_900 + 10_000);
    assert_eq!(client.get_accumulated_fees(), 100);

    // Budget still available → the campaign stays active.
    let waiver = client.get_fee_waiver(&merchant).unwrap();
    assert_eq!(waiver.accumulated_waived_fee, 300);
    assert_eq!(waiver.max_total_waived_fee, 1_000);
}
