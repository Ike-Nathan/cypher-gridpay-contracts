#![cfg(test)]

use super::*;
use soroban_sdk::testutils::{storage::Instance as _, Address as _, Events, Ledger};

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

// ── Canary upgrade verification (Issue #235) ────────────────────────────────

mod canary {
    use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, String, Vec};

    #[contracttype]
    enum Key {
        Version,
        Paused,
        Faulty,
        Hash,
    }

    /// Stand-in upgrade target exposing the entry points `execute_upgrade`
    /// drives. A faulty target ignores `migrate_schema`.
    #[contract]
    pub struct MockTarget;

    #[contractimpl]
    impl MockTarget {
        pub fn configure(env: Env, version: u32, paused: bool, faulty: bool) {
            env.storage().instance().set(&Key::Version, &version);
            env.storage().instance().set(&Key::Paused, &paused);
            env.storage().instance().set(&Key::Faulty, &faulty);
        }

        pub fn get_schema_version(env: Env) -> u32 {
            env.storage().instance().get(&Key::Version).unwrap_or(1)
        }

        pub fn get_pause_state(env: Env) -> payments::PauseState {
            payments::PauseState {
                globally_paused: env.storage().instance().get(&Key::Paused).unwrap_or(false),
                paused_functions: Vec::new(&env),
                paused_at: 0,
                paused_by: env.current_contract_address(),
                pause_reason: String::from_str(&env, ""),
            }
        }

        pub fn upgrade(env: Env, _caller: Address, new_wasm_hash: BytesN<32>) -> BytesN<32> {
            env.storage().instance().set(&Key::Hash, &new_wasm_hash);
            new_wasm_hash
        }

        pub fn migrate_schema(env: Env, _caller: Address, target_version: u32) {
            let faulty: bool = env.storage().instance().get(&Key::Faulty).unwrap_or(false);
            if !faulty {
                env.storage().instance().set(&Key::Version, &target_version);
            }
        }

        pub fn installed_hash(env: Env) -> Option<BytesN<32>> {
            env.storage().instance().get(&Key::Hash)
        }
    }
}

fn setup_canary(s: &Setup, version: u32, paused: bool, faulty: bool) -> Address {
    let id = s.env.register(canary::MockTarget, ());
    canary::MockTargetClient::new(&s.env, &id).configure(&version, &paused, &faulty);
    id
}

fn wasm_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[7u8; 32])
}

#[test]
fn test_upgrade_with_migration_verifies_post_schema_version() {
    let s = setup();
    let c = s.client();
    c.configure_upgrade(&s.admin, &0, &1);
    let target = setup_canary(&s, 1, true, false);

    let id = c.propose_upgrade(&s.admin, &target, &wasm_hash(&s.env), &2, &Some(2));
    c.approve_upgrade(&s.admin, &id);
    c.execute_upgrade(&s.admin, &id);

    let mock = canary::MockTargetClient::new(&s.env, &target);
    assert_eq!(mock.get_schema_version(), 2);
    assert_eq!(mock.installed_hash(), Some(wasm_hash(&s.env)));
    assert!(c.get_upgrade_proposal(&id).executed);
}

#[test]
fn test_upgrade_rolls_back_on_post_schema_mismatch() {
    let s = setup();
    let c = s.client();
    c.configure_upgrade(&s.admin, &0, &1);
    let target = setup_canary(&s, 1, true, true);

    let id = c.propose_upgrade(&s.admin, &target, &wasm_hash(&s.env), &2, &Some(2));
    c.approve_upgrade(&s.admin, &id);
    let res = c.try_execute_upgrade(&s.admin, &id);
    assert_eq!(res, Err(Ok(Error::PostUpgradeVerificationFailed)));

    // Upgrade, migration and the executed flag are all rolled back.
    let mock = canary::MockTargetClient::new(&s.env, &target);
    assert_eq!(mock.installed_hash(), None);
    assert_eq!(mock.get_schema_version(), 1);
    assert!(!c.get_upgrade_proposal(&id).executed);
}

#[test]
fn test_migrating_upgrade_requires_paused_target() {
    let s = setup();
    let c = s.client();
    c.configure_upgrade(&s.admin, &0, &1);
    let target = setup_canary(&s, 1, false, false);

    let id = c.propose_upgrade(&s.admin, &target, &wasm_hash(&s.env), &2, &Some(2));
    c.approve_upgrade(&s.admin, &id);
    let res = c.try_execute_upgrade(&s.admin, &id);
    assert_eq!(res, Err(Ok(Error::TargetNotPaused)));
}

#[test]
fn test_expired_upgrade_proposal_cannot_execute() {
    let s = setup();
    let c = s.client();
    c.configure_upgrade(&s.admin, &3_600, &1);
    let target = setup_canary(&s, 1, false, false);

    s.env.ledger().set_timestamp(1_000);
    let id = c.propose_upgrade(&s.admin, &target, &wasm_hash(&s.env), &1, &None);
    c.approve_upgrade(&s.admin, &id);

    let expires_at = c.get_upgrade_proposal(&id).expires_at;
    assert_eq!(expires_at, 1_000 + 3_600 + UPGRADE_PROPOSAL_VALIDITY);

    s.env.ledger().set_timestamp(expires_at + 1);
    let res = c.try_execute_upgrade(&s.admin, &id);
    assert_eq!(res, Err(Ok(Error::UpgradeProposalExpired)));

    // Still executable right up to the expiry.
    s.env.ledger().set_timestamp(expires_at);
    c.execute_upgrade(&s.admin, &id);
}

// ── Protocol parameter guardrails (Issue #240) ──────────────────────────────

fn empty_patch() -> ProtocolParameterPatch {
    ProtocolParameterPatch {
        max_fee_bps: None,
        min_finality_delay_secs: None,
        max_slippage_bps: None,
        min_escrow_hold_secs: None,
    }
}

#[test]
fn test_sync_parameters_rejects_out_of_bounds() {
    let s = setup();
    let c = s.client();

    let mut patch = empty_patch();
    patch.max_fee_bps = Some(9_900);
    assert_eq!(
        c.try_sync_protocol_parameters(&s.admin, &patch),
        Err(Ok(Error::ParameterOutOfBounds))
    );

    let mut patch = empty_patch();
    patch.min_finality_delay_secs = Some(0);
    assert_eq!(
        c.try_sync_protocol_parameters(&s.admin, &patch),
        Err(Ok(Error::ParameterOutOfBounds))
    );

    // Nothing was applied.
    assert_eq!(c.get_protocol_parameters().max_fee_bps, 100);
}

#[test]
fn test_sync_parameters_rejects_large_step() {
    let s = setup();
    let c = s.client();

    // Default fee is 100 bps; a 25% step allows at most 125.
    let mut patch = empty_patch();
    patch.max_fee_bps = Some(126);
    assert_eq!(
        c.try_sync_protocol_parameters(&s.admin, &patch),
        Err(Ok(Error::ParameterStepTooLarge))
    );

    patch.max_fee_bps = Some(125);
    assert_eq!(
        c.sync_protocol_parameters(&s.admin, &patch).max_fee_bps,
        125
    );
}

#[test]
fn test_sync_parameters_enforces_cooldown() {
    let s = setup();
    let c = s.client();
    s.env.ledger().set_timestamp(10_000);

    let mut patch = empty_patch();
    patch.max_slippage_bps = Some(110);
    c.sync_protocol_parameters(&s.admin, &patch);

    patch.max_slippage_bps = Some(120);
    s.env
        .ledger()
        .set_timestamp(10_000 + GlobalParameterGuardrails::MIN_PARAMETER_COOLDOWN_SECS - 1);
    assert_eq!(
        c.try_sync_protocol_parameters(&s.admin, &patch),
        Err(Ok(Error::ParameterCooldownActive))
    );

    // A different parameter is not affected by that cooldown.
    let mut other = empty_patch();
    other.min_escrow_hold_secs = Some(100_000);
    c.sync_protocol_parameters(&s.admin, &other);

    s.env
        .ledger()
        .set_timestamp(10_000 + GlobalParameterGuardrails::MIN_PARAMETER_COOLDOWN_SECS);
    assert_eq!(
        c.sync_protocol_parameters(&s.admin, &patch)
            .max_slippage_bps,
        120
    );
}

#[test]
fn test_sync_parameters_requires_admin_and_broadcasts() {
    let s = setup();
    let c = s.client();

    let mut patch = empty_patch();
    patch.max_fee_bps = Some(110);
    assert_eq!(
        c.try_sync_protocol_parameters(&s.pauser, &patch),
        Err(Ok(Error::Unauthorized))
    );

    c.sync_protocol_parameters(&s.admin, &patch);
    let synced = s
        .env
        .events()
        .all()
        .iter()
        .filter(|e| e.0 == s.contract_id)
        .count();
    assert_eq!(synced, 3);
    assert_eq!(c.get_protocol_parameters().max_fee_bps, 110);
}
