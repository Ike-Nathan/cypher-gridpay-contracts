#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address, Env, String};

/// A payment may only have one active refund at a time, so pay each refund
/// out before requesting the next one against the same payment.
fn settle(client: &RefundContractClient, admin: &Address, refund_id: u64) {
    client.approve_refund(admin, &refund_id);
    client.process_refund(admin, &refund_id);
}

fn funded_token(env: &Env, holder: &Address) -> Address {
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(env))
        .address();
    soroban_sdk::token::StellarAssetClient::new(env, &token)
        .mock_all_auths()
        .mint(holder, &1_000_000);
    token
}

#[test]
fn test_set_payment_refund_cap() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let payment_id = 1u64;

    env.mock_all_auths();
    client.initialize(&admin);

    let cap = PaymentRefundCap {
        payment_id,
        max_refund_count: 5,
        max_total_amount: 5000i128,
    };

    let res = client.try_set_payment_refund_cap(&admin, &cap);
    assert!(res.is_ok());

    let retrieved_cap = client.get_payment_refund_cap(&payment_id);
    assert!(retrieved_cap.is_some());
    let retrieved = retrieved_cap.unwrap();
    assert_eq!(retrieved.payment_id, payment_id);
    assert_eq!(retrieved.max_refund_count, 5);
    assert_eq!(retrieved.max_total_amount, 5000i128);
}

#[test]
fn test_get_payment_refund_usage_default() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);

    let payment_id = 1u64;
    let (count, amount) = client.get_payment_refund_usage(&payment_id);

    assert_eq!(count, 0);
    assert_eq!(amount, 0);
}

#[test]
fn test_refund_count_cap_exceeded() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = funded_token(&env, &contract_id);
    let payment_id = 1u64;
    let reason = String::from_str(&env, "Customer requested");

    env.mock_all_auths();
    client.initialize(&admin);

    // Set cap: max 2 refunds for this payment
    let cap = PaymentRefundCap {
        payment_id,
        max_refund_count: 2,
        max_total_amount: 10000i128,
    };
    client.set_payment_refund_cap(&admin, &cap);

    // First refund should succeed
    let res1 = client.try_request_refund(
        &merchant,
        &payment_id,
        &customer,
        &100,
        &1000,
        &token,
        &reason,
        &RefundReasonCode::CustomerRequest,
        &0,
    );
    assert!(res1.is_ok());
    settle(&client, &admin, res1.unwrap().unwrap());

    // Second refund should succeed
    let res2 = client.try_request_refund(
        &merchant,
        &payment_id,
        &customer,
        &100,
        &1000,
        &token,
        &reason,
        &RefundReasonCode::CustomerRequest,
        &0,
    );
    assert!(res2.is_ok());
    settle(&client, &admin, res2.unwrap().unwrap());

    // Third refund should fail with RefundCountCapExceeded
    let res3 = client.try_request_refund(
        &merchant,
        &payment_id,
        &customer,
        &100,
        &1000,
        &token,
        &reason,
        &RefundReasonCode::CustomerRequest,
        &0,
    );
    assert!(res3.is_err());
    assert_eq!(
        res3.unwrap_err().unwrap(),
        Error::Ext(ExtError::RefundCountCapExceeded)
    );
}

#[test]
fn test_refund_amount_cap_exceeded() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = funded_token(&env, &contract_id);
    let payment_id = 1u64;
    let reason = String::from_str(&env, "Customer requested");

    env.mock_all_auths();
    client.initialize(&admin);

    // Set cap: max 3 refunds, max 500 total amount for this payment
    let cap = PaymentRefundCap {
        payment_id,
        max_refund_count: 3,
        max_total_amount: 500i128,
    };
    client.set_payment_refund_cap(&admin, &cap);

    // First refund: 200 (total: 200, within 500 limit)
    let res1 = client.try_request_refund(
        &merchant,
        &payment_id,
        &customer,
        &200,
        &1000,
        &token,
        &reason,
        &RefundReasonCode::CustomerRequest,
        &0,
    );
    assert!(res1.is_ok());
    settle(&client, &admin, res1.unwrap().unwrap());

    // Second refund: 200 (total: 400, within 500 limit)
    let res2 = client.try_request_refund(
        &merchant,
        &payment_id,
        &customer,
        &200,
        &1000,
        &token,
        &reason,
        &RefundReasonCode::CustomerRequest,
        &0,
    );
    assert!(res2.is_ok());
    settle(&client, &admin, res2.unwrap().unwrap());

    // Third refund: 150 (total would be 550, exceeds 500 limit)
    let res3 = client.try_request_refund(
        &merchant,
        &payment_id,
        &customer,
        &150,
        &1000,
        &token,
        &reason,
        &RefundReasonCode::CustomerRequest,
        &0,
    );
    assert!(res3.is_err());
    assert_eq!(
        res3.unwrap_err().unwrap(),
        Error::Ext(ExtError::RefundAmountCapExceeded)
    );
}

#[test]
fn test_cumulative_amount_enforcement() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = funded_token(&env, &contract_id);
    let payment_id = 1u64;
    let reason = String::from_str(&env, "Customer requested");

    env.mock_all_auths();
    client.initialize(&admin);

    // Set cap: max 1000 total amount
    let cap = PaymentRefundCap {
        payment_id,
        max_refund_count: 10,
        max_total_amount: 1000i128,
    };
    client.set_payment_refund_cap(&admin, &cap);

    // Add refund 1: 400
    let res1 = client.try_request_refund(
        &merchant,
        &payment_id,
        &customer,
        &400,
        &10_000, // large payment, so the refund cap is what binds
        &token,
        &reason,
        &RefundReasonCode::CustomerRequest,
        &0,
    );
    assert!(res1.is_ok());
    settle(&client, &admin, res1.unwrap().unwrap());

    let (count1, amount1) = client.get_payment_refund_usage(&payment_id);
    assert_eq!(count1, 1);
    assert_eq!(amount1, 400i128);

    // Add refund 2: 350 (cumulative: 750)
    let res2 = client.try_request_refund(
        &merchant,
        &payment_id,
        &customer,
        &350,
        &10_000, // large payment, so the refund cap is what binds
        &token,
        &reason,
        &RefundReasonCode::CustomerRequest,
        &0,
    );
    assert!(res2.is_ok());
    settle(&client, &admin, res2.unwrap().unwrap());

    let (count2, amount2) = client.get_payment_refund_usage(&payment_id);
    assert_eq!(count2, 2);
    assert_eq!(amount2, 750i128);

    // Add refund 3: 300 (cumulative would be 1050, exceeds 1000)
    let res3 = client.try_request_refund(
        &merchant,
        &payment_id,
        &customer,
        &300,
        &10_000, // large payment, so the refund cap is what binds
        &token,
        &reason,
        &RefundReasonCode::CustomerRequest,
        &0,
    );
    assert!(res3.is_err());
    assert_eq!(
        res3.unwrap_err().unwrap(),
        Error::Ext(ExtError::RefundAmountCapExceeded)
    );

    // But 250 should succeed (cumulative: 1000)
    let res4 = client.try_request_refund(
        &merchant,
        &payment_id,
        &customer,
        &250,
        &10_000, // large payment, so the refund cap is what binds
        &token,
        &reason,
        &RefundReasonCode::CustomerRequest,
        &0,
    );
    assert!(res4.is_ok());
    settle(&client, &admin, res4.unwrap().unwrap());

    let (count4, amount4) = client.get_payment_refund_usage(&payment_id);
    assert_eq!(count4, 3);
    assert_eq!(amount4, 1000i128);
}

#[test]
fn test_no_cap_allows_unlimited() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = funded_token(&env, &contract_id);
    let payment_id = 1u64;
    let reason = String::from_str(&env, "Customer requested");

    env.mock_all_auths();
    client.initialize(&admin);

    // No cap is set for this payment, so unlimited refunds should be allowed

    // Request multiple refunds without a cap
    for i in 0..5 {
        let res = client.try_request_refund(
            &merchant,
            &payment_id,
            &customer,
            &100,
            &1000,
            &token,
            &reason,
            &RefundReasonCode::CustomerRequest,
            &0,
        );
        assert!(res.is_ok(), "Refund {} should succeed without cap", i + 1);
        settle(&client, &admin, res.unwrap().unwrap());
    }

    let (count, amount) = client.get_payment_refund_usage(&payment_id);
    assert_eq!(count, 5);
    assert_eq!(amount, 500i128);
}

#[test]
fn test_multiple_payments_independent_caps() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = funded_token(&env, &contract_id);
    let reason = String::from_str(&env, "Customer requested");

    env.mock_all_auths();
    client.initialize(&admin);

    // Set cap for payment 1: max 2 refunds, 500 total
    let cap1 = PaymentRefundCap {
        payment_id: 1,
        max_refund_count: 2,
        max_total_amount: 500i128,
    };
    client.set_payment_refund_cap(&admin, &cap1);

    // Set cap for payment 2: max 3 refunds, 1000 total
    let cap2 = PaymentRefundCap {
        payment_id: 2,
        max_refund_count: 3,
        max_total_amount: 1000i128,
    };
    client.set_payment_refund_cap(&admin, &cap2);

    // Request refunds for payment 1
    for i in 0..2 {
        let res = client.try_request_refund(
            &merchant,
            &1,
            &customer,
            &250,
            &1000,
            &token,
            &reason,
            &RefundReasonCode::CustomerRequest,
            &0,
        );
        assert!(res.is_ok(), "Payment 1 refund {} should succeed", i + 1);
        settle(&client, &admin, res.unwrap().unwrap());
    }

    // Third refund for payment 1 should fail
    let res_fail = client.try_request_refund(
        &merchant,
        &1,
        &customer,
        &100,
        &1000,
        &token,
        &reason,
        &RefundReasonCode::CustomerRequest,
        &0,
    );
    assert!(res_fail.is_err());

    // Request refunds for payment 2 (should succeed since it has higher cap)
    for i in 0..3 {
        let res = client.try_request_refund(
            &merchant,
            &2,
            &customer,
            &300,
            &1000,
            &token,
            &reason,
            &RefundReasonCode::CustomerRequest,
            &0,
        );
        assert!(res.is_ok(), "Payment 2 refund {} should succeed", i + 1);
        settle(&client, &admin, res.unwrap().unwrap());
    }

    let (count1, amount1) = client.get_payment_refund_usage(&1);
    assert_eq!(count1, 2);
    assert_eq!(amount1, 500i128);

    let (count2, amount2) = client.get_payment_refund_usage(&2);
    assert_eq!(count2, 3);
    assert_eq!(amount2, 900i128);
}

#[test]
fn test_cap_update() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let payment_id = 1u64;

    env.mock_all_auths();
    client.initialize(&admin);

    // Set initial cap
    let cap1 = PaymentRefundCap {
        payment_id,
        max_refund_count: 3,
        max_total_amount: 300i128,
    };
    client.set_payment_refund_cap(&admin, &cap1);

    let retrieved1 = client.get_payment_refund_cap(&payment_id).unwrap();
    assert_eq!(retrieved1.max_refund_count, 3);
    assert_eq!(retrieved1.max_total_amount, 300i128);

    // Update cap with higher limits
    let cap2 = PaymentRefundCap {
        payment_id,
        max_refund_count: 5,
        max_total_amount: 1000i128,
    };
    client.set_payment_refund_cap(&admin, &cap2);

    let retrieved2 = client.get_payment_refund_cap(&payment_id).unwrap();
    assert_eq!(retrieved2.max_refund_count, 5);
    assert_eq!(retrieved2.max_total_amount, 1000i128);
}

#[test]
fn test_unauthorized_set_cap() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    let payment_id = 1u64;

    env.mock_all_auths();
    client.initialize(&admin);

    let cap = PaymentRefundCap {
        payment_id,
        max_refund_count: 3,
        max_total_amount: 300i128,
    };

    // Attempt to set cap as non-admin
    let res = client.try_set_payment_refund_cap(&unauthorized, &cap);
    assert!(res.is_err());
    assert_eq!(
        res.unwrap_err().unwrap(),
        Error::Core(CoreError::Unauthorized)
    );
}

#[test]
fn test_invalid_payment_id_cap() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);

    // Try to set cap with payment_id = 0
    let cap = PaymentRefundCap {
        payment_id: 0,
        max_refund_count: 3,
        max_total_amount: 300i128,
    };

    let res = client.try_set_payment_refund_cap(&admin, &cap);
    assert!(res.is_err());
    assert_eq!(
        res.unwrap_err().unwrap(),
        Error::Core(CoreError::InvalidPaymentId)
    );
}

#[test]
fn test_exact_boundary_amount() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = funded_token(&env, &contract_id);
    let payment_id = 1u64;
    let reason = String::from_str(&env, "Customer requested");

    env.mock_all_auths();
    client.initialize(&admin);

    // Set cap: exactly 500 total amount
    let cap = PaymentRefundCap {
        payment_id,
        max_refund_count: 10,
        max_total_amount: 500i128,
    };
    client.set_payment_refund_cap(&admin, &cap);

    // Refund exactly 500 (should succeed)
    let res1 = client.try_request_refund(
        &merchant,
        &payment_id,
        &customer,
        &500,
        &1000,
        &token,
        &reason,
        &RefundReasonCode::CustomerRequest,
        &0,
    );
    assert!(res1.is_ok());
    settle(&client, &admin, res1.unwrap().unwrap());

    // Any additional amount should fail
    let res2 = client.try_request_refund(
        &merchant,
        &payment_id,
        &customer,
        &1,
        &1000,
        &token,
        &reason,
        &RefundReasonCode::CustomerRequest,
        &0,
    );
    assert!(res2.is_err());
    assert_eq!(
        res2.unwrap_err().unwrap(),
        Error::Ext(ExtError::RefundAmountCapExceeded)
    );
}

#[test]
fn test_exact_boundary_count() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = funded_token(&env, &contract_id);
    let payment_id = 1u64;
    let reason = String::from_str(&env, "Customer requested");

    env.mock_all_auths();
    client.initialize(&admin);

    // Set cap: exactly 2 refunds
    let cap = PaymentRefundCap {
        payment_id,
        max_refund_count: 2,
        max_total_amount: 10000i128,
    };
    client.set_payment_refund_cap(&admin, &cap);

    // First refund succeeds
    let res1 = client.try_request_refund(
        &merchant,
        &payment_id,
        &customer,
        &100,
        &1000,
        &token,
        &reason,
        &RefundReasonCode::CustomerRequest,
        &0,
    );
    assert!(res1.is_ok());
    settle(&client, &admin, res1.unwrap().unwrap());

    // Second refund succeeds
    let res2 = client.try_request_refund(
        &merchant,
        &payment_id,
        &customer,
        &100,
        &1000,
        &token,
        &reason,
        &RefundReasonCode::CustomerRequest,
        &0,
    );
    assert!(res2.is_ok());
    settle(&client, &admin, res2.unwrap().unwrap());

    // Third refund fails
    let res3 = client.try_request_refund(
        &merchant,
        &payment_id,
        &customer,
        &100,
        &1000,
        &token,
        &reason,
        &RefundReasonCode::CustomerRequest,
        &0,
    );
    assert!(res3.is_err());
    assert_eq!(
        res3.unwrap_err().unwrap(),
        Error::Ext(ExtError::RefundCountCapExceeded)
    );
}

fn request_amount(
    client: &RefundContractClient,
    env: &Env,
    merchant: &Address,
    customer: &Address,
    token: &Address,
    payment_id: u64,
    amount: i128,
    payment_amount: i128,
) -> Result<u64, Error> {
    match client.try_request_refund(
        merchant,
        &payment_id,
        customer,
        &amount,
        &payment_amount,
        token,
        &String::from_str(env, "partial"),
        &RefundReasonCode::CustomerRequest,
        &0,
    ) {
        Ok(Ok(id)) => Ok(id),
        Err(Ok(e)) => Err(e),
        other => panic!("unexpected result: {:?}", other),
    }
}

fn setup_accumulated<'a>(
    env: &'a Env,
) -> (RefundContractClient<'a>, Address, Address, Address, Address) {
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin);
    let token = funded_token(env, &contract_id);
    (
        client,
        admin,
        Address::generate(env),
        Address::generate(env),
        token,
    )
}

#[test]
fn test_accumulated_refunds_exact_boundary() {
    let env = Env::default();
    let (client, admin, merchant, customer, token) = setup_accumulated(&env);

    // 400 + 599 + 1 == 1000: every step up to the exact payment amount is allowed.
    for amount in [400, 599, 1] {
        let id = request_amount(
            &client, &env, &merchant, &customer, &token, 1, amount, 1_000,
        )
        .unwrap();
        settle(&client, &admin, id);
    }
    assert_eq!(client.get_accumulated_refunds(&1), 1_000);

    // One more unit would exceed the payment.
    assert_eq!(
        request_amount(&client, &env, &merchant, &customer, &token, 1, 1, 1_000),
        Err(Error::Ext(ExtError::RefundCapExceeded))
    );
}

#[test]
fn test_accumulated_refunds_one_over_boundary() {
    let env = Env::default();
    let (client, admin, merchant, customer, token) = setup_accumulated(&env);

    let id = request_amount(&client, &env, &merchant, &customer, &token, 2, 700, 1_000).unwrap();
    settle(&client, &admin, id);

    // 700 + 301 = 1001 > 1000.
    assert_eq!(
        request_amount(&client, &env, &merchant, &customer, &token, 2, 301, 1_000),
        Err(Error::Ext(ExtError::RefundCapExceeded))
    );
    assert_eq!(client.get_accumulated_refunds(&2), 700);
    // 700 + 300 = 1000 is exactly allowed.
    request_amount(&client, &env, &merchant, &customer, &token, 2, 300, 1_000).unwrap();
    assert_eq!(client.get_accumulated_refunds(&2), 1_000);
}

#[test]
fn test_accumulated_refunds_counts_pending_and_releases_on_denial() {
    let env = Env::default();
    let (client, admin, merchant, customer, token) = setup_accumulated(&env);
    client.set_payment_rejection_cooldown(&admin, &0);

    // A pending (not yet paid) refund already counts toward the total.
    let pending =
        request_amount(&client, &env, &merchant, &customer, &token, 3, 800, 1_000).unwrap();
    assert_eq!(client.get_accumulated_refunds(&3), 800);

    // Finalized denial releases it again.
    client.reject_refund(&admin, &pending, &String::from_str(&env, "no"));
    let deadline = client.get_refund(&pending).appeal_deadline.unwrap();
    env.ledger().set_timestamp(deadline);
    client.finalize_denial(&pending);
    assert_eq!(client.get_accumulated_refunds(&3), 0);

    let paid = request_amount(&client, &env, &merchant, &customer, &token, 3, 500, 1_000).unwrap();
    settle(&client, &admin, paid);

    // Reinstating the old 800 denial would make 500 + 800 > 1000.
    assert_eq!(
        client.try_merchant_override_denial(&merchant, &pending),
        Err(Ok(Error::Ext(ExtError::RefundCapExceeded)))
    );
    assert_eq!(client.get_refund(&pending).status, RefundStatus::Rejected);
    assert_eq!(client.get_accumulated_refunds(&3), 500);
}
