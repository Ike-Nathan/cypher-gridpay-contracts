//! Differential fuzzing: native Rust execution vs. compiled WASM (issue #253).
//!
//! Two parallel Soroban environments are built with an identical setup order, so
//! generated addresses and contract ids line up:
//!   * `native` registers the contracts with `register_contract(None, T)`,
//!   * `wasm`   registers the compiled bytecode with `register_contract_wasm`.
//! The same randomized call sequence (including i128 boundary values) is fed to
//! both; per-call results (values or error codes), the emitted events, and
//! post-state records must be identical.
//!
//! WASM location: `$GRIDPAY_WASM_DIR` (default `../../core/target/wasm32v1-none/release`,
//! as produced by `make build` / `stellar contract build`). If the files are
//! missing the tests skip, unless `REQUIRE_WASM=1` (set by `make differential`),
//! in which case they fail.
//!
//! Run: `make differential` from the repo root, or
//! `cargo test --test differential_wasm` from `tests/e2e`.

use escrow::{EscrowContract, EscrowContractClient};
use payment::{Currency, PaymentContract, PaymentContractClient};
use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, token, Address, Bytes, Env, String};
use std::{format, path::PathBuf, string::String as StdString, vec::Vec as StdVec};

fn wasm_bytes(name: &str) -> Option<StdVec<u8>> {
    let dir = std::env::var("GRIDPAY_WASM_DIR")
        .unwrap_or_else(|_| "../../core/target/wasm32v1-none/release".into());
    let path = PathBuf::from(dir).join(format!("{name}.wasm"));
    match std::fs::read(&path) {
        Ok(b) => Some(b),
        Err(_) => {
            assert!(
                std::env::var("REQUIRE_WASM").map(|v| v != "1").unwrap_or(true),
                "REQUIRE_WASM=1 but {} is missing; run `make build` first",
                path.display()
            );
            None
        }
    }
}

#[derive(Clone, Debug)]
enum Op {
    Create { amount: i128, expiry: u64, meta_len: usize },
    Complete { idx: usize },
    Refund { idx: usize },
    EscrowCreate { amount: i128 },
    EscrowRelease { idx: usize },
    Pause,
    Unpause,
}

fn amount() -> impl Strategy<Value = i128> {
    prop_oneof![
        3 => 1i128..1_000_000,
        1 => Just(0i128),
        1 => Just(-1i128),
        1 => Just(i128::MAX),
        1 => Just(i128::MIN),
        1 => Just(i128::MAX / 10_000),
    ]
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (amount(), prop_oneof![Just(0u64), Just(1u64), Just(u64::MAX)], 0usize..256)
            .prop_map(|(amount, expiry, meta_len)| Op::Create { amount, expiry, meta_len }),
        2 => (0usize..8).prop_map(|idx| Op::Complete { idx }),
        2 => (0usize..8).prop_map(|idx| Op::Refund { idx }),
        2 => amount().prop_map(|amount| Op::EscrowCreate { amount }),
        2 => (0usize..8).prop_map(|idx| Op::EscrowRelease { idx }),
        1 => Just(Op::Pause),
        1 => Just(Op::Unpause),
    ]
}

struct World {
    env: Env,
    admin: Address,
    customer: Address,
    merchant: Address,
    token: Address,
    payment: PaymentContractClient<'static>,
    escrow: EscrowContractClient<'static>,
}

/// `wasm = Some((payment_wasm, escrow_wasm))` selects WASM registration.
fn world(wasm: Option<(&[u8], &[u8])>) -> World {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(admin.clone()).address();
    token::StellarAssetClient::new(&env, &token).mint(&customer, &i128::MAX);

    let (p, e) = match wasm {
        None => (
            env.register_contract(None, PaymentContract),
            env.register_contract(None, EscrowContract),
        ),
        Some((pw, ew)) => (
            env.register_contract_wasm(None, Bytes::from_slice(&env, pw)),
            env.register_contract_wasm(None, Bytes::from_slice(&env, ew)),
        ),
    };
    let payment = PaymentContractClient::new(&env, &p);
    let escrow = EscrowContractClient::new(&env, &e);
    payment.initialize(&admin);
    escrow.initialize(&admin);
    let tk = token::Client::new(&env, &token);
    tk.approve(&customer, &p, &i128::MAX, &u32::MAX);
    tk.approve(&customer, &e, &i128::MAX, &u32::MAX);
    World { env, admin, customer, merchant, token, payment, escrow }
}

/// Executes `ops`, returning a transcript of every observable result.
fn run(w: &World, ops: &[Op]) -> StdVec<StdString> {
    let mut log = StdVec::new();
    let (mut pids, mut eids) = (StdVec::new(), StdVec::new());
    let now = w.env.ledger().timestamp();
    for o in ops {
        match o {
            Op::Create { amount, expiry, meta_len } => {
                let meta = String::from_str(&w.env, &"x".repeat(*meta_len));
                let r = w.payment.try_create_payment(
                    &w.customer, &w.merchant, amount, &w.token, &Currency::USDC, expiry, &meta,
                );
                if let Ok(Ok(id)) = &r {
                    pids.push(*id);
                }
                log.push(format!("{o:?} => {r:?}"));
            }
            Op::Complete { idx } if !pids.is_empty() => {
                let id = pids[idx % pids.len()];
                log.push(format!("{o:?} => {:?}", w.payment.try_complete_payment(&w.admin, &id)));
            }
            Op::Refund { idx } if !pids.is_empty() => {
                let id = pids[idx % pids.len()];
                log.push(format!("{o:?} => {:?}", w.payment.try_refund_payment(&w.admin, &id)));
            }
            Op::EscrowCreate { amount } => {
                let r = w.escrow.try_create_escrow(
                    &w.customer, &w.merchant, amount, &w.token, &now, &0, &(now + 1_000), &false,
                );
                if let Ok(Ok(id)) = &r {
                    eids.push(*id);
                }
                log.push(format!("{o:?} => {r:?}"));
            }
            Op::EscrowRelease { idx } if !eids.is_empty() => {
                let id = eids[idx % eids.len()];
                log.push(format!("{o:?} => {:?}", w.escrow.try_release_escrow(&w.admin, &id, &false)));
            }
            Op::Pause => {
                let reason = String::from_str(&w.env, "diff");
                log.push(format!("{o:?} => {:?} {:?}",
                    w.payment.try_pause_contract(&w.admin, &reason),
                    w.escrow.try_pause_contract(&w.admin, &reason)));
            }
            Op::Unpause => log.push(format!("{o:?} => {:?} {:?}",
                w.payment.try_unpause_contract(&w.admin),
                w.escrow.try_unpause_contract(&w.admin))),
            _ => {}
        }
        // Post-state queries after every step.
        for id in &pids {
            let rec = w.env.as_contract(&w.payment.address, || {
                PaymentContract::get_payment(&w.env, *id)
            });
            log.push(format!("payment {id}: {rec:?}"));
        }
        for id in &eids {
            log.push(format!("escrow {id}: {:?}", w.escrow.try_get_escrow_status(id)));
        }
        log.push(format!("events: {:?}", w.env.events().all().events()));
    }
    log
}

fn load() -> Option<(StdVec<u8>, StdVec<u8>)> {
    Some((wasm_bytes("payments")?, wasm_bytes("escrow")?))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn native_and_wasm_agree(ops in proptest::collection::vec(op(), 1..20)) {
        let Some((pw, ew)) = load() else { return Ok(()); };
        let native = run(&world(None), &ops);
        let wasm = run(&world(Some((&pw, &ew))), &ops);
        prop_assert_eq!(native, wasm);
    }
}

/// Fixed boundary sweep so the extreme values are always covered, independent of
/// proptest's sampling.
#[test]
fn boundary_amounts_agree() {
    let Some((pw, ew)) = load() else { return; };
    let ops: StdVec<Op> = [0i128, 1, -1, i128::MAX, i128::MIN, i128::MAX / 10_000]
        .into_iter()
        .flat_map(|a| {
            [
                Op::Create { amount: a, expiry: 0, meta_len: 0 },
                Op::Complete { idx: 0 },
                Op::EscrowCreate { amount: a },
                Op::EscrowRelease { idx: 0 },
            ]
        })
        .collect();
    assert_eq!(run(&world(None), &ops), run(&world(Some((&pw, &ew))), &ops));
}
