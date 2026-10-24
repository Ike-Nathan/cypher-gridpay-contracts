#![cfg(test)]

// Issue #87: unbounded per-customer history and appeal lists must be stored
// under keyed indices and returned page by page so a query result can never
// exceed Soroban's ledger entry limit.

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

/// Number of records used to exceed the `MAX_QUERY_PAGE_SIZE` cap.
const LARGE_LIST_LEN: u64 = 120;

fn setup<'a>(env: &Env) -> (RefundContractClient<'a>, Address, Address, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    let merchant = Address::generate(env);
    let customer = Address::generate(env);
    let token = Address::generate(env);
    client.set_fraud_config(
        &admin,
        &FraudConfig {
            max_refund_rate_bps: 10000,
            min_transactions_for_check: 1000,
            enabled: false,
        },
    );
    (client, admin, merchant, customer, token)
}

fn request_refund(
    env: &Env,
    client: &RefundContractClient,
    merchant: &Address,
    customer: &Address,
    token: &Address,
    payment_id: u64,
) -> u64 {
    client.request_refund(
        merchant,
        &payment_id,
        customer,
        &1000,
        &1000,
        token,
        &String::from_str(env, "damaged on arrival"),
        &RefundReasonCode::ProductDefect,
        &0_u64,
    )
}

/// Creates `count` requested refunds for the same customer and merchant.
fn seed_refunds(
    env: &Env,
    client: &RefundContractClient,
    merchant: &Address,
    customer: &Address,
    token: &Address,
    count: u64,
) -> Vec<u64> {
    let mut ids = Vec::new(env);
    for i in 1..=count {
        ids.push_back(request_refund(env, client, merchant, customer, token, i));
    }
    ids
}

#[test]
fn test_appeals_by_customer_are_paginated_over_large_lists() {
    let env = Env::default();
    let (client, admin, merchant, customer, token) = setup(&env);
    let refund_ids = seed_refunds(&env, &client, &merchant, &customer, &token, LARGE_LIST_LEN);

    for refund_id in refund_ids.iter() {
        client.reject_refund(&admin, &refund_id, &String::from_str(&env, "denied"));
        client.file_appeal(
            &customer,
            &refund_id,
            &String::from_str(&env, "please review"),
        );
    }

    // The index stores one appeal id per slot, so the total is unbounded.
    assert_eq!(
        client.get_appeal_count_by_customer(&customer),
        LARGE_LIST_LEN
    );

    // Each appeal lives in its own ledger entry; queries return pages only.
    let first_page = client.get_appeals_by_customer(&customer, &50, &0);
    let second_page = client.get_appeals_by_customer(&customer, &50, &50);
    let tail = client.get_appeals_by_customer(&customer, &50, &100);
    assert_eq!(first_page.len(), 50);
    assert_eq!(second_page.len(), 50);
    assert_eq!(tail.len(), 20);

    // Pages do not overlap and cover every appeal.
    assert_eq!(first_page.get(0).unwrap().appeal_id, 1);
    assert_eq!(first_page.get(49).unwrap().appeal_id, 50);
    assert_eq!(second_page.get(0).unwrap().appeal_id, 51);
    assert_eq!(tail.get(19).unwrap().appeal_id, 120);

    // A request for more than a page is clamped instead of returning an
    // oversized result.
    let clamped = client.get_appeals_by_customer(&customer, &1000, &0);
    assert_eq!(clamped.len(), 100);

    // Out-of-range and zero-limit reads return nothing.
    assert!(client
        .get_appeals_by_customer(&customer, &10, &1000)
        .is_empty());
    assert!(client.get_appeals_by_customer(&customer, &0, &0).is_empty());
}

#[test]
fn test_customer_and_merchant_history_pages_over_large_lists() {
    let env = Env::default();
    let (client, _admin, merchant, customer, token) = setup(&env);
    seed_refunds(&env, &client, &merchant, &customer, &token, LARGE_LIST_LEN);

    // ── Customer history (newest-first) ──
    assert_eq!(
        client.get_customer_refund_count_public(&customer),
        LARGE_LIST_LEN
    );

    let first_page = client.get_customer_refund_history(&customer, &60, &0);
    let second_page = client.get_customer_refund_history(&customer, &60, &60);
    assert_eq!(first_page.len(), 60);
    assert_eq!(second_page.len(), 60);

    // History is newest-first and the pages are contiguous.
    assert_eq!(first_page.get(0).unwrap().id, LARGE_LIST_LEN);
    assert_eq!(first_page.get(59).unwrap().id, LARGE_LIST_LEN - 59);
    assert_eq!(second_page.get(0).unwrap().id, LARGE_LIST_LEN - 60);
    assert_eq!(second_page.get(59).unwrap().id, 1);

    // Oversized requests are clamped to a single page.
    assert_eq!(
        client
            .get_customer_refund_history(&customer, &1000, &0)
            .len(),
        100
    );

    // ── Merchant pending refunds (oldest-first) ──
    let pending_first = client.get_merchant_pending_refunds(&merchant, &100, &0);
    let pending_second = client.get_merchant_pending_refunds(&merchant, &100, &100);
    assert_eq!(pending_first.len(), 100);
    assert_eq!(pending_second.len(), 20);
    assert_eq!(pending_first.get(0).unwrap().id, 1);
    assert_eq!(
        pending_first.get(0).unwrap().status,
        RefundStatus::Requested
    );
    assert_eq!(pending_second.get(0).unwrap().id, 101);
    assert_eq!(pending_second.get(19).unwrap().id, 120);

    // Oversized requests are clamped to a single page.
    assert_eq!(
        client
            .get_merchant_pending_refunds(&merchant, &5000, &0)
            .len(),
        100
    );
    assert!(client
        .get_merchant_pending_refunds(&merchant, &10, &1000)
        .is_empty());
}

#[test]
fn test_customer_vouchers_are_paginated() {
    let env = Env::default();
    let (client, admin, merchant, customer, token) = setup(&env);
    let refund_ids = seed_refunds(&env, &client, &merchant, &customer, &token, LARGE_LIST_LEN);

    for refund_id in refund_ids.iter() {
        client.approve_refund(&admin, &refund_id);
        client.issue_refund_voucher(&admin, &refund_id, &86_400);
    }

    assert_eq!(client.get_customer_voucher_count(&customer), LARGE_LIST_LEN);

    let first_page = client.get_customer_vouchers(&customer, &40, &0);
    let second_page = client.get_customer_vouchers(&customer, &40, &40);
    let tail = client.get_customer_vouchers(&customer, &40, &80);
    assert_eq!(first_page.len(), 40);
    assert_eq!(second_page.len(), 40);
    assert_eq!(tail.len(), 40);
    assert_eq!(first_page.get(0).unwrap().voucher_id, 1);
    assert_eq!(second_page.get(0).unwrap().voucher_id, 41);
    assert_eq!(tail.get(39).unwrap().voucher_id, 120);

    // Oversized requests are clamped instead of returning an oversized result.
    assert_eq!(
        client.get_customer_vouchers(&customer, &1000, &0).len(),
        100
    );
    assert!(client
        .get_customer_vouchers(&customer, &10, &1000)
        .is_empty());
}
