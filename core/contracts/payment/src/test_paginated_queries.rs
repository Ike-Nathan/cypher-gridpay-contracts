#![cfg(test)]

// Issue #87: unbounded per-customer / per-merchant payment lists must be
// returned page by page so a query result can never exceed Soroban's ledger
// entry limit.

use super::*;
use soroban_sdk::{testutils::Address as _, token, Address, Env, String};

/// Number of records used to exceed the `MAX_QUERY_PAGE_SIZE` cap.
const LARGE_LIST_LEN: u64 = 120;

fn setup<'a>(
    env: &Env,
) -> (
    PaymentContractClient<'a>,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    env.mock_all_auths();
    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let token_address = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = token::StellarAssetClient::new(env, &token_address);
    token_client.mint(&contract_id, &1_000_000_000);
    client.initialize(&admin);
    let customer = Address::generate(env);
    let merchant = Address::generate(env);
    (
        client,
        contract_id,
        admin,
        customer,
        merchant,
        token_address,
    )
}

fn create_payment(
    env: &Env,
    client: &PaymentContractClient,
    customer: &Address,
    merchant: &Address,
    token_address: &Address,
    amount: i128,
) -> u64 {
    client.create_payment(
        customer,
        merchant,
        &amount,
        token_address,
        &Currency::USDC,
        &0_u64,
        &String::from_str(env, ""),
    )
}

#[test]
fn test_customer_and_merchant_payment_pages_are_clamped() {
    let env = Env::default();
    let (client, _contract_id, _admin, customer, merchant, token_address) = setup(&env);

    for _ in 0..LARGE_LIST_LEN {
        create_payment(&env, &client, &customer, &merchant, &token_address, 10);
    }

    assert_eq!(
        client.get_payment_count_by_customer(&customer),
        LARGE_LIST_LEN
    );

    // Oversized requests are clamped to a single page instead of returning a
    // result set that would overflow a ledger entry.
    let clamped = client.get_payments_by_customer(&customer, &10_000, &0);
    assert_eq!(clamped.len(), 100);

    // Pages are contiguous and cover every payment exactly once.
    let mut seen = 0u64;
    let mut offset = 0u64;
    loop {
        let page = client.get_payments_by_customer(&customer, &40, &offset);
        if page.is_empty() {
            break;
        }
        seen += page.len() as u64;
        offset += page.len() as u64;
    }
    assert_eq!(seen, LARGE_LIST_LEN);

    // The same guarantee applies to the merchant view.
    assert_eq!(
        client
            .get_payments_by_merchant(&merchant, &10_000, &0)
            .len(),
        100
    );
    assert_eq!(
        client.get_payments_by_merchant(&merchant, &40, &100).len(),
        20
    );
}

#[test]
fn test_pending_settlements_are_paginated_over_large_lists() {
    let env = Env::default();
    let (client, _contract_id, admin, customer, merchant, token_address) = setup(&env);

    // Queue a finality delay so every completed payment lands in the
    // merchant's pending settlement index.
    client.configure_finality_delay(
        &admin,
        &FinalityConfig {
            delay_seconds: 3600,
            min_amount_threshold: 0,
            active: true,
        },
    );

    let token_client = token::StellarAssetClient::new(&env, &token_address);
    for _ in 0..LARGE_LIST_LEN {
        let payment_id = create_payment(&env, &client, &customer, &merchant, &token_address, 100);
        token_client.mint(&customer, &1_000_000);
        client.complete_payment(&admin, &payment_id);
    }

    assert_eq!(
        client.get_pending_settlement_count(&merchant),
        LARGE_LIST_LEN
    );

    let first_page = client.get_pending_settlements(&merchant, &50, &0);
    let second_page = client.get_pending_settlements(&merchant, &50, &50);
    let tail = client.get_pending_settlements(&merchant, &50, &100);
    assert_eq!(first_page.len(), 50);
    assert_eq!(second_page.len(), 50);
    assert_eq!(tail.len(), 20);

    // Oversized requests are clamped instead of returning an oversized result.
    assert_eq!(
        client.get_pending_settlements(&merchant, &1_000, &0).len(),
        100
    );
    assert!(client
        .get_pending_settlements(&merchant, &10, &1_000)
        .is_empty());
    assert!(client.get_pending_settlements(&merchant, &0, &0).is_empty());
}

#[test]
fn test_installment_history_is_paginated_newest_first() {
    let env = Env::default();
    let (client, _contract_id, _admin, customer, merchant, token_address) = setup(&env);

    let payment_id = create_payment(&env, &client, &customer, &merchant, &token_address, 1_000);
    let token_client = token::StellarAssetClient::new(&env, &token_address);
    for _ in 0..LARGE_LIST_LEN {
        token_client.mint(&customer, &1_000_000);
        client.pay_installment(&customer, &payment_id, &1);
    }

    assert_eq!(
        client.get_installment_count(&payment_id),
        LARGE_LIST_LEN as u32
    );

    let first_page = client.get_installment_history(&payment_id, &60, &0);
    let second_page = client.get_installment_history(&payment_id, &60, &60);
    assert_eq!(first_page.len(), 60);
    assert_eq!(second_page.len(), 60);

    // Newest-first and contiguous.
    assert_eq!(first_page.get(0).unwrap().installment_number, 120);
    assert_eq!(second_page.get(59).unwrap().installment_number, 1);

    // Oversized requests are clamped to a single page.
    assert_eq!(
        client
            .get_installment_history(&payment_id, &10_000, &0)
            .len(),
        100
    );
    assert!(client
        .get_installment_history(&payment_id, &10, &1_000)
        .is_empty());
}
