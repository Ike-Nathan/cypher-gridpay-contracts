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

// ── Reentrancy guard (reentrancy.rs) ─────────────────────────────────────────

mod reentrancy_guard {
    use crate::reentrancy::{self, CallContextState, TemporaryKey};
    use crate::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env};

    /// A token whose `transfer` tries to re-enter the payment contract by
    /// opening a channel from inside the transfer, and records the outcome.
    #[contract]
    pub struct MaliciousToken;

    #[contracttype]
    enum HookKey {
        Target,
        Attempted,
        Rejected,
    }

    #[contractimpl]
    impl MaliciousToken {
        pub fn set_target(env: Env, target: Address) {
            env.storage().instance().set(&HookKey::Target, &target);
        }

        pub fn transfer(env: Env, from: Address, _to: Address, amount: i128) {
            let target: Address = env.storage().instance().get(&HookKey::Target).unwrap();
            let rejected = PaymentContractClient::new(&env, &target)
                .try_open_channel(
                    &from,
                    &from,
                    &env.current_contract_address(),
                    &amount,
                    &0,
                    &BytesN::from_array(&env, &[1; 32]),
                )
                .is_err();
            env.storage().instance().set(&HookKey::Attempted, &true);
            env.storage().instance().set(&HookKey::Rejected, &rejected);
        }

        pub fn reentry_rejected(env: Env) -> Option<bool> {
            env.storage().instance().get(&HookKey::Rejected)
        }
    }

    fn setup(env: &Env) -> (PaymentContractClient<'_>, Address, Address) {
        env.mock_all_auths();
        let contract_id = env.register(PaymentContract, ());
        let client = PaymentContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(&admin);
        (client, contract_id, admin)
    }

    fn set_lock(env: &Env, contract: &Address, held: bool) {
        env.as_contract(contract, || {
            if held {
                env.storage().temporary().set(&TemporaryKey::ReentrancyLock, &true);
            } else {
                env.storage().temporary().remove(&TemporaryKey::ReentrancyLock);
            }
        });
    }

    #[test]
    fn guarded_entry_point_rejects_a_call_while_the_lock_is_held() {
        let env = Env::default();
        let (client, contract_id, admin) = setup(&env);
        set_lock(&env, &contract_id, true);
        assert_eq!(
            client.try_withdraw_fees(&admin, &1),
            Err(Ok(Error::Basic(BasicError::ReentrantCallDetected)))
        );
        assert_eq!(
            client.try_complete_payment(&admin, &1),
            Err(Ok(Error::Basic(BasicError::ReentrantCallDetected)))
        );
        // Unguarded read-only calls are unaffected.
        client.get_accumulated_fees();
    }

    #[test]
    fn lock_is_released_after_success_and_after_an_error() {
        let env = Env::default();
        let (client, contract_id, admin) = setup(&env);
        let customer = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(admin.clone()).address();
        soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&customer, &1_000);

        client.open_channel(
            &customer,
            &Address::generate(&env),
            &token,
            &100,
            &0,
            &BytesN::from_array(&env, &[2; 32]),
        );
        assert!(!env.as_contract(&contract_id, || reentrancy::is_locked(&env)));

        // An error return doesn't leave the lock behind either.
        assert!(client.try_withdraw_fees(&Address::generate(&env), &1).is_err());
        assert!(!env.as_contract(&contract_id, || reentrancy::is_locked(&env)));
    }

    #[test]
    fn malicious_token_cannot_reenter_during_transfer() {
        let env = Env::default();
        let (client, contract_id, _) = setup(&env);
        let token = env.register(MaliciousToken, ());
        let hook = MaliciousTokenClient::new(&env, &token);
        hook.set_target(&contract_id);

        let customer = Address::generate(&env);
        // The outer open_channel calls token.transfer, which re-enters.
        let channel = client.open_channel(
            &customer,
            &Address::generate(&env),
            &token,
            &100,
            &0,
            &BytesN::from_array(&env, &[3; 32]),
        );
        assert_eq!(hook.reentry_rejected(), Some(true), "the reentrant call must be refused");
        // Exactly one channel exists: the reentrant one never got created.
        assert_eq!(client.get_channel(&channel).deposited, 100);
        assert!(client.try_get_channel(&(channel + 1)).is_err());
    }

    #[test]
    fn sibling_callbacks_are_accepted_only_from_the_expected_sibling() {
        let env = Env::default();
        let (_, contract_id, _) = setup(&env);
        let escrow = Address::generate(&env);
        let other = Address::generate(&env);
        env.as_contract(&contract_id, || {
            let expected = CallContextState::AwaitingEscrowCallback(escrow.clone());
            // Nothing expected yet.
            assert!(reentrancy::accept_callback(&env, &expected, &escrow).is_err());

            reentrancy::expect_callback(&env, expected.clone());
            assert!(reentrancy::accept_callback(&env, &expected, &escrow).is_ok());
            // Wrong caller, or the wrong kind of callback.
            assert!(reentrancy::accept_callback(&env, &expected, &other).is_err());
            assert!(reentrancy::accept_callback(
                &env,
                &CallContextState::AwaitingRefundCallback(escrow.clone()),
                &escrow
            )
            .is_err());

            reentrancy::end_callback(&env);
            assert_eq!(reentrancy::current_context(&env), CallContextState::Idle);
            assert!(reentrancy::accept_callback(&env, &expected, &escrow).is_err());
        });
    }
}
