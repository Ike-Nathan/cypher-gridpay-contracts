#![cfg(test)]

use crate::{
    CustomerRefundSummary, RefundContract, RefundContractClient, RefundReasonCode, RefundStatus,
    ABUSE_QUARANTINE_PERIOD_SECS, CLEAN_PAYMENT_DECAY_BPS,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

fn setup_test_env<'a>() -> (Env, RefundContractClient<'a>, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, RefundContract);
    let client = RefundContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    client.initialize(&admin);

    (env, client, admin, merchant, customer)
}

#[test]
fn test_lifecycle_timestamps_on_request() {
    let (env, client, _admin, merchant, customer) = setup_test_env();
    let token = Address::generate(&env);

    env.ledger().set_timestamp(50); // Set a non-zero timestamp

    let refund_id = client.request_refund(
        &merchant,
        &1,
        &customer,
        &1000,
        &5000,
        &token,
        &String::from_str(&env, "Test refund"),
        &RefundReasonCode::CustomerRequest,
        &env.ledger().timestamp(),
    );

    let refund = client.get_refund(&refund_id);

    // Verify requested_at is set
    assert_eq!(refund.requested_at, 50);

    // Verify other timestamps are None
    assert_eq!(refund.approved_at, None);
    assert_eq!(refund.rejected_at, None);
    assert_eq!(refund.processed_at, None);
}

#[test]
fn test_lifecycle_timestamps_on_approve() {
    let (env, client, admin, merchant, customer) = setup_test_env();
    let token = Address::generate(&env);

    let refund_id = client.request_refund(
        &merchant,
        &1,
        &customer,
        &1000,
        &5000,
        &token,
        &String::from_str(&env, "Test refund"),
        &RefundReasonCode::CustomerRequest,
        &env.ledger().timestamp(),
    );

    // Advance ledger time
    env.ledger().set_timestamp(100);

    client.approve_refund(&admin, &refund_id);

    let refund = client.get_refund(&refund_id);

    // Verify approved_at is set
    assert!(refund.approved_at.is_some());
    assert_eq!(refund.approved_at.unwrap(), 100);

    // Verify other timestamps
    assert!(refund.requested_at < refund.approved_at.unwrap());
    assert_eq!(refund.rejected_at, None);
    assert_eq!(refund.processed_at, None);
}

#[test]
fn test_lifecycle_timestamps_on_reject() {
    let (env, client, admin, merchant, customer) = setup_test_env();
    let token = Address::generate(&env);

    let refund_id = client.request_refund(
        &merchant,
        &1,
        &customer,
        &1000,
        &5000,
        &token,
        &String::from_str(&env, "Test refund"),
        &RefundReasonCode::CustomerRequest,
        &env.ledger().timestamp(),
    );

    // Advance ledger time
    env.ledger().set_timestamp(100);

    client.reject_refund(&admin, &refund_id, &String::from_str(&env, "Invalid"));

    // Rejection enters the appeal window (PendingAppeal); rejected_at records
    // the denial time, which the appeal window is measured from.
    let refund = client.get_refund(&refund_id);
    assert_eq!(refund.status, RefundStatus::PendingAppeal);
    assert_eq!(refund.rejected_at, Some(100));

    let finalize_at = refund.appeal_deadline.unwrap();
    env.ledger().set_timestamp(finalize_at);
    client.finalize_denial(&refund_id);

    let refund = client.get_refund(&refund_id);

    // Finalizing keeps the original denial time, so it can't reopen the
    // appeal window.
    assert_eq!(refund.status, RefundStatus::Rejected);
    assert_eq!(refund.rejected_at, Some(100));

    // Verify other timestamps
    assert!(refund.requested_at < refund.rejected_at.unwrap());
    assert_eq!(refund.approved_at, None);
    assert_eq!(refund.processed_at, None);
}

#[test]
fn test_lifecycle_timestamps_on_process() {
    let (env, client, admin, merchant, customer) = setup_test_env();
    let token = Address::generate(&env);

    let refund_id = client.request_refund(
        &merchant,
        &1,
        &customer,
        &1000,
        &5000,
        &token,
        &String::from_str(&env, "Test refund"),
        &RefundReasonCode::CustomerRequest,
        &env.ledger().timestamp(),
    );

    // Advance ledger time for approval
    env.ledger().set_timestamp(100);
    client.approve_refund(&admin, &refund_id);

    // Advance ledger time for processing
    env.ledger().set_timestamp(200);
    client.process_refund(&admin, &refund_id);

    let refund = client.get_refund(&refund_id);

    // Verify all timestamps are set correctly
    assert!(refund.requested_at == 0);
    assert_eq!(refund.approved_at, Some(100));
    assert_eq!(refund.processed_at, Some(200));
    assert_eq!(refund.rejected_at, None);

    // Verify chronological order
    assert!(refund.requested_at < refund.approved_at.unwrap());
    assert!(refund.approved_at.unwrap() < refund.processed_at.unwrap());
}

#[test]
fn test_get_customer_refund_history_empty() {
    let (_env, client, _admin, _merchant, customer) = setup_test_env();

    let history = client.get_customer_refund_history(&customer, &10, &0);
    assert_eq!(history.len(), 0);
}

#[test]
fn test_get_customer_refund_history_single() {
    let (env, client, _admin, merchant, customer) = setup_test_env();
    let token = Address::generate(&env);

    client.request_refund(
        &merchant,
        &1,
        &customer,
        &1000,
        &5000,
        &token,
        &String::from_str(&env, "Test refund"),
        &RefundReasonCode::CustomerRequest,
        &env.ledger().timestamp(),
    );

    let history = client.get_customer_refund_history(&customer, &10, &0);
    assert_eq!(history.len(), 1);
    assert_eq!(history.get(0).unwrap().amount, 1000);
}

#[test]
fn test_get_customer_refund_history_newest_first() {
    let (env, client, _admin, merchant, customer) = setup_test_env();
    let token = Address::generate(&env);

    // Create 3 refunds at different times
    env.ledger().set_timestamp(100);
    let refund_id1 = client.request_refund(
        &merchant,
        &1,
        &customer,
        &1000,
        &5000,
        &token,
        &String::from_str(&env, "First refund"),
        &RefundReasonCode::CustomerRequest,
        &env.ledger().timestamp(),
    );

    env.ledger().set_timestamp(200);
    let refund_id2 = client.request_refund(
        &merchant,
        &2,
        &customer,
        &2000,
        &5000,
        &token,
        &String::from_str(&env, "Second refund"),
        &RefundReasonCode::CustomerRequest,
        &env.ledger().timestamp(),
    );

    env.ledger().set_timestamp(300);
    let refund_id3 = client.request_refund(
        &merchant,
        &3,
        &customer,
        &3000,
        &5000,
        &token,
        &String::from_str(&env, "Third refund"),
        &RefundReasonCode::CustomerRequest,
        &env.ledger().timestamp(),
    );

    let history = client.get_customer_refund_history(&customer, &10, &0);
    assert_eq!(history.len(), 3);

    // Verify newest first ordering
    assert_eq!(history.get(0).unwrap().id, refund_id3);
    assert_eq!(history.get(1).unwrap().id, refund_id2);
    assert_eq!(history.get(2).unwrap().id, refund_id1);

    // Verify timestamps are in descending order
    assert!(history.get(0).unwrap().requested_at > history.get(1).unwrap().requested_at);
    assert!(history.get(1).unwrap().requested_at > history.get(2).unwrap().requested_at);
}

#[test]
fn test_get_customer_refund_history_pagination() {
    let (env, client, admin, merchant, _customer) = setup_test_env();
    let token = Address::generate(&env);

    // Disable fraud detection for this test
    let fraud_config = crate::FraudConfig {
        enabled: false,
        max_refund_rate_bps: 10000,
        min_transactions_for_check: 100,
    };
    client.set_fraud_config(&admin, &fraud_config);

    // Use a fresh customer
    let customer = Address::generate(&env);

    // Create 5 refunds
    for i in 1..=5 {
        env.ledger().set_timestamp(i * 100);
        client.request_refund(
            &merchant,
            &i,
            &customer,
            &(i as i128 * 1000),
            &10000, // Higher original amount to avoid exceeding
            &token,
            &String::from_str(&env, "Test refund"),
            &RefundReasonCode::CustomerRequest,
            &env.ledger().timestamp(),
        );
    }

    // Get first page (2 items)
    let page1 = client.get_customer_refund_history(&customer, &2, &0);
    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get(0).unwrap().amount, 5000); // Newest
    assert_eq!(page1.get(1).unwrap().amount, 4000);

    // Get second page (2 items)
    let page2 = client.get_customer_refund_history(&customer, &2, &2);
    assert_eq!(page2.len(), 2);
    assert_eq!(page2.get(0).unwrap().amount, 3000);
    assert_eq!(page2.get(1).unwrap().amount, 2000);

    // Get third page (1 item)
    let page3 = client.get_customer_refund_history(&customer, &2, &4);
    assert_eq!(page3.len(), 1);
    assert_eq!(page3.get(0).unwrap().amount, 1000); // Oldest
}

#[test]
fn test_get_customer_refund_history_offset_beyond_total() {
    let (env, client, _admin, merchant, customer) = setup_test_env();
    let token = Address::generate(&env);

    client.request_refund(
        &merchant,
        &1,
        &customer,
        &1000,
        &5000,
        &token,
        &String::from_str(&env, "Test refund"),
        &RefundReasonCode::CustomerRequest,
        &env.ledger().timestamp(),
    );

    let history = client.get_customer_refund_history(&customer, &10, &100);
    assert_eq!(history.len(), 0);
}

#[test]
fn test_get_customer_refund_count() {
    let (env, client, _admin, merchant, customer) = setup_test_env();
    let token = Address::generate(&env);

    assert_eq!(client.get_customer_refund_count_public(&customer), 0);

    client.request_refund(
        &merchant,
        &1,
        &customer,
        &1000,
        &5000,
        &token,
        &String::from_str(&env, "Test refund 1"),
        &RefundReasonCode::CustomerRequest,
        &env.ledger().timestamp(),
    );

    assert_eq!(client.get_customer_refund_count_public(&customer), 1);

    client.request_refund(
        &merchant,
        &2,
        &customer,
        &2000,
        &5000,
        &token,
        &String::from_str(&env, "Test refund 2"),
        &RefundReasonCode::CustomerRequest,
        &env.ledger().timestamp(),
    );

    assert_eq!(client.get_customer_refund_count_public(&customer), 2);
}

#[test]
fn test_get_customer_refund_summary_empty() {
    let (_env, client, _admin, _merchant, customer) = setup_test_env();

    let summary = client.get_customer_refund_summary(&customer);
    assert_eq!(summary.total_requested, 0);
    assert_eq!(summary.total_approved, 0);
    assert_eq!(summary.total_amount_refunded, 0);
    assert_eq!(summary.avg_processing_time, 0);
}

#[test]
fn test_get_customer_refund_summary_with_data() {
    let (env, client, admin, merchant, customer) = setup_test_env();
    let token = Address::generate(&env);

    // Create refund 1 - will be processed
    env.ledger().set_timestamp(100);
    let refund_id1 = client.request_refund(
        &merchant,
        &1,
        &customer,
        &1000,
        &5000,
        &token,
        &String::from_str(&env, "Refund 1"),
        &RefundReasonCode::CustomerRequest,
        &env.ledger().timestamp(),
    );

    env.ledger().set_timestamp(150);
    client.approve_refund(&admin, &refund_id1);

    env.ledger().set_timestamp(200);
    client.process_refund(&admin, &refund_id1);

    // Create refund 2 - will be processed
    env.ledger().set_timestamp(300);
    let refund_id2 = client.request_refund(
        &merchant,
        &2,
        &customer,
        &2000,
        &5000,
        &token,
        &String::from_str(&env, "Refund 2"),
        &RefundReasonCode::CustomerRequest,
        &env.ledger().timestamp(),
    );

    env.ledger().set_timestamp(350);
    client.approve_refund(&admin, &refund_id2);

    env.ledger().set_timestamp(500);
    client.process_refund(&admin, &refund_id2);

    // Create refund 3 - will be rejected
    env.ledger().set_timestamp(600);
    let refund_id3 = client.request_refund(
        &merchant,
        &3,
        &customer,
        &3000,
        &5000,
        &token,
        &String::from_str(&env, "Refund 3"),
        &RefundReasonCode::CustomerRequest,
        &env.ledger().timestamp(),
    );

    env.ledger().set_timestamp(650);
    client.reject_refund(&admin, &refund_id3, &String::from_str(&env, "Invalid"));

    let summary = client.get_customer_refund_summary(&customer);

    assert_eq!(summary.total_requested, 3);
    assert_eq!(summary.total_approved, 2); // Only processed ones count as approved
    assert_eq!(summary.total_amount_refunded, 3000); // 1000 + 2000

    // Average processing time: ((200-100) + (500-300)) / 2 = (100 + 200) / 2 = 150
    assert_eq!(summary.avg_processing_time, 150);
}

#[test]
fn test_get_customer_refund_summary_only_requested() {
    let (env, client, _admin, merchant, customer) = setup_test_env();
    let token = Address::generate(&env);

    // Create refunds that are only requested (not approved/processed)
    client.request_refund(
        &merchant,
        &1,
        &customer,
        &1000,
        &5000,
        &token,
        &String::from_str(&env, "Refund 1"),
        &RefundReasonCode::CustomerRequest,
        &env.ledger().timestamp(),
    );

    client.request_refund(
        &merchant,
        &2,
        &customer,
        &2000,
        &5000,
        &token,
        &String::from_str(&env, "Refund 2"),
        &RefundReasonCode::CustomerRequest,
        &env.ledger().timestamp(),
    );

    let summary = client.get_customer_refund_summary(&customer);

    assert_eq!(summary.total_requested, 2);
    assert_eq!(summary.total_approved, 0);
    assert_eq!(summary.total_amount_refunded, 0);
    assert_eq!(summary.avg_processing_time, 0);
}

#[test]
fn test_customer_refund_history_multiple_customers() {
    let (env, client, _admin, merchant, _customer) = setup_test_env();
    let token = Address::generate(&env);

    let customer1 = Address::generate(&env);
    let customer2 = Address::generate(&env);

    // Create refunds for customer1
    client.request_refund(
        &merchant,
        &1,
        &customer1,
        &1000,
        &5000,
        &token,
        &String::from_str(&env, "Customer 1 refund"),
        &RefundReasonCode::CustomerRequest,
        &env.ledger().timestamp(),
    );

    // Create refunds for customer2
    client.request_refund(
        &merchant,
        &2,
        &customer2,
        &2000,
        &5000,
        &token,
        &String::from_str(&env, "Customer 2 refund"),
        &RefundReasonCode::CustomerRequest,
        &env.ledger().timestamp(),
    );

    // Verify each customer only sees their own refunds
    let history1 = client.get_customer_refund_history(&customer1, &10, &0);
    assert_eq!(history1.len(), 1);
    assert_eq!(history1.get(0).unwrap().amount, 1000);

    let history2 = client.get_customer_refund_history(&customer2, &10, &0);
    assert_eq!(history2.len(), 1);
    assert_eq!(history2.get(0).unwrap().amount, 2000);
}

// ── Issue #230: serial-refunder scoring and quarantine ──────────────────────

const VIP_TIER: u32 = 9;

fn disable_fraud_checks(client: &RefundContractClient, admin: &Address) {
    client.set_fraud_config(
        admin,
        &crate::FraudConfig {
            enabled: false,
            max_refund_rate_bps: 10000,
            min_transactions_for_check: 100,
        },
    );
}

fn vip_merchant(env: &Env, client: &RefundContractClient) -> Address {
    let merchant = Address::generate(env);
    client.set_customer_tier_policy(&merchant, &VIP_TIER, &10000u32);
    client.set_vip_tier_policy(&merchant, &VIP_TIER, &true);
    merchant
}

fn request(
    env: &Env,
    client: &RefundContractClient,
    merchant: &Address,
    customer: &Address,
    payment_id: u64,
    reason_code: RefundReasonCode,
) -> u64 {
    client.request_refund(
        merchant,
        &payment_id,
        customer,
        &500,
        &1000,
        &Address::generate(env),
        &String::from_str(env, "claim"),
        &reason_code,
        &env.ledger().timestamp(),
    )
}

/// Files three denied claims at three different merchants.
fn make_serial_refunder(
    env: &Env,
    client: &RefundContractClient,
    admin: &Address,
    customer: &Address,
) {
    for payment_id in 1..=3u64 {
        let merchant = Address::generate(env);
        let refund_id = request(
            env,
            client,
            &merchant,
            customer,
            payment_id,
            RefundReasonCode::NonDelivery,
        );
        client.reject_refund(admin, &refund_id, &String::from_str(env, "no evidence"));
    }
}

#[test]
fn test_serial_refunder_is_quarantined_from_fast_track() {
    let (env, client, admin, _merchant, customer) = setup_test_env();
    disable_fraud_checks(&client, &admin);
    client.set_abuse_quarantine_threshold(&admin, &5_000);
    client.set_customer_tier(&admin, &customer, &VIP_TIER);
    env.ledger().set_timestamp(1_000);

    make_serial_refunder(&env, &client, &admin, &customer);

    let profile = client.get_customer_refund_profile(&customer);
    assert_eq!(profile.total_refunds_requested, 3);
    assert_eq!(profile.denied_refunds_count, 3);
    assert_eq!(profile.distinct_merchants_refunded, 3);
    // (3 denied * 3) * 10_000 / max(10, 0 payments)
    assert_eq!(profile.abuse_risk_bps, 9_000);
    assert_eq!(
        profile.auto_refund_disqualified_until,
        1_000 + ABUSE_QUARANTINE_PERIOD_SECS
    );

    // The VIP fast-track no longer applies, but the claim is still accepted
    // for manual review.
    let merchant = vip_merchant(&env, &client);
    let refund_id = request(
        &env,
        &client,
        &merchant,
        &customer,
        4,
        RefundReasonCode::ProductDefect,
    );
    assert_eq!(
        client.get_refund(&refund_id).status,
        RefundStatus::Requested
    );
    assert_eq!(
        client
            .get_customer_refund_profile(&customer)
            .distinct_merchants_refunded,
        4
    );

    // Manual review can still approve it.
    client.approve_refund(&admin, &refund_id);
    assert_eq!(client.get_refund(&refund_id).status, RefundStatus::Approved);
}

#[test]
fn test_high_volume_buyer_keeps_fast_track() {
    let (env, client, admin, _merchant, customer) = setup_test_env();
    disable_fraud_checks(&client, &admin);
    client.set_abuse_quarantine_threshold(&admin, &5_000);
    client.set_customer_tier(&admin, &customer, &VIP_TIER);

    for _ in 0..50 {
        client.record_customer_payment(&admin, &customer);
    }
    let merchant = vip_merchant(&env, &client);
    let refund_id = request(
        &env,
        &client,
        &merchant,
        &customer,
        1,
        RefundReasonCode::ProductDefect,
    );

    assert_eq!(client.get_refund(&refund_id).status, RefundStatus::Approved);
    let profile = client.get_customer_refund_profile(&customer);
    assert_eq!(profile.approved_refunds_count, 1);
    // 1 approved * 10_000 / 50 payments
    assert_eq!(profile.abuse_risk_bps, 200);
}

#[test]
fn test_clean_payments_decay_score_and_lift_quarantine() {
    let (env, client, admin, _merchant, customer) = setup_test_env();
    disable_fraud_checks(&client, &admin);
    client.set_abuse_quarantine_threshold(&admin, &5_000);
    client.set_customer_tier(&admin, &customer, &VIP_TIER);
    env.ledger().set_timestamp(1_000);

    make_serial_refunder(&env, &client, &admin, &customer);
    assert_eq!(
        client.get_customer_refund_profile(&customer).abuse_risk_bps,
        9_000
    );

    // Each consecutive refund-free payment lowers the score.
    client.record_customer_payment(&admin, &customer);
    let profile = client.get_customer_refund_profile(&customer);
    assert_eq!(profile.consecutive_clean_payments, 1);
    assert_eq!(profile.abuse_risk_bps, 9_000 - CLEAN_PAYMENT_DECAY_BPS);

    for _ in 0..9 {
        client.record_customer_payment(&admin, &customer);
    }
    // 9 * 10_000 / 10 payments - 10 * 500
    assert_eq!(
        client.get_customer_refund_profile(&customer).abuse_risk_bps,
        4_000
    );

    // Below threshold, but the quarantine period still has to run out.
    let merchant = vip_merchant(&env, &client);
    let refund_id = request(
        &env,
        &client,
        &merchant,
        &customer,
        4,
        RefundReasonCode::Other,
    );
    assert_eq!(
        client.get_refund(&refund_id).status,
        RefundStatus::Requested
    );

    // Filing a refund resets the clean streak.
    assert_eq!(
        client
            .get_customer_refund_profile(&customer)
            .consecutive_clean_payments,
        0
    );

    for _ in 0..10 {
        client.record_customer_payment(&admin, &customer);
    }
    env.ledger()
        .set_timestamp(1_000 + ABUSE_QUARANTINE_PERIOD_SECS + 1);
    let refund_id = request(
        &env,
        &client,
        &merchant,
        &customer,
        5,
        RefundReasonCode::Other,
    );
    assert_eq!(client.get_refund(&refund_id).status, RefundStatus::Approved);
}

#[test]
fn test_record_customer_payment_requires_admin_or_payment_contract() {
    let (env, client, _admin, _merchant, customer) = setup_test_env();
    let stranger = Address::generate(&env);
    assert!(client
        .try_record_customer_payment(&stranger, &customer)
        .is_err());
}
