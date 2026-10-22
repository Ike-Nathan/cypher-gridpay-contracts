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

// ── harvest_all_protocol_fees ────────────────────────────────────────────────

struct Harvest {
    env: Env,
    contract_id: Address,
    admin: Address,
    payment: Address,
    escrow: Address,
    refund: Address,
    token: Address,
    recipients: TreasuryRecipients,
}

impl Harvest {
    fn client(&self) -> AdminContractClient<'_> {
        AdminContractClient::new(&self.env, &self.contract_id)
    }

    fn balance(&self, token: &Address, who: &Address) -> i128 {
        soroban_sdk::token::Client::new(&self.env, token).balance(who)
    }

    /// Credits `amount` of protocol fees to each child's accumulator and gives
    /// the child the tokens backing it, as fee collection would.
    fn accrue(&self, token: &Address, payment: i128, escrow: i128, refund: i128) {
        let mint = soroban_sdk::token::StellarAssetClient::new(&self.env, token);
        if payment > 0 {
            mint.mint(&self.payment, &payment);
            self.env.as_contract(&self.payment, || {
                self.env.storage().instance().set(
                    &payments::DataKey::Payment(payments::PaymentKey::AccumulatedFees),
                    &payment,
                );
            });
        }
        if escrow > 0 {
            mint.mint(&self.escrow, &escrow);
            self.env.as_contract(&self.escrow, || {
                self.env.storage().instance().set(
                    &escrow::DataKey::Participant(escrow::ParticipantKey::AccumulatedFees(
                        token.clone(),
                    )),
                    &escrow,
                );
            });
        }
        if refund > 0 {
            mint.mint(&self.refund, &refund);
            self.env.as_contract(&self.refund, || {
                self.env
                    .storage()
                    .instance()
                    .set(&refund::ConfigKey::AccumulatedFees(token.clone()), &refund);
            });
        }
    }
}

fn new_token(env: &Env) -> Address {
    env.register_stellar_asset_contract_v2(Address::generate(env))
        .address()
}

/// Admin contract that administers all three children, the payment contract
/// sweeping its fee token to the admin contract, and a 25/15/50/10 split.
fn harvest_setup() -> Harvest {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(AdminContract, ());
    let admin = Address::generate(&env);
    let pauser = Address::generate(&env);
    // The admin contract is the children's admin, so it can sweep them.
    let payment = setup_payment(&env, &contract_id);
    let escrow = setup_escrow(&env, &contract_id);
    let refund = setup_refund(&env, &contract_id);
    let token = new_token(&env);

    let payment_client = PaymentContractClient::new(&env, &payment);
    payment_client.set_fee_config(
        &contract_id,
        &payments::FeeConfig {
            fee_bps: 100,
            min_fee: 0,
            max_fee: 0,
            treasury: Address::generate(&env),
            fee_token: token.clone(),
            active: true,
        },
    );
    payment_client.set_sweep_recipient(&contract_id, &contract_id);

    let client = AdminContractClient::new(&env, &contract_id);
    client.initialize(&admin, &pauser, &payment, &escrow, &refund);
    let recipients = TreasuryRecipients {
        insurance_pool: Address::generate(&env),
        arbitrator_pool: Address::generate(&env),
        protocol_treasury: Address::generate(&env),
        operations: Address::generate(&env),
    };
    client.set_treasury_allocation(
        &admin,
        &TreasuryAllocationConfig {
            insurance_pool_bps: 2_500,
            arbitrator_incentive_bps: 1_500,
            protocol_treasury_bps: 5_000,
            operations_bps: 1_000,
        },
        &recipients,
    );

    Harvest {
        env,
        contract_id,
        admin,
        payment,
        escrow,
        refund,
        token,
        recipients,
    }
}

#[test]
fn test_harvest_collects_all_three_and_splits_by_bps() {
    let h = harvest_setup();
    h.accrue(&h.token, 10_000, 5_000, 3_333);

    let reports = h
        .client()
        .harvest_all_protocol_fees(&h.admin, &soroban_sdk::vec![&h.env, h.token.clone()]);

    assert_eq!(reports.len(), 1);
    let r = reports.get(0).unwrap();
    assert_eq!((r.from_payment, r.from_escrow, r.from_refund), (10_000, 5_000, 3_333));
    assert_eq!(r.total, 18_333);
    // 25% / 15% / 10% rounded down; the treasury takes 50% plus the remainder.
    assert_eq!(r.insurance_pool, 4_583);
    assert_eq!(r.arbitrator_incentive, 2_749);
    assert_eq!(r.operations, 1_833);
    assert_eq!(r.protocol_treasury, 9_168);

    // Conservation: everything harvested was paid out, nothing left behind.
    let recipients = &h.recipients;
    let paid = h.balance(&h.token, &recipients.insurance_pool)
        + h.balance(&h.token, &recipients.arbitrator_pool)
        + h.balance(&h.token, &recipients.protocol_treasury)
        + h.balance(&h.token, &recipients.operations);
    assert_eq!(paid, r.total);
    assert_eq!(h.balance(&h.token, &recipients.protocol_treasury), 9_168);
    assert_eq!(h.balance(&h.token, &h.contract_id), 0);
    for child in [&h.payment, &h.escrow, &h.refund] {
        assert_eq!(h.balance(&h.token, child), 0);
    }

    // The children's accumulators are cleared.
    assert_eq!(PaymentContractClient::new(&h.env, &h.payment).get_accumulated_fees(), 0);
    assert_eq!(
        EscrowContractClient::new(&h.env, &h.escrow).get_accumulated_escrow_fees(&h.token),
        0
    );
    assert_eq!(
        RefundContractClient::new(&h.env, &h.refund).get_accumulated_protocol_fees(&h.token),
        0
    );
}

#[test]
fn test_harvest_skips_a_contract_with_no_fees() {
    let h = harvest_setup();
    // Escrow has nothing accrued; payment and refund still get harvested.
    h.accrue(&h.token, 4_000, 0, 1_000);

    let reports = h
        .client()
        .harvest_all_protocol_fees(&h.admin, &soroban_sdk::vec![&h.env, h.token.clone()]);
    let r = reports.get(0).unwrap();
    assert_eq!((r.from_payment, r.from_escrow, r.from_refund), (4_000, 0, 1_000));
    assert_eq!(r.total, 5_000);
    assert_eq!(
        r.insurance_pool + r.arbitrator_incentive + r.protocol_treasury + r.operations,
        5_000
    );
}

#[test]
fn test_harvest_handles_several_tokens_and_empty_ones() {
    let h = harvest_setup();
    let other = new_token(&h.env);
    let empty = new_token(&h.env);
    h.accrue(&h.token, 1_000, 0, 0);
    // A second token only accrues in escrow and refund: the payment contract's
    // fee token is `h.token`, so it is not swept for `other`.
    h.accrue(&other, 0, 700, 300);

    let reports = h.client().harvest_all_protocol_fees(
        &h.admin,
        &soroban_sdk::vec![&h.env, h.token.clone(), empty.clone(), other.clone()],
    );

    // `empty` had nothing anywhere, so it has no report and moved no funds.
    assert_eq!(reports.len(), 2);
    let first = reports.get(0).unwrap();
    assert_eq!((first.token.clone(), first.total), (h.token.clone(), 1_000));
    let second = reports.get(1).unwrap();
    assert_eq!(second.token, other);
    assert_eq!((second.from_payment, second.from_escrow, second.from_refund), (0, 700, 300));
    assert_eq!(h.balance(&other, &h.recipients.protocol_treasury), 500);
    assert_eq!(h.balance(&empty, &h.recipients.protocol_treasury), 0);
}

#[test]
fn test_harvest_with_nothing_accrued_returns_no_reports() {
    let h = harvest_setup();
    let reports = h
        .client()
        .harvest_all_protocol_fees(&h.admin, &soroban_sdk::vec![&h.env, h.token.clone()]);
    assert_eq!(reports.len(), 0);
}

#[test]
fn test_treasury_allocation_must_sum_to_10000() {
    let h = harvest_setup();
    for (a, b, c, d) in [(2_500, 1_500, 5_000, 999), (2_500, 1_500, 5_000, 1_001), (0, 0, 0, 0)] {
        let result = h.client().try_set_treasury_allocation(
            &h.admin,
            &TreasuryAllocationConfig {
                insurance_pool_bps: a,
                arbitrator_incentive_bps: b,
                protocol_treasury_bps: c,
                operations_bps: d,
            },
            &h.recipients,
        );
        assert_eq!(result, Err(Ok(Error::InvalidTreasuryAllocation)));
    }
    // All to one bucket is fine.
    h.client().set_treasury_allocation(
        &h.admin,
        &TreasuryAllocationConfig {
            insurance_pool_bps: 0,
            arbitrator_incentive_bps: 0,
            protocol_treasury_bps: 10_000,
            operations_bps: 0,
        },
        &h.recipients,
    );
}

#[test]
fn test_harvest_requires_admin_and_an_allocation() {
    let h = harvest_setup();
    let stranger = Address::generate(&h.env);
    assert_eq!(
        h.client()
            .try_harvest_all_protocol_fees(&stranger, &soroban_sdk::vec![&h.env, h.token.clone()]),
        Err(Ok(Error::Unauthorized))
    );

    // A fresh admin contract with no allocation configured.
    let env = &h.env;
    let fresh_id = env.register(AdminContract, ());
    let fresh = AdminContractClient::new(env, &fresh_id);
    fresh.initialize(&h.admin, &h.admin, &h.payment, &h.escrow, &h.refund);
    assert_eq!(
        fresh.try_harvest_all_protocol_fees(&h.admin, &soroban_sdk::vec![env, h.token.clone()]),
        Err(Ok(Error::TreasuryAllocationNotSet))
    );
}

#[test]
fn test_harvest_detects_fees_swept_elsewhere() {
    let h = harvest_setup();
    // Point the payment contract's sweep recipient somewhere else: the sweep
    // succeeds, but the fees never reach the admin contract.
    PaymentContractClient::new(&h.env, &h.payment)
        .set_sweep_recipient(&h.contract_id, &Address::generate(&h.env));
    h.accrue(&h.token, 1_000, 0, 0);

    assert_eq!(
        h.client()
            .try_harvest_all_protocol_fees(&h.admin, &soroban_sdk::vec![&h.env, h.token.clone()]),
        Err(Ok(Error::FeeHarvestNotReceived))
    );
}

// ── Instance-storage budget ──────────────────────────────────────────────────

/// Instance storage must stay below this many serialized bytes (it is loaded
/// on every invocation); see core/contracts/*/src/storage.rs.
const MAX_INSTANCE_STORAGE_BYTES: u32 = 4_096;

fn instance_storage_xdr_len(s: &Setup) -> u32 {
    use soroban_sdk::xdr::ToXdr;
    s.env.as_contract(&s.contract_id, || {
        s.env.storage().instance().all().to_xdr(&s.env).len()
    })
}

#[test]
fn test_upgrade_proposals_keep_instance_storage_under_budget() {
    let s = setup();
    for i in 0..40u8 {
        let hash = BytesN::<32>::from_array(&s.env, &[i + 1; 32]);
        s.client().propose_upgrade(&s.admin, &s.contract_id, &hash);
    }
    let len = instance_storage_xdr_len(&s);
    assert!(len < MAX_INSTANCE_STORAGE_BYTES, "instance storage is {len} bytes");

    // Proposals are persistent and still readable.
    assert_eq!(
        s.client().get_upgrade_proposal(&39).new_wasm_hash,
        BytesN::<32>::from_array(&s.env, &[40; 32])
    );
    s.env.as_contract(&s.contract_id, || {
        assert!(s.env.storage().persistent().has(&DataKey::UpgradeProposal(0)));
        assert!(!s.env.storage().instance().has(&DataKey::UpgradeProposal(0)));
    });
}

#[test]
fn test_migrate_schema_moves_legacy_upgrade_proposals() {
    let s = setup();
    // Pre-schema-3 layout: every proposal in instance storage.
    s.env.as_contract(&s.contract_id, || {
        for i in 0..40u64 {
            let proposal = UpgradeProposal {
                contract_address: s.contract_id.clone(),
                new_wasm_hash: BytesN::from_array(&s.env, &[i as u8 + 1; 32]),
                proposed_at: 0,
                timelock_period: 0,
                approvals: Vec::new(&s.env),
                executed: false,
            };
            s.env
                .storage()
                .instance()
                .set(&DataKey::UpgradeProposal(i), &proposal);
        }
        s.env
            .storage()
            .instance()
            .set(&DataKey::UpgradeProposalCounter, &40u64);
    });
    assert!(instance_storage_xdr_len(&s) > MAX_INSTANCE_STORAGE_BYTES);
    // Readable before migrating.
    assert!(!s.client().get_upgrade_proposal(&7).executed);

    s.client().migrate_schema(&s.admin, &3);

    assert!(instance_storage_xdr_len(&s) < MAX_INSTANCE_STORAGE_BYTES);
    assert_eq!(
        s.client().get_upgrade_proposal(&7).new_wasm_hash,
        BytesN::<32>::from_array(&s.env, &[8; 32])
    );
    assert_eq!(s.client().get_schema_version(), 3);
}
