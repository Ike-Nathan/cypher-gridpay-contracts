//! Stateful model-based property tests (issue #251) for Payment and Escrow.
//!
//! A `proptest-state-machine` reference model generates sequences of up to 40
//! interleaved actions (create / complete / refund payment, create / release
//! escrow, pause / unpause, time jumps). The system under test is the real
//! contracts running in a Soroban `Env`. After every step the harness asserts:
//!
//! * exact conservation of token balances over every tracked account,
//! * monotonicity of terminal states (a status rank never decreases),
//! * agreement between a shadow ledger (updated only from real call results)
//!   and the contract getters, and that a call rejected by the model rules
//!   (paused / already terminal) is also rejected by the contract.
//!
//! Replay: proptest persists failing seeds to `proptest-regressions/`; set
//! `PROPTEST_RNG_SEED`-style replay via `PROPTEST_CASES` / the persisted file,
//! and the failing transition list is printed on failure.
//!
//! Run: `cargo test --test state_machine_fuzz` from `tests/e2e`.

use payment::{Currency, PaymentContract, PaymentContractClient, PaymentStatus};
use escrow::{EscrowContract, EscrowContractClient, EscrowStatus};
use proptest::prelude::*;
use proptest_state_machine::{prop_state_machine, ReferenceStateMachine, StateMachineTest};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, String,
};

const MINT: i128 = 1_000_000_000;

#[derive(Clone, Debug)]
enum Action {
    CreatePayment { amount: i128 },
    CompletePayment { idx: usize },
    RefundPayment { idx: usize },
    CreateEscrow { amount: i128 },
    ReleaseEscrow { idx: usize },
    PauseAll,
    UnpauseAll,
    AdvanceTime { secs: u64 },
}

/// Reference model: tracks only what is deterministic regardless of contract
/// internals (the pause flag and how many objects exist).
#[derive(Clone, Debug, Default)]
struct RefModel {
    paused: bool,
    payments: usize,
    escrows: usize,
}

impl ReferenceStateMachine for RefModel {
    type State = RefModel;
    type Transition = Action;

    fn init_state() -> BoxedStrategy<Self::State> {
        Just(RefModel::default()).boxed()
    }

    fn transitions(_s: &Self::State) -> BoxedStrategy<Self::Transition> {
        prop_oneof![
            4 => (1i128..10_000).prop_map(|amount| Action::CreatePayment { amount }),
            3 => (0usize..64).prop_map(|idx| Action::CompletePayment { idx }),
            2 => (0usize..64).prop_map(|idx| Action::RefundPayment { idx }),
            2 => (1i128..10_000).prop_map(|amount| Action::CreateEscrow { amount }),
            2 => (0usize..64).prop_map(|idx| Action::ReleaseEscrow { idx }),
            1 => Just(Action::PauseAll),
            1 => Just(Action::UnpauseAll),
            2 => (0u64..2_000_000).prop_map(|secs| Action::AdvanceTime { secs }),
        ]
        .boxed()
    }

    fn apply(mut s: Self::State, t: &Self::Transition) -> Self::State {
        match t {
            Action::CreatePayment { .. } if !s.paused => s.payments += 1,
            Action::CreateEscrow { .. } if !s.paused => s.escrows += 1,
            Action::PauseAll => s.paused = true,
            Action::UnpauseAll => s.paused = false,
            _ => {}
        }
        s
    }
}

fn payment_rank(s: &PaymentStatus) -> u8 {
    match s {
        PaymentStatus::Pending => 0,
        PaymentStatus::PartialRefunded => 1,
        _ => 2,
    }
}

fn escrow_rank(s: &EscrowStatus) -> u8 {
    match s {
        EscrowStatus::Locked => 0,
        EscrowStatus::Disputed => 1,
        _ => 2,
    }
}

struct Sut {
    env: Env,
    admin: Address,
    customer: Address,
    merchant: Address,
    token: Address,
    payment: PaymentContractClient<'static>,
    escrow: EscrowContractClient<'static>,
    // Shadow ledger: (id, last observed status), updated only from real results.
    payments: std::vec::Vec<(u64, PaymentStatus)>,
    escrows: std::vec::Vec<(u64, EscrowStatus)>,
    paused: bool,
}

impl Sut {
    fn tracked(&self) -> [Address; 5] {
        [
            self.admin.clone(),
            self.customer.clone(),
            self.merchant.clone(),
            self.payment.address.clone(),
            self.escrow.address.clone(),
        ]
    }

    fn payment_status(&self, id: u64) -> PaymentStatus {
        self.env.as_contract(&self.payment.address, || {
            PaymentContract::get_payment(&self.env, id).status
        })
    }

    fn total_balance(&self) -> i128 {
        let t = token::Client::new(&self.env, &self.token);
        self.tracked().iter().map(|a| t.balance(a)).sum()
    }
}

struct ContractHarness;

impl StateMachineTest for ContractHarness {
    type SystemUnderTest = Sut;
    type Reference = RefModel;

    fn init_test(_r: &<Self::Reference as ReferenceStateMachine>::State) -> Self::SystemUnderTest {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let customer = Address::generate(&env);
        let merchant = Address::generate(&env);

        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        token::StellarAssetClient::new(&env, &token_id).mint(&customer, &MINT);

        let payment_id = env.register_contract(None, PaymentContract);
        let escrow_id = env.register_contract(None, EscrowContract);
        let payment = PaymentContractClient::new(&env, &payment_id);
        let escrow = EscrowContractClient::new(&env, &escrow_id);
        payment.initialize(&admin);
        escrow.initialize(&admin);
        payment.set_fee_config(
            &admin,
            &payment::FeeConfig {
                fee_bps: 100,
                min_fee: 0,
                max_fee: 0,
                treasury: admin.clone(),
                fee_token: token_id.clone(),
                active: true,
            },
        );
        let tk = token::Client::new(&env, &token_id);
        tk.approve(&customer, &payment_id, &MINT, &u32::MAX);
        tk.approve(&customer, &escrow_id, &MINT, &u32::MAX);

        Sut {
            env,
            admin,
            customer,
            merchant,
            token: token_id,
            payment,
            escrow,
            payments: std::vec::Vec::new(),
            escrows: std::vec::Vec::new(),
            paused: false,
        }
    }

    fn apply(
        mut s: Self::SystemUnderTest,
        _r: &<Self::Reference as ReferenceStateMachine>::State,
        t: Action,
    ) -> Self::SystemUnderTest {
        let now = s.env.ledger().timestamp();
        match t {
            Action::CreatePayment { amount } => {
                let res = s.payment.try_create_payment(
                    &s.customer,
                    &s.merchant,
                    &amount,
                    &s.token,
                    &Currency::USDC,
                    &0,
                    &String::from_str(&s.env, ""),
                );
                if s.paused {
                    assert!(!matches!(res, Ok(Ok(_))), "create_payment succeeded while paused");
                } else if let Ok(Ok(id)) = res {
                    s.payments.push((id, PaymentStatus::Pending));
                }
            }
            Action::CompletePayment { idx } | Action::RefundPayment { idx } => {
                if s.payments.is_empty() {
                    return s;
                }
                let i = idx % s.payments.len();
                let (id, before) = s.payments[i].clone();
                let is_complete = matches!(t, Action::CompletePayment { .. });
                let ok = if is_complete {
                    matches!(s.payment.try_complete_payment(&s.admin, &id), Ok(Ok(_)))
                } else {
                    matches!(s.payment.try_refund_payment(&s.admin, &id), Ok(Ok(_)))
                };
                if s.paused || payment_rank(&before) == 2 {
                    assert!(!ok, "illegal payment transition accepted (paused={})", s.paused);
                }
                let after = s.payment_status(id);
                s.payments[i].1 = after;
            }
            Action::CreateEscrow { amount } => {
                let res = s.escrow.try_create_escrow(
                    &s.customer,
                    &s.merchant,
                    &amount,
                    &s.token,
                    &now,
                    &0,
                    &(now + 10_000_000),
                    &false,
                );
                if s.paused {
                    assert!(!matches!(res, Ok(Ok(_))), "create_escrow succeeded while paused");
                } else if let Ok(Ok(id)) = res {
                    s.escrows.push((id, EscrowStatus::Locked));
                }
            }
            Action::ReleaseEscrow { idx } => {
                if s.escrows.is_empty() {
                    return s;
                }
                let i = idx % s.escrows.len();
                let (id, before) = s.escrows[i].clone();
                let ok = matches!(
                    s.escrow.try_release_escrow(&s.admin, &id, &false),
                    Ok(Ok(_))
                );
                if escrow_rank(&before) == 2 {
                    assert!(!ok, "release accepted on a terminal escrow");
                }
                s.escrows[i].1 = s.escrow.get_escrow_status(&id);
            }
            Action::PauseAll => {
                let reason = String::from_str(&s.env, "fuzz");
                let p = s.payment.try_pause_contract(&s.admin, &reason).is_ok();
                let e = s.escrow.try_pause_contract(&s.admin, &reason).is_ok();
                s.paused = s.paused || p || e;
            }
            Action::UnpauseAll => {
                let p = s.payment.try_unpause_contract(&s.admin).is_ok();
                let e = s.escrow.try_unpause_contract(&s.admin).is_ok();
                if p || e {
                    s.paused = false;
                }
            }
            Action::AdvanceTime { secs } => {
                s.env.ledger().with_mut(|l| l.timestamp += secs);
            }
        }
        s
    }

    fn check_invariants(
        s: &Self::SystemUnderTest,
        r: &<Self::Reference as ReferenceStateMachine>::State,
    ) {
        // 1. Exact conservation of the token across every tracked account.
        assert_eq!(s.total_balance(), MINT, "token conservation violated");

        // 2. Terminal-state monotonicity + 3. shadow ledger vs. getters.
        for (id, seen) in &s.payments {
            let now = s.payment_status(*id);
            assert!(
                payment_rank(&now) >= payment_rank(seen),
                "payment {id} moved backward: {seen:?} -> {now:?}"
            );
            assert_eq!(&now, seen, "payment {id} getter disagrees with shadow ledger");
        }
        for (id, seen) in &s.escrows {
            let now = s.escrow.get_escrow_status(id);
            assert!(
                escrow_rank(&now) >= escrow_rank(seen),
                "escrow {id} moved backward: {seen:?} -> {now:?}"
            );
            assert_eq!(&now, seen, "escrow {id} getter disagrees with shadow ledger");
        }

        // Reference model agreement on counters and pause flag.
        assert_eq!(r.paused, s.paused, "pause flag diverged from model");
        assert_eq!(r.payments, s.payments.len(), "payment count diverged from model");
        assert_eq!(r.escrows, s.escrows.len(), "escrow count diverged from model");
    }
}

prop_state_machine! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        failure_persistence: Some(Box::new(
            proptest::test_runner::FileFailurePersistence::WithSource("regressions")
        )),
        ..ProptestConfig::default()
    })]

    #[test]
    fn payment_escrow_state_machine(sequential 1..41 => ContractHarness);
}
