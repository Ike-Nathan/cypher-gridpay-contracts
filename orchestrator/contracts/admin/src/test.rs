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

// Disabled: exercises the instance-TTL helpers (ping, INSTANCE_BUMP_AMOUNT, DAY_IN_LEDGERS), which are not in lib.rs.
#[cfg(any())]
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

// Disabled: written for immediate set_*_contract rotation; since #77 a rotation only takes effect after the 48h timelock and execute_*_rotation.
#[cfg(any())]
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

// Disabled: written for immediate set_*_contract rotation; since #77 a rotation only takes effect after the 48h timelock and execute_*_rotation.
#[cfg(any())]
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

// Disabled: written for immediate set_*_contract rotation; since #77 a rotation only takes effect after the 48h timelock and execute_*_rotation.
#[cfg(any())]
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

// Disabled: exercises the instance-TTL helpers (ping, INSTANCE_BUMP_AMOUNT, DAY_IN_LEDGERS), which are not in lib.rs.
#[cfg(any())]
#[test]
fn test_initialize_extends_instance_ttl() {
    let s = setup();
    assert_eq!(instance_ttl(&s), INSTANCE_BUMP_AMOUNT);
}

// Disabled: exercises the instance-TTL helpers (ping, INSTANCE_BUMP_AMOUNT, DAY_IN_LEDGERS), which are not in lib.rs.
#[cfg(any())]
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

// Disabled: exercises the instance-TTL helpers (ping, INSTANCE_BUMP_AMOUNT, DAY_IN_LEDGERS), which are not in lib.rs.
#[cfg(any())]
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

// ── Scoped, time-bounded role grants ─────────────────────────────────────────

const DAY: u64 = 86_400;

fn hash(env: &Env, b: u8) -> BytesN<32> {
    BytesN::from_array(env, &[b; 32])
}

fn at(s: &Setup, ts: u64) {
    s.env.ledger().set_timestamp(ts);
}

#[test]
fn test_scoped_upgrade_proposer_can_only_target_its_contracts() {
    let s = setup();
    at(&s, 1_000);
    let operator = Address::generate(&s.env);
    s.client().grant_scoped_role(
        &s.admin,
        &operator,
        &OperatorRole::UpgradeProposer,
        &soroban_sdk::vec![&s.env, s.payment.clone()],
        &(1_000 + 7 * DAY),
    );

    // In scope: allowed.
    s.client().propose_upgrade(&operator, &s.payment, &hash(&s.env, 1));
    // Out of scope: rejected.
    assert_eq!(
        s.client().try_propose_upgrade(&operator, &s.escrow, &hash(&s.env, 2)),
        Err(Ok(Error::RoleTargetNotAllowed))
    );
    // No grant at all: rejected.
    assert_eq!(
        s.client()
            .try_propose_upgrade(&Address::generate(&s.env), &s.payment, &hash(&s.env, 3)),
        Err(Ok(Error::RoleGrantNotFound))
    );
    // The stored admin needs no grant.
    s.client().propose_upgrade(&s.admin, &s.escrow, &hash(&s.env, 4));
}

#[test]
fn test_unrestricted_grant_covers_every_target() {
    let s = setup();
    at(&s, 1_000);
    let operator = Address::generate(&s.env);
    s.client().grant_scoped_role(
        &s.admin,
        &operator,
        &OperatorRole::UpgradeProposer,
        &Vec::new(&s.env),
        &(1_000 + DAY),
    );
    for target in [&s.payment, &s.escrow, &s.refund] {
        s.client().propose_upgrade(&operator, target, &hash(&s.env, 9));
    }
}

#[test]
fn test_expired_grant_is_rejected() {
    let s = setup();
    at(&s, 1_000);
    let operator = Address::generate(&s.env);
    let expires_at = 1_000 + DAY;
    s.client().grant_scoped_role(
        &s.admin,
        &operator,
        &OperatorRole::UpgradeProposer,
        &Vec::new(&s.env),
        &expires_at,
    );

    at(&s, expires_at); // last valid second
    s.client().propose_upgrade(&operator, &s.payment, &hash(&s.env, 1));
    at(&s, expires_at + 1);
    assert_eq!(
        s.client().try_propose_upgrade(&operator, &s.payment, &hash(&s.env, 2)),
        Err(Ok(Error::RoleGrantExpired))
    );
}

#[test]
fn test_operational_grants_must_expire_within_the_maximum() {
    let s = setup();
    at(&s, 1_000);
    let operator = Address::generate(&s.env);
    let grant = |expires_at: u64| {
        s.client().try_grant_scoped_role(
            &s.admin,
            &operator,
            &OperatorRole::FeeHarvester,
            &Vec::new(&s.env),
            &expires_at,
        )
    };
    let max = DEFAULT_MAX_ROLE_GRANT_DURATION_SECS;
    assert_eq!(grant(0), Err(Ok(Error::InvalidRoleExpiry)), "no indefinite operator grants");
    assert_eq!(grant(1_000), Err(Ok(Error::InvalidRoleExpiry)), "already expired");
    assert_eq!(grant(1_000 + max + 1), Err(Ok(Error::InvalidRoleExpiry)), "too long");
    assert_eq!(grant(1_000 + max), Ok(Ok(())));

    // The maximum is configurable.
    s.client().set_role_policy(&s.admin, &DAY, &2);
    assert_eq!(grant(1_000 + 2 * DAY), Err(Ok(Error::InvalidRoleExpiry)));

    // SuperAdmin grants may be indefinite.
    s.client().grant_scoped_role(
        &s.admin,
        &Address::generate(&s.env),
        &OperatorRole::SuperAdmin,
        &Vec::new(&s.env),
        &0,
    );
}

#[test]
fn test_only_super_admins_grant_and_never_to_themselves() {
    let s = setup();
    at(&s, 1_000);
    let super_op = Address::generate(&s.env);
    let operator = Address::generate(&s.env);
    s.client().grant_scoped_role(
        &s.admin,
        &super_op,
        &OperatorRole::SuperAdmin,
        &Vec::new(&s.env),
        &(1_000 + 10 * DAY),
    );

    // A SuperAdmin grantee can grant others...
    s.client().grant_scoped_role(
        &super_op,
        &operator,
        &OperatorRole::ComplianceOfficer,
        &Vec::new(&s.env),
        &(1_000 + DAY),
    );
    // ...but not renew its own grant.
    assert_eq!(
        s.client().try_grant_scoped_role(
            &super_op,
            &super_op,
            &OperatorRole::SuperAdmin,
            &Vec::new(&s.env),
            &(1_000 + 100 * DAY),
        ),
        Err(Ok(Error::SelfGrantForbidden))
    );
    // An operational role holder can't grant at all.
    assert_eq!(
        s.client().try_grant_scoped_role(
            &operator,
            &Address::generate(&s.env),
            &OperatorRole::ComplianceOfficer,
            &Vec::new(&s.env),
            &(1_000 + DAY),
        ),
        Err(Ok(Error::Unauthorized))
    );
    // Once the SuperAdmin grant expires, its holder can't grant either.
    at(&s, 1_000 + 10 * DAY + 1);
    assert_eq!(
        s.client().try_grant_scoped_role(
            &super_op,
            &Address::generate(&s.env),
            &OperatorRole::ComplianceOfficer,
            &Vec::new(&s.env),
            &(1_000 + 11 * DAY),
        ),
        Err(Ok(Error::RoleGrantExpired))
    );
}

#[test]
fn test_self_renewal_needs_a_super_admin_quorum() {
    let s = setup();
    at(&s, 1_000);
    let a = Address::generate(&s.env);
    let b = Address::generate(&s.env);
    for who in [&a, &b] {
        s.client().grant_scoped_role(
            &s.admin,
            who,
            &OperatorRole::SuperAdmin,
            &Vec::new(&s.env),
            &(1_000 + 10 * DAY),
        );
    }
    let renew = |approvers: Vec<Address>| {
        s.client().try_renew_role_with_quorum(
            &approvers,
            &a,
            &OperatorRole::SuperAdmin,
            &Vec::new(&s.env),
            &(1_000 + 100 * DAY),
        )
    };
    // The grantee's own approval doesn't count, nor do duplicates or outsiders.
    assert_eq!(
        renew(soroban_sdk::vec![&s.env, a.clone(), b.clone()]),
        Err(Ok(Error::InsufficientRoleQuorum))
    );
    assert_eq!(
        renew(soroban_sdk::vec![&s.env, b.clone(), b.clone(), Address::generate(&s.env)]),
        Err(Ok(Error::InsufficientRoleQuorum))
    );
    // Two other SuperAdmins: renewed.
    assert_eq!(renew(soroban_sdk::vec![&s.env, b.clone(), s.admin.clone()]), Ok(Ok(())));
    assert_eq!(
        s.client().get_role_grant(&a, &OperatorRole::SuperAdmin).unwrap().expires_at,
        1_000 + 100 * DAY
    );
}

#[test]
fn test_revoked_grant_stops_working() {
    let s = setup();
    at(&s, 1_000);
    let operator = Address::generate(&s.env);
    s.client().grant_scoped_role(
        &s.admin,
        &operator,
        &OperatorRole::UpgradeProposer,
        &Vec::new(&s.env),
        &(1_000 + DAY),
    );
    s.client()
        .revoke_scoped_role(&s.admin, &operator, &OperatorRole::UpgradeProposer);
    assert_eq!(
        s.client().try_propose_upgrade(&operator, &s.payment, &hash(&s.env, 1)),
        Err(Ok(Error::RoleGrantNotFound))
    );
    assert_eq!(
        s.client()
            .try_revoke_scoped_role(&s.admin, &operator, &OperatorRole::UpgradeProposer),
        Err(Ok(Error::RoleGrantNotFound))
    );
}
