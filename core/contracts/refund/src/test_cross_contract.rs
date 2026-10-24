#![cfg(test)]

use super::*;
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, testutils::Address as _,
    token, Address, Env, IntoVal, String, Symbol,
};

#[contracterror]
#[derive(Clone, Debug, PartialEq)]
pub enum StubError {
    Refusing = 1,
}

fn setup(env: &Env) -> (Address, Address) {
    let id = env.register(RefundContract, ());
    let client = RefundContractClient::new(env, &id);
    let admin = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin);
    (id, admin)
}

fn reason(env: &Env) -> String {
    String::from_str(env, "reason")
}

fn client_of<'a>(env: &'a Env, id: &Address) -> RefundContractClient<'a> {
    RefundContractClient::new(env, id)
}

/// Registers the payment-contract stand-in and wires it into the refund contract.
struct Fixture {
    refund_id: Address,
    payment_id: Address,
    admin: Address,
    merchant: Address,
    customer: Address,
    stranger: Address,
    token: Address,
}

fn fixture_with_payment(env: &Env) -> Fixture {
    let (refund_id, admin) = setup(env);
    let merchant = Address::generate(env);
    let customer = Address::generate(env);
    let stranger = Address::generate(env);
    let token = Address::generate(env);

    let payment_id = env.register(StubPaymentContract, ());
    client_of(env, &refund_id).set_payment_contract_address(&admin, &payment_id);

    Fixture {
        refund_id,
        payment_id,
        admin,
        merchant,
        customer,
        stranger,
        token,
    }
}

// ── Issue #70: a payment contract whose behaviour the refund contract must
// survive: panicking, reverting, and mis-answering. ─────────────────────────

/// A child that can be told to succeed, revert, or panic. The refund contract
/// has to fail safely in every case.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StubMode {
    Ok,
    Revert,
    Panic,
    NoVerificationFn,
    /// Answers successfully but reports the payment contract as paused, the way
    /// a real contract does when `is_function_paused` is true.
    PausedUnavailable,
}

#[contracttype]
pub enum StubKey {
    Mode,
    Status,
    Customer,
}

/// Mirrors `payments::PaymentVerification` exactly so the refund contract's
/// `PaymentContractVerification` decodes it. A mismatch here is exactly the
/// schema drift the primitive-only design is meant to survive.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StubPaymentVerification {
    pub payment_contract_available: bool,
    pub exists: bool,
    pub is_completed: bool,
    pub owned_by_customer: bool,
}

#[contract]
pub struct StubPaymentContract;

#[contractimpl]
impl StubPaymentContract {
    pub fn set_mode(env: Env, mode: StubMode) {
        env.storage().instance().set(&StubKey::Mode, &mode);
    }

    /// Seeds a completed payment owned by `customer`.
    pub fn seed_completed_payment(env: Env, payment_id: u64, customer: Address) {
        env.storage().instance().set(&StubKey::Status, &true);
        env.storage().instance().set(&StubKey::Customer, &customer);
        env.storage()
            .instance()
            .set(&payment_id_key(payment_id), &true);
    }

    /// Seeds a payment that exists but is not `Completed`.
    pub fn seed_incomplete_payment(env: Env, payment_id: u64, customer: Address) {
        env.storage().instance().set(&StubKey::Status, &false);
        env.storage().instance().set(&StubKey::Customer, &customer);
        env.storage()
            .instance()
            .set(&payment_id_key(payment_id), &true);
    }

    pub fn get_payment_verification(
        env: Env,
        payment_id: u64,
        customer: Address,
    ) -> StubPaymentVerification {
        let mode: StubMode = env
            .storage()
            .instance()
            .get(&StubKey::Mode)
            .unwrap_or(StubMode::Ok);

        match mode {
            // Revert with a contract error, the way a real child would refuse.
            StubMode::Revert => panic_with_error!(&env, StubError::Refusing),
            // Hard trap: the child is unreachable/broken mid-call.
            StubMode::Panic => panic!("payment contract exploded"),
            // The child does not export the verification function at all.
            StubMode::NoVerificationFn => {
                return StubPaymentVerification {
                    payment_contract_available: true,
                    exists: false,
                    is_completed: false,
                    owned_by_customer: false,
                }
            }
            StubMode::Ok => {}
            StubMode::PausedUnavailable => {}
        }

        let exists = env
            .storage()
            .instance()
            .get::<PaymentIdKey, bool>(&payment_id_key(payment_id))
            .unwrap_or(false);
        let is_completed = env
            .storage()
            .instance()
            .get::<StubKey, bool>(&StubKey::Status)
            .unwrap_or(false);
        let owner: Address = env
            .storage()
            .instance()
            .get(&StubKey::Customer)
            .unwrap_or(env.current_contract_address());

        StubPaymentVerification {
            payment_contract_available: mode != StubMode::PausedUnavailable,
            exists,
            is_completed,
            owned_by_customer: exists && owner == customer,
        }
    }
}

#[contracttype]
pub enum PaymentIdKey {
    Payment(u64),
}

fn payment_id_key(id: u64) -> PaymentIdKey {
    PaymentIdKey::Payment(id)
}

// If payment contract address is not set, verification is skipped (backward-compatible)
#[test]
fn test_request_refund_without_payment_contract_set() {
    let env = Env::default();
    let (id, _) = setup(&env);
    let client = client_of(&env, &id);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = Address::generate(&env);

    // No payment contract set — should succeed normally
    let refund_id = client.request_refund(
        &merchant,
        &1u64,
        &customer,
        &500i128,
        &1000i128,
        &token,
        &String::from_str(&env, "reason"),
        &RefundReasonCode::Other,
        &0u64,
    );
    assert_eq!(refund_id, 1u64);
}

// set/get payment contract address
#[test]
fn test_set_get_payment_contract_address() {
    let env = Env::default();
    let (id, admin) = setup(&env);
    let client = client_of(&env, &id);
    let payment_contract = Address::generate(&env);

    assert!(client.get_payment_contract_address().is_none());
    client.set_payment_contract_address(&admin, &payment_contract);
    assert_eq!(
        client.get_payment_contract_address().unwrap(),
        payment_contract
    );
}

// verify_payment_ownership returns false when no contract set
#[test]
fn test_verify_ownership_no_contract_returns_false() {
    let env = Env::default();
    let (id, _) = setup(&env);
    let client = client_of(&env, &id);
    let customer = Address::generate(&env);
    let result = client.verify_payment_ownership(&1u64, &customer);
    assert!(!result);
}

// Ownership mismatch: wrong customer returns PaymentOwnershipMismatch
// (tested via mock: set a fake payment contract address and expect the
//  cross-contract call to return false → refund request rejected)
#[test]
fn test_ownership_mismatch_rejects_refund() {
    let env = Env::default();
    let (id, admin) = setup(&env);
    let client = client_of(&env, &id);

    // Point to a random address as "payment contract" — calls will fail → false
    let fake_payment_contract = Address::generate(&env);
    client.set_payment_contract_address(&admin, &fake_payment_contract);

    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = Address::generate(&env);

    // Should fail with PaymentOwnershipMismatch because cross-contract call returns false
    let result = client.try_request_refund(
        &merchant,
        &1u64,
        &customer,
        &500i128,
        &1000i128,
        &token,
        &String::from_str(&env, "reason"),
        &RefundReasonCode::Other,
        &0u64,
    );
    assert!(result.is_err());
}

// ── Issue #70 ───────────────────────────────────────────────────────────────

/// Issue #70: when the configured payment contract is not a contract at all,
/// the cross-contract call fails. The refund must fail with the specific
/// `PaymentContractCallFailed` code rather than silently degrading to a
/// generic ownership mismatch.
#[test]
fn test_refund_fails_with_payment_contract_call_failed() {
    let env = Env::default();
    let (id, admin) = setup(&env);
    let client = client_of(&env, &id);
    client.set_payment_contract_address(&admin, &Address::generate(&env));

    let result = client.try_request_refund(
        &Address::generate(&env),
        &1u64,
        &Address::generate(&env),
        &500i128,
        &1000i128,
        &Address::generate(&env),
        &reason(&env),
        &RefundReasonCode::Other,
        &0u64,
    );
    assert_eq!(
        result,
        Err(Ok(Error::Ext(ExtError::PaymentContractCallFailed)))
    );
}

/// A child that reverts must not be mistaken for a negative answer.
#[test]
fn test_refund_fails_when_payment_contract_reverts() {
    let env = Env::default();
    let f = fixture_with_payment(&env);
    StubPaymentContractClient::new(&env, &f.payment_id).set_mode(&StubMode::Revert);

    let result = client_of(&env, &f.refund_id).try_request_refund(
        &f.merchant,
        &1u64,
        &f.customer,
        &500i128,
        &1000i128,
        &f.token,
        &reason(&env),
        &RefundReasonCode::Other,
        &0u64,
    );
    assert_eq!(
        result,
        Err(Ok(Error::Ext(ExtError::PaymentContractCallFailed)))
    );
}

/// A child that traps must not be mistaken for a negative answer either.
#[test]
fn test_refund_fails_when_payment_contract_panics() {
    let env = Env::default();
    let f = fixture_with_payment(&env);
    StubPaymentContractClient::new(&env, &f.payment_id).set_mode(&StubMode::Panic);

    let result = client_of(&env, &f.refund_id).try_request_refund(
        &f.merchant,
        &1u64,
        &f.customer,
        &500i128,
        &1000i128,
        &f.token,
        &reason(&env),
        &RefundReasonCode::Other,
        &0u64,
    );
    assert!(result.is_err());
    // The refund must not have been recorded.
    assert!(matches!(
        result,
        Err(Ok(Error::Ext(ExtError::PaymentContractCallFailed)))
    ));
}

/// Issue #70: a paused payment contract reports
/// `payment_contract_available == false`. The refund must fail with
/// `PaymentContractUnavailable` rather than being allowed through.
#[test]
fn test_refund_fails_when_payment_contract_is_paused() {
    let env = Env::default();
    let f = fixture_with_payment(&env);
    let stub = StubPaymentContractClient::new(&env, &f.payment_id);
    stub.seed_completed_payment(&1u64, &f.customer);
    stub.set_mode(&StubMode::PausedUnavailable);

    let result = client_of(&env, &f.refund_id).try_request_refund(
        &f.merchant,
        &1u64,
        &f.customer,
        &500i128,
        &1000i128,
        &f.token,
        &reason(&env),
        &RefundReasonCode::Other,
        &0u64,
    );
    assert_eq!(
        result,
        Err(Ok(Error::Ext(ExtError::PaymentContractUnavailable)))
    );
}

/// Issue #70: the payment status must be `Completed` before a refund is allowed.
#[test]
fn test_refund_rejected_when_payment_not_completed() {
    let env = Env::default();
    let f = fixture_with_payment(&env);
    let stub = StubPaymentContractClient::new(&env, &f.payment_id);
    stub.seed_incomplete_payment(&7u64, &f.customer);

    let result = client_of(&env, &f.refund_id).try_request_refund(
        &f.merchant,
        &7u64,
        &f.customer,
        &500i128,
        &1000i128,
        &f.token,
        &reason(&env),
        &RefundReasonCode::Other,
        &0u64,
    );
    assert_eq!(result, Err(Ok(Error::Ext(ExtError::PaymentNotCompleted))));
}

/// An unknown payment ID is reported as an invalid payment, not as a mismatch.
#[test]
fn test_refund_rejected_when_payment_missing() {
    let env = Env::default();
    let f = fixture_with_payment(&env);
    let stub = StubPaymentContractClient::new(&env, &f.payment_id);
    stub.seed_completed_payment(&1u64, &f.customer);

    let result = client_of(&env, &f.refund_id).try_request_refund(
        &f.merchant,
        &999u64,
        &f.customer,
        &500i128,
        &1000i128,
        &f.token,
        &reason(&env),
        &RefundReasonCode::Other,
        &0u64,
    );
    assert_eq!(result, Err(Ok(Error::Core(CoreError::InvalidPaymentId))));
}

/// A completed payment owned by the right customer is refundable, and each
/// precondition is reported individually.
#[test]
fn test_verify_payment_state_accepts_completed_payment() {
    let env = Env::default();
    let f = fixture_with_payment(&env);
    let stub = StubPaymentContractClient::new(&env, &f.payment_id);
    stub.seed_completed_payment(&3u64, &f.customer);

    let v = client_of(&env, &f.refund_id).verify_payment_state(&3u64, &f.customer);
    assert!(v.payment_contract_configured);
    assert!(v.payment_contract_reachable);
    assert!(v.payment_contract_available);
    assert!(v.payment_exists);
    assert!(v.payment_completed);
    assert!(v.owned_by_customer);
    assert!(v.refundable);

    assert!(client_of(&env, &f.refund_id).verify_payment_ownership(&3u64, &f.customer));
}

/// The preconditions are reported separately so callers can tell them apart.
#[test]
fn test_verify_payment_state_reports_each_precondition() {
    let env = Env::default();
    let f = fixture_with_payment(&env);
    let stub = StubPaymentContractClient::new(&env, &f.payment_id);
    stub.seed_completed_payment(&3u64, &f.customer);

    // Wrong customer: exists and completed, but not owned.
    let v = client_of(&env, &f.refund_id).verify_payment_state(&3u64, &f.stranger);
    assert!(v.payment_contract_configured);
    assert!(v.payment_exists);
    assert!(v.payment_completed);
    assert!(!v.owned_by_customer);
    assert!(!v.refundable);

    // Unknown payment.
    let v = client_of(&env, &f.refund_id).verify_payment_state(&404u64, &f.customer);
    assert!(v.payment_contract_reachable);
    assert!(!v.payment_exists);
    assert!(!v.refundable);

    // Not completed.
    stub.seed_incomplete_payment(&4u64, &f.customer);
    let v = client_of(&env, &f.refund_id).verify_payment_state(&4u64, &f.customer);
    assert!(v.payment_exists);
    assert!(!v.payment_completed);
    assert!(!v.refundable);
}

/// Unconfigured payment contract: reported as not configured, verification is
/// skipped, and refunds keep working exactly as before the cross-contract check
/// existed.
#[test]
fn test_verify_payment_state_reports_unconfigured() {
    let env = Env::default();
    let (id, _) = setup(&env);
    let client = client_of(&env, &id);
    let v = client.verify_payment_state(&1u64, &Address::generate(&env));
    assert!(!v.payment_contract_configured);
    assert!(!v.refundable);
}

/// The payment contract's answer is re-checked immediately before money moves,
/// so a refund approved while the payment was `Completed` cannot be paid out
/// after the payment contract has been paused.
#[test]
fn test_process_refund_rechecks_payment_state() {
    let env = Env::default();
    let f = fixture_with_payment(&env);
    let stub = StubPaymentContractClient::new(&env, &f.payment_id);
    stub.seed_completed_payment(&5u64, &f.customer);

    // A real token, funded, so the payout path is exercised for real.
    let asset = env.register_stellar_asset_contract_v2(f.admin.clone());
    let token_addr = asset.address();
    token::StellarAssetClient::new(&env, &token_addr).mint(&f.refund_id, &10_000);

    let refund_id = client_of(&env, &f.refund_id).request_refund(
        &f.merchant,
        &5u64,
        &f.customer,
        &1_000i128,
        &1_000i128,
        &token_addr,
        &reason(&env),
        &RefundReasonCode::Other,
        &0u64,
    );
    client_of(&env, &f.refund_id).approve_refund(&f.admin, &refund_id);

    // Between approval and payout the payment contract is paused.
    stub.set_mode(&StubMode::PausedUnavailable);

    let result = client_of(&env, &f.refund_id).try_process_refund(&f.admin, &refund_id);
    assert_eq!(
        result,
        Err(Ok(Error::Ext(ExtError::PaymentContractUnavailable)))
    );

    // Nothing moved: the refund is still awaiting payout and the customer has
    // received nothing.
    assert_eq!(
        client_of(&env, &f.refund_id).get_refund(&refund_id).status,
        RefundStatus::Approved
    );
    assert_eq!(
        token::Client::new(&env, &token_addr).balance(&f.customer),
        0
    );
}

/// The happy path: a completed payment stays refundable through payout.
#[test]
fn test_process_refund_succeeds_with_consistent_payment_state() {
    let env = Env::default();
    let f = fixture_with_payment(&env);
    let stub = StubPaymentContractClient::new(&env, &f.payment_id);
    stub.seed_completed_payment(&5u64, &f.customer);

    let asset = env.register_stellar_asset_contract_v2(f.admin.clone());
    let token_addr = asset.address();
    token::StellarAssetClient::new(&env, &token_addr).mint(&f.refund_id, &10_000);

    let refund_id = client_of(&env, &f.refund_id).request_refund(
        &f.merchant,
        &5u64,
        &f.customer,
        &1_000i128,
        &1_000i128,
        &token_addr,
        &reason(&env),
        &RefundReasonCode::Other,
        &0u64,
    );
    client_of(&env, &f.refund_id).approve_refund(&f.admin, &refund_id);
    client_of(&env, &f.refund_id).process_refund(&f.admin, &refund_id);

    assert_eq!(
        client_of(&env, &f.refund_id).get_refund(&refund_id).status,
        RefundStatus::Processed
    );
    assert_eq!(
        token::Client::new(&env, &token_addr).balance(&f.customer),
        1_000
    );
}

/// The refund contract's mirror of the payment contract's verification struct
/// must stay byte-compatible, otherwise every cross-contract call would fail to
/// decode and every refund would be rejected.
#[test]
fn test_payment_contract_verification_mirror_is_wire_compatible() {
    use soroban_sdk::xdr::ToXdr;

    let env = Env::default();
    let f = fixture_with_payment(&env);
    let stub = StubPaymentContractClient::new(&env, &f.payment_id);
    stub.seed_completed_payment(&1u64, &f.customer);

    let stub_state = StubPaymentVerification {
        payment_contract_available: true,
        exists: true,
        is_completed: true,
        owned_by_customer: true,
    };
    let mirror = PaymentContractVerification {
        payment_contract_available: true,
        exists: true,
        is_completed: true,
        owned_by_customer: true,
    };
    assert_eq!(
        stub_state.clone().to_xdr(&env),
        mirror.clone().to_xdr(&env),
        "refund mirror must encode identically to the payment contract type"
    );

    // And the live cross-contract call must decode into the mirror.
    let args = (1u64, f.customer.clone()).into_val(&env);
    let raw: PaymentContractVerification = env.invoke_contract(
        &f.payment_id,
        &Symbol::new(&env, "get_payment_verification"),
        args,
    );
    assert!(raw.payment_contract_available);
    assert!(raw.exists);
    assert!(raw.is_completed);
    assert!(raw.owned_by_customer);
}
