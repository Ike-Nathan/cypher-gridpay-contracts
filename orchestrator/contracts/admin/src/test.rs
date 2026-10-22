#![cfg(test)]

use super::*;
use soroban_sdk::testutils::{storage::Instance as _, Address as _, Ledger};

struct Setup {
    env: Env,
    contract_id: Address,
    admin: Address,
    pauser: Address,
    payment: Address,
    escrow: Address,
    refund: Address,
}

impl Setup {
    fn client(&self) -> AdminContractClient<'_> {
        AdminContractClient::new(&self.env, &self.contract_id)
    }

    fn reason(&self) -> String {
        String::from_str(&self.env, "security incident")
    }
}

fn setup_payment(env: &Env, admin: &Address) -> Address {
    let contract_id = env.register(payments::PaymentContract, ());
    PaymentContractClient::new(env, &contract_id).initialize(admin);
    contract_id
}

fn setup_escrow(env: &Env, admin: &Address) -> Address {
    let contract_id = env.register(escrow::EscrowContract, ());
    EscrowContractClient::new(env, &contract_id).initialize(admin);
    contract_id
}

fn setup_refund(env: &Env, admin: &Address) -> Address {
    let contract_id = env.register(refund::RefundContract, ());
    RefundContractClient::new(env, &contract_id).initialize(admin);
    contract_id
}

/// Registers the admin contract and child contracts (administered by the
/// pauser) without initializing the admin contract.
fn setup_uninitialized() -> Setup {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AdminContract, ());
    let admin = Address::generate(&env);
    let pauser = Address::generate(&env);
    let payment = setup_payment(&env, &pauser);
    let escrow = setup_escrow(&env, &pauser);
    let refund = setup_refund(&env, &pauser);

    Setup {
        env,
        contract_id,
        admin,
        pauser,
        payment,
        escrow,
        refund,
    }
}

fn setup() -> Setup {
    let s = setup_uninitialized();
    s.client()
        .initialize(&s.admin, &s.pauser, &s.payment, &s.escrow, &s.refund);
    s
}

fn payment_paused(env: &Env, id: &Address) -> bool {
    PaymentContractClient::new(env, id)
        .get_pause_state()
        .globally_paused
}

fn escrow_paused(env: &Env, id: &Address) -> bool {
    EscrowContractClient::new(env, id)
        .get_pause_state()
        .globally_paused
}

fn refund_paused(env: &Env, id: &Address) -> bool {
    RefundContractClient::new(env, id)
        .get_pause_state()
        .globally_paused
}

fn all_paused(s: &Setup) -> [bool; 3] {
    [
        payment_paused(&s.env, &s.payment),
        escrow_paused(&s.env, &s.escrow),
        refund_paused(&s.env, &s.refund),
    ]
}

// ── Initialization ──────────────────────────────────────────────────────────

#[test]
fn test_initialize_stores_configuration() {
    let s = setup();
    s.env.as_contract(&s.contract_id, || {
        let storage = s.env.storage().instance();
        assert_eq!(
            storage.get::<_, Address>(&DataKey::Admin),
            Some(s.admin.clone())
        );
        assert_eq!(
            storage.get::<_, Address>(&DataKey::Pauser),
            Some(s.pauser.clone())
        );
        assert_eq!(
            storage.get::<_, Address>(&DataKey::PaymentContract),
            Some(s.payment.clone())
        );
        assert_eq!(
            storage.get::<_, Address>(&DataKey::EscrowContract),
            Some(s.escrow.clone())
        );
        assert_eq!(
            storage.get::<_, Address>(&DataKey::RefundContract),
            Some(s.refund.clone())
        );
    });
}

#[test]
fn test_initialize_twice_fails() {
    let s = setup();
    let other = Address::generate(&s.env);
    let res = s
        .client()
        .try_initialize(&other, &other, &s.payment, &s.escrow, &s.refund);
    assert_eq!(res, Err(Ok(Error::AlreadyInitialized)));
}

#[test]
fn test_initialize_requires_admin_auth() {
    let s = setup_uninitialized();
    s.env.set_auths(&[]);
    let res = s
        .client()
        .try_initialize(&s.admin, &s.pauser, &s.payment, &s.escrow, &s.refund);
    assert!(res.is_err());
}

// ── Uninitialized state ─────────────────────────────────────────────────────

#[test]
fn test_pause_all_uninitialized_fails() {
    let s = setup_uninitialized();
    let res = s.client().try_emergency_pause_all(&s.pauser, &s.reason());
    assert_eq!(res, Err(Ok(Error::NotInitialized)));
}

#[test]
fn test_unpause_all_uninitialized_fails() {
    let s = setup_uninitialized();
    let res = s.client().try_emergency_unpause_all(&s.pauser);
    assert_eq!(res, Err(Ok(Error::NotInitialized)));
}

#[test]
fn test_setters_uninitialized_fail() {
    let s = setup_uninitialized();
    let c = s.client();
    let new_addr = Address::generate(&s.env);
    assert_eq!(
        c.try_set_payment_contract(&s.admin, &new_addr),
        Err(Ok(Error::NotInitialized))
    );
    assert_eq!(
        c.try_set_escrow_contract(&s.admin, &new_addr),
        Err(Ok(Error::NotInitialized))
    );
    assert_eq!(
        c.try_set_refund_contract(&s.admin, &new_addr),
        Err(Ok(Error::NotInitialized))
    );
}

#[test]
fn test_ping_uninitialized_succeeds() {
    let s = setup_uninitialized();
    s.client().ping();
}

// ── Authorization ───────────────────────────────────────────────────────────

#[test]
fn test_pause_all_by_non_pauser_fails() {
    let s = setup();
    let attacker = Address::generate(&s.env);
    let res = s.client().try_emergency_pause_all(&attacker, &s.reason());
    assert_eq!(res, Err(Ok(Error::Unauthorized)));
    assert_eq!(all_paused(&s), [false, false, false]);
}

#[test]
fn test_pause_all_by_admin_not_pauser_fails() {
    let s = setup();
    let res = s.client().try_emergency_pause_all(&s.admin, &s.reason());
    assert_eq!(res, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_unpause_all_by_non_pauser_fails() {
    let s = setup();
    s.client().emergency_pause_all(&s.pauser, &s.reason());
    let attacker = Address::generate(&s.env);
    let res = s.client().try_emergency_unpause_all(&attacker);
    assert_eq!(res, Err(Ok(Error::Unauthorized)));
    assert_eq!(all_paused(&s), [true, true, true]);
}

#[test]
fn test_pause_all_without_signature_fails() {
    let s = setup();
    s.env.set_auths(&[]);
    let res = s.client().try_emergency_pause_all(&s.pauser, &s.reason());
    assert!(res.is_err());
}

#[test]
fn test_setters_by_non_admin_fail() {
    let s = setup();
    let c = s.client();
    let new_addr = Address::generate(&s.env);
    for caller in [Address::generate(&s.env), s.pauser.clone()] {
        assert_eq!(
            c.try_set_payment_contract(&caller, &new_addr),
            Err(Ok(Error::Unauthorized))
        );
        assert_eq!(
            c.try_set_escrow_contract(&caller, &new_addr),
            Err(Ok(Error::Unauthorized))
        );
        assert_eq!(
            c.try_set_refund_contract(&caller, &new_addr),
            Err(Ok(Error::Unauthorized))
        );
    }
}

#[test]
fn test_setters_by_admin_redirect_pause() {
    let mut s = setup();
    let payment = setup_payment(&s.env, &s.pauser);
    let escrow = setup_escrow(&s.env, &s.pauser);
    let refund = setup_refund(&s.env, &s.pauser);
    let c = s.client();
    c.set_payment_contract(&s.admin, &payment);
    c.set_escrow_contract(&s.admin, &escrow);
    c.set_refund_contract(&s.admin, &refund);
    c.emergency_pause_all(&s.pauser, &s.reason());

    // The previously configured contracts are untouched.
    assert_eq!(all_paused(&s), [false, false, false]);

    s.payment = payment;
    s.escrow = escrow;
    s.refund = refund;
    assert_eq!(all_paused(&s), [true, true, true]);
}

// ── Pause / unpause behaviour ───────────────────────────────────────────────

#[test]
fn test_pause_and_unpause_all() {
    let s = setup();
    s.client().emergency_pause_all(&s.pauser, &s.reason());
    assert_eq!(all_paused(&s), [true, true, true]);
    s.client().emergency_unpause_all(&s.pauser);
    assert_eq!(all_paused(&s), [false, false, false]);
}

#[test]
fn test_pause_all_is_idempotent() {
    let s = setup();
    s.client().emergency_pause_all(&s.pauser, &s.reason());
    s.client().emergency_pause_all(&s.pauser, &s.reason());
    assert_eq!(all_paused(&s), [true, true, true]);

    // A single unpause fully restores operation after repeated pauses.
    s.client().emergency_unpause_all(&s.pauser);
    assert_eq!(all_paused(&s), [false, false, false]);
}

#[test]
fn test_unpause_all_is_idempotent() {
    let s = setup();
    // Unpausing when nothing is paused is a no-op.
    s.client().emergency_unpause_all(&s.pauser);
    assert_eq!(all_paused(&s), [false, false, false]);

    s.client().emergency_pause_all(&s.pauser, &s.reason());
    s.client().emergency_unpause_all(&s.pauser);
    s.client().emergency_unpause_all(&s.pauser);
    assert_eq!(all_paused(&s), [false, false, false]);
}

// ── Simulated failures ──────────────────────────────────────────────────────

#[test]
fn test_pause_all_rolls_back_when_child_rejects() {
    let mut s = setup();
    // Escrow is administered by someone else, so the cross-contract pause fails.
    let foreign_admin = Address::generate(&s.env);
    s.escrow = setup_escrow(&s.env, &foreign_admin);
    s.client().set_escrow_contract(&s.admin, &s.escrow);

    let res = s.client().try_emergency_pause_all(&s.pauser, &s.reason());
    assert!(res.is_err());
    // The payment pause executed before the failure must be rolled back.
    assert_eq!(all_paused(&s), [false, false, false]);
}

#[test]
fn test_pause_all_fails_when_child_not_deployed() {
    let s = setup();
    let missing = Address::generate(&s.env);
    s.client().set_refund_contract(&s.admin, &missing);

    let res = s.client().try_emergency_pause_all(&s.pauser, &s.reason());
    assert!(res.is_err());
    assert!(!payment_paused(&s.env, &s.payment));
    assert!(!escrow_paused(&s.env, &s.escrow));
}

// ── TTL ─────────────────────────────────────────────────────────────────────

fn instance_ttl(s: &Setup) -> u32 {
    s.env
        .as_contract(&s.contract_id, || s.env.storage().instance().get_ttl())
}

#[test]
fn test_initialize_extends_instance_ttl() {
    let s = setup();
    assert_eq!(instance_ttl(&s), INSTANCE_BUMP_AMOUNT);
}

#[test]
fn test_ping_extends_instance_ttl() {
    let s = setup();

    // Let the TTL decay below the threshold, then ping to restore it.
    s.env
        .ledger()
        .with_mut(|l| l.sequence_number += 2 * DAY_IN_LEDGERS);
    assert!(instance_ttl(&s) < INSTANCE_LIFETIME_THRESHOLD);

    s.client().ping();
    assert_eq!(instance_ttl(&s), INSTANCE_BUMP_AMOUNT);
}

#[test]
fn test_admin_calls_extend_instance_ttl() {
    let s = setup();
    s.env
        .ledger()
        .with_mut(|l| l.sequence_number += 2 * DAY_IN_LEDGERS);
    s.client().emergency_pause_all(&s.pauser, &s.reason());
    assert_eq!(instance_ttl(&s), INSTANCE_BUMP_AMOUNT);

    s.env
        .ledger()
        .with_mut(|l| l.sequence_number += 2 * DAY_IN_LEDGERS);
    s.client().set_payment_contract(&s.admin, &s.payment);
    assert_eq!(instance_ttl(&s), INSTANCE_BUMP_AMOUNT);
}

// ── Rotation handshake (Issue #237) ─────────────────────────────────────────

mod peer {
    use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

    #[contracttype]
    enum Key {
        Initialized,
        Rejecting,
        Peer(Symbol),
    }

    /// Stand-in sibling contract exposing the rotation handshake entry points.
    #[contract]
    pub struct MockPeer;

    #[contractimpl]
    impl MockPeer {
        pub fn configure(env: Env, initialized: bool, rejecting: bool) {
            env.storage()
                .instance()
                .set(&Key::Initialized, &initialized);
            env.storage().instance().set(&Key::Rejecting, &rejecting);
        }

        pub fn get_schema_version(_env: Env) -> u32 {
            1
        }

        pub fn is_initialized(env: Env) -> bool {
            env.storage()
                .instance()
                .get(&Key::Initialized)
                .unwrap_or(false)
        }

        pub fn set_payment_contract(env: Env, caller: Address, address: Address) {
            let name = Symbol::new(&env, "payment");
            Self::set_peer(env, caller, name, address);
        }

        pub fn set_escrow_contract(env: Env, caller: Address, address: Address) {
            let name = Symbol::new(&env, "escrow");
            Self::set_peer(env, caller, name, address);
        }

        pub fn set_refund_contract(env: Env, caller: Address, address: Address) {
            let name = Symbol::new(&env, "refund");
            Self::set_peer(env, caller, name, address);
        }

        pub fn peer(env: Env, name: Symbol) -> Option<Address> {
            env.storage().instance().get(&Key::Peer(name))
        }
    }

    impl MockPeer {
        fn set_peer(env: Env, caller: Address, name: Symbol, address: Address) {
            caller.require_auth();
            let rejecting: bool = env
                .storage()
                .instance()
                .get(&Key::Rejecting)
                .unwrap_or(false);
            if rejecting {
                panic!("peer rejected address update");
            }
            env.storage().instance().set(&Key::Peer(name), &address);
        }
    }
}

fn setup_peer(env: &Env, initialized: bool, rejecting: bool) -> Address {
    let id = env.register(peer::MockPeer, ());
    peer::MockPeerClient::new(env, &id).configure(&initialized, &rejecting);
    id
}

/// Admin contract wired to three mock siblings.
fn setup_with_peers(refund_rejects: bool) -> Setup {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(AdminContract, ());
    let admin = Address::generate(&env);
    let pauser = Address::generate(&env);
    let payment = setup_peer(&env, true, false);
    let escrow = setup_peer(&env, true, false);
    let refund = setup_peer(&env, true, refund_rejects);
    let s = Setup {
        env,
        contract_id,
        admin,
        pauser,
        payment,
        escrow,
        refund,
    };
    s.client()
        .initialize(&s.admin, &s.pauser, &s.payment, &s.escrow, &s.refund);
    s
}

fn peer_of(s: &Setup, contract: &Address, name: &str) -> Option<Address> {
    peer::MockPeerClient::new(&s.env, contract).peer(&Symbol::new(&s.env, name))
}

fn pass_rotation_timelock(s: &Setup) {
    s.env
        .ledger()
        .with_mut(|l| l.timestamp += CONTRACT_ROTATION_TIMELOCK);
}

#[test]
fn test_escrow_rotation_propagates_to_peers() {
    let s = setup_with_peers(false);
    let c = s.client();
    let new_escrow = setup_peer(&s.env, true, false);

    c.set_escrow_contract(&s.admin, &new_escrow);
    pass_rotation_timelock(&s);
    c.execute_escrow_rotation(&s.admin);

    assert_eq!(peer_of(&s, &s.payment, "escrow"), Some(new_escrow.clone()));
    assert_eq!(peer_of(&s, &s.refund, "escrow"), Some(new_escrow.clone()));
    assert!(c.is_deprecated_contract(&ContractTarget::Escrow, &s.escrow));
    assert!(!c.is_deprecated_contract(&ContractTarget::Escrow, &new_escrow));
}

#[test]
fn test_payment_and_refund_rotations_propagate_to_peers() {
    let s = setup_with_peers(false);
    let c = s.client();
    let new_payment = setup_peer(&s.env, true, false);
    let new_refund = setup_peer(&s.env, true, false);

    c.set_payment_contract(&s.admin, &new_payment);
    c.set_refund_contract(&s.admin, &new_refund);
    pass_rotation_timelock(&s);
    c.execute_payment_rotation(&s.admin);
    c.execute_refund_rotation(&s.admin);

    assert_eq!(peer_of(&s, &s.escrow, "payment"), Some(new_payment.clone()));
    assert_eq!(peer_of(&s, &s.refund, "payment"), Some(new_payment.clone()));
    // The refund rotation reaches the already-rotated payment contract.
    assert_eq!(
        peer_of(&s, &new_payment, "refund"),
        Some(new_refund.clone())
    );
    assert_eq!(peer_of(&s, &s.escrow, "refund"), Some(new_refund));
    assert_eq!(
        c.get_deprecated_contracts(&ContractTarget::Payment),
        Vec::from_array(&s.env, [s.payment.clone()])
    );
}

#[test]
fn test_rotation_rejects_uninitialized_target() {
    let s = setup_with_peers(false);
    let c = s.client();
    let uninitialized = setup_peer(&s.env, false, false);

    c.set_escrow_contract(&s.admin, &uninitialized);
    pass_rotation_timelock(&s);
    assert_eq!(
        c.try_execute_escrow_rotation(&s.admin),
        Err(Ok(Error::RotationTargetNotReady))
    );
    assert!(c.get_pending_rotation(&ContractTarget::Escrow).is_some());
    assert_eq!(peer_of(&s, &s.payment, "escrow"), None);
}

#[test]
fn test_rotation_rejects_non_contract_target() {
    let s = setup_with_peers(false);
    let c = s.client();
    c.set_payment_contract(&s.admin, &Address::generate(&s.env));
    pass_rotation_timelock(&s);
    assert_eq!(
        c.try_execute_payment_rotation(&s.admin),
        Err(Ok(Error::RotationTargetNotReady))
    );
}

#[test]
fn test_rotation_rejects_current_address() {
    let s = setup_with_peers(false);
    let c = s.client();
    c.set_refund_contract(&s.admin, &s.refund);
    pass_rotation_timelock(&s);
    assert_eq!(
        c.try_execute_refund_rotation(&s.admin),
        Err(Ok(Error::RotationToCurrentAddress))
    );
}

#[test]
fn test_rotation_rolls_back_when_peer_rejects() {
    let s = setup_with_peers(true);
    let c = s.client();
    let new_escrow = setup_peer(&s.env, true, false);

    c.set_escrow_contract(&s.admin, &new_escrow);
    pass_rotation_timelock(&s);
    assert_eq!(
        c.try_execute_escrow_rotation(&s.admin),
        Err(Ok(Error::PeerSyncFailed))
    );

    // Payment's update, the deprecation entry and the address swap all roll back.
    assert_eq!(peer_of(&s, &s.payment, "escrow"), None);
    assert!(c
        .get_deprecated_contracts(&ContractTarget::Escrow)
        .is_empty());
    assert!(c.get_pending_rotation(&ContractTarget::Escrow).is_some());
}
