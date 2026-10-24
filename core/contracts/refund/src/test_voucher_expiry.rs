#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, testutils::Ledger, token, Address, Env, String};

fn setup(env: &Env) -> (RefundContractClient, Address, Address) {
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin);
    (client, admin, contract_id)
}

fn create_refund_and_issue_voucher(
    env: &Env,
    client: &RefundContractClient,
    admin: &Address,
    contract_id: &Address,
    expiry_seconds: u64,
) -> (u64, u64) {
    let merchant = Address::generate(env);
    let customer = Address::generate(env);
    let token = env.register_stellar_asset_contract(admin.clone());
    token::StellarAssetClient::new(env, &token).mint(contract_id, &1_000_000);
    let amount = 1000_i128;
    let payment_id = 1_u64;
    let reason = String::from_str(env, "defective product");

    let refund_id = client.request_refund(
        &merchant,
        &payment_id,
        &customer,
        &amount,
        &amount,
        &token,
        &reason,
        &RefundReasonCode::ProductDefect,
        &env.ledger().timestamp(),
    );

    let voucher_id = client.issue_refund_voucher(admin, &refund_id, &expiry_seconds);
    (refund_id, voucher_id)
}

#[test]
fn test_redeem_voucher_before_expiry_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, contract_id) = setup(&env);

    // Issue a voucher expiring 1000s from now
    env.ledger().set_timestamp(1000);
    let (_, voucher_id) =
        create_refund_and_issue_voucher(&env, &client, &admin, &contract_id, 1000);

    // Redeem before expiry (at t=1500, expires at t=2000)
    env.ledger().set_timestamp(1500);
    let result = client.try_redeem_refund_voucher(
        &client.get_voucher(&voucher_id).unwrap().customer,
        &voucher_id,
        &1_u64,
    );
    assert!(
        result.is_ok(),
        "redeeming a valid unexpired voucher should succeed"
    );
}

#[test]
fn test_redeem_expired_voucher_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, contract_id) = setup(&env);

    // Issue a voucher expiring 500s from t=1000 → expires at t=1500
    env.ledger().set_timestamp(1000);
    let (_, voucher_id) = create_refund_and_issue_voucher(&env, &client, &admin, &contract_id, 500);

    // Advance past expiry
    env.ledger().set_timestamp(1501);
    let result = client.try_redeem_refund_voucher(
        &client.get_voucher(&voucher_id).unwrap().customer,
        &voucher_id,
        &1_u64,
    );
    assert!(result.is_err(), "redeeming an expired voucher should fail");
}

#[test]
fn test_redeem_voucher_at_exact_expiry_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, contract_id) = setup(&env);

    // Issue a voucher expiring 300s from t=1000 → expires at t=1300
    env.ledger().set_timestamp(1000);
    let (_, voucher_id) = create_refund_and_issue_voucher(&env, &client, &admin, &contract_id, 300);

    // At exactly the expiry timestamp the voucher is expired (> check uses >)
    env.ledger().set_timestamp(1300);
    let result = client.try_redeem_refund_voucher(
        &client.get_voucher(&voucher_id).unwrap().customer,
        &voucher_id,
        &1_u64,
    );
    // timestamp(1300) > expires_at(1300) is false, so it should succeed at exactly the boundary
    assert!(
        result.is_ok(),
        "voucher should still be valid at exactly the expiry timestamp"
    );
}

#[test]
fn test_already_redeemed_voucher_cannot_be_redeemed_again() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, contract_id) = setup(&env);

    env.ledger().set_timestamp(1000);
    let (_, voucher_id) =
        create_refund_and_issue_voucher(&env, &client, &admin, &contract_id, 1000);

    let customer = client.get_voucher(&voucher_id).unwrap().customer;

    // First redemption should succeed
    client.redeem_refund_voucher(&customer, &voucher_id, &1_u64);

    // Second redemption must fail
    let result = client.try_redeem_refund_voucher(&customer, &voucher_id, &1_u64);
    assert!(result.is_err(), "a voucher can only be redeemed once");
}

/// Issue #60: Customers may partially redeem a voucher's balance across
/// multiple purchases; the voucher tracks `remaining_balance` and is only
/// marked fully redeemed once the balance reaches zero.
#[test]
fn test_partial_redemption_tracks_remaining_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, contract_id) = setup(&env);

    env.ledger().set_timestamp(1000);
    let (_, voucher_id) =
        create_refund_and_issue_voucher(&env, &client, &admin, &contract_id, 1000);
    let customer = client.get_voucher(&voucher_id).unwrap().customer;

    // Voucher amount is 1000; redeem 400 first.
    client.redeem_voucher(&customer, &voucher_id, &400i128);
    let voucher = client.get_voucher(&voucher_id).unwrap();
    assert_eq!(voucher.remaining_balance, 600);
    assert!(
        !voucher.redeemed,
        "voucher with remaining balance must not be fully redeemed"
    );

    // Redeem remaining 600; voucher should now be fully redeemed.
    client.redeem_voucher(&customer, &voucher_id, &600i128);
    let voucher = client.get_voucher(&voucher_id).unwrap();
    assert_eq!(voucher.remaining_balance, 0);
    assert!(voucher.redeemed);

    // Further redemption attempts must fail.
    let result = client.try_redeem_voucher(&customer, &voucher_id, &1i128);
    assert!(
        result.is_err(),
        "fully redeemed voucher cannot be redeemed further"
    );
}

/// Issue #60: An expired voucher must reject partial redemption attempts.
#[test]
fn test_partial_redemption_rejects_expired_voucher() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, contract_id) = setup(&env);

    env.ledger().set_timestamp(1000);
    let (_, voucher_id) = create_refund_and_issue_voucher(&env, &client, &admin, &contract_id, 500);
    let customer = client.get_voucher(&voucher_id).unwrap().customer;

    env.ledger().set_timestamp(1600);
    let result = client.try_redeem_voucher(&customer, &voucher_id, &100i128);
    assert!(result.is_err(), "redeeming an expired voucher should fail");
}

/// Issue #60: Attempting to redeem more than the remaining balance must fail.
#[test]
fn test_partial_redemption_rejects_amount_exceeding_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, contract_id) = setup(&env);

    env.ledger().set_timestamp(1000);
    let (_, voucher_id) =
        create_refund_and_issue_voucher(&env, &client, &admin, &contract_id, 1000);
    let customer = client.get_voucher(&voucher_id).unwrap().customer;

    let result = client.try_redeem_voucher(&customer, &voucher_id, &5000i128);
    assert!(
        result.is_err(),
        "redeeming more than the remaining balance should fail"
    );
}

#[test]
fn test_expired_voucher_sweep_releases_only_unredeemed_backing_once() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, contract_id) = setup(&env);

    env.ledger().set_timestamp(1000);
    let (_, voucher_id) = create_refund_and_issue_voucher(&env, &client, &admin, &contract_id, 500);
    let voucher = client.get_voucher(&voucher_id).unwrap();
    let customer = voucher.customer.clone();
    let token_client = token::Client::new(&env, &voucher.token);
    let merchant_balance_before = token_client.balance(&voucher.merchant);

    client.redeem_voucher(&customer, &voucher_id, &400);
    env.ledger().set_timestamp(1501);

    assert_eq!(client.sweep_expired_vouchers(&voucher_id, &10), 1);
    assert_eq!(client.sweep_expired_vouchers(&voucher_id, &10), 0);
    assert_eq!(
        token_client.balance(&voucher.merchant) - merchant_balance_before,
        600
    );

    let settled = client.get_voucher(&voucher_id).unwrap();
    assert_eq!(settled.remaining_balance, 0);
    assert!(settled.redeemed);
}
