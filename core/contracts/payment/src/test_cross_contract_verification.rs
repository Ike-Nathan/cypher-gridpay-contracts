#![cfg(test)]

//! Issue #70: the payment-contract half of cross-contract refund verification.
//!
//! The refund contract gates cross-contract refunds on this contract vouching
//! for payment state. These tests pin the contract's side of that contract:
//! it must answer honestly about existence, completion and ownership, and it
//! must refuse to vouch for anything at all while it is paused.

use super::*;
use soroban_sdk::{testutils::Address as _, token, Address, Env, String};

struct Fixture {
    env: Env,
    client: PaymentContractClient<'static>,
    customer: Address,
    merchant: Address,
    admin: Address,
    token: Address,
}

fn fixture() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    client.initialize(&admin);
    token::StellarAssetClient::new(&env, &token_addr).mint(&customer, &100_000);
    token::Client::new(&env, &token_addr).approve(&customer, &contract_id, &100_000, &100_000);

    Fixture {
        env,
        client,
        customer,
        merchant,
        admin,
        token: token_addr,
    }
}

fn create_payment(f: &Fixture, amount: i128) -> u64 {
    f.client.create_payment(
        &f.customer,
        &f.merchant,
        &amount,
        &f.token,
        &Currency::USDC,
        &0u64,
        &String::from_str(&f.env, "meta"),
    )
}

fn complete_payment(f: &Fixture, payment_id: u64) {
    f.client.complete_payment(&f.admin, &payment_id)
}

#[test]
fn test_verification_unknown_payment() {
    let f = fixture();
    let v = f.client.get_payment_verification(&404u64, &f.customer);
    assert!(v.payment_contract_available);
    assert!(!v.exists);
    assert!(!v.is_completed);
    assert!(!v.owned_by_customer);
    assert!(!f.client.check_payment_customer(&404u64, &f.customer));
}

#[test]
fn test_verification_pending_payment_is_not_refundable() {
    let f = fixture();
    let id = create_payment(&f, 1_000);

    let v = f.client.get_payment_verification(&id, &f.customer);
    assert!(v.payment_contract_available);
    assert!(v.exists);
    // Issue #70: a pending payment is not a settled payment, so it must not
    // pass the refund gate.
    assert!(!v.is_completed);
    assert!(v.owned_by_customer);
    assert!(!f.client.check_payment_customer(&id, &f.customer));
}

#[test]
fn test_verification_completed_payment_is_refundable() {
    let f = fixture();
    let id = create_payment(&f, 1_000);
    complete_payment(&f, id);

    let v = f.client.get_payment_verification(&id, &f.customer);
    assert!(v.payment_contract_available);
    assert!(v.exists);
    assert!(v.is_completed);
    assert!(v.owned_by_customer);
    assert!(f.client.check_payment_customer(&id, &f.customer));
}

#[test]
fn test_verification_wrong_customer_is_not_owned() {
    let f = fixture();
    let id = create_payment(&f, 1_000);
    complete_payment(&f, id);

    let stranger = Address::generate(&f.env);
    let v = f.client.get_payment_verification(&id, &stranger);
    assert!(v.exists);
    assert!(v.is_completed);
    assert!(!v.owned_by_customer);
    assert!(!f.client.check_payment_customer(&id, &stranger));
}

/// Issue #70: a globally paused payment contract must not vouch for any
/// payment state. Without this the refund contract would happily let refunds
/// through against a payment contract that can no longer service them.
#[test]
fn test_verification_refuses_to_vouch_while_globally_paused() {
    let f = fixture();
    let id = create_payment(&f, 1_000);
    complete_payment(&f, id);
    assert!(f.client.check_payment_customer(&id, &f.customer));

    f.client
        .pause_contract(&f.admin, &String::from_str(&f.env, "incident"));

    let v = f.client.get_payment_verification(&id, &f.customer);
    assert!(
        !v.payment_contract_available,
        "a paused payment contract must report itself unavailable"
    );
    // Fail closed: nothing is asserted about the payment while the contract is
    // paused, so a paused contract cannot be used as a state oracle either.
    assert!(!v.exists);
    assert!(!v.is_completed);
    assert!(!v.owned_by_customer);
    assert!(!f.client.check_payment_customer(&id, &f.customer));
}

/// Pausing just the verification entry point is enough to stop vouching.
#[test]
fn test_verification_refuses_to_vouch_when_its_own_function_is_paused() {
    let f = fixture();
    let id = create_payment(&f, 1_000);
    complete_payment(&f, id);

    f.client.pause_function(
        &f.admin,
        &String::from_str(&f.env, "get_payment_verification"),
        &String::from_str(&f.env, "incident"),
    );

    let v = f.client.get_payment_verification(&id, &f.customer);
    assert!(!v.payment_contract_available);
    assert!(!f.client.check_payment_customer(&id, &f.customer));
}

/// Unpausing restores the ability to vouch.
#[test]
fn test_verification_recovers_after_unpause() {
    let f = fixture();
    let id = create_payment(&f, 1_000);
    complete_payment(&f, id);

    f.client
        .pause_contract(&f.admin, &String::from_str(&f.env, "incident"));
    assert!(!f.client.check_payment_customer(&id, &f.customer));

    f.client.unpause_contract(&f.admin);
    assert!(f.client.check_payment_customer(&id, &f.customer));
}
