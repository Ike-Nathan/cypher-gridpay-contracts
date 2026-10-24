//! Cross-workspace end-to-end integration tests.
//!
//! This crate wires the payment, escrow, dispute, and emergency-pause
//! contracts into a single test environment and exercises realistic flows:
//! Payment -> Escrow -> Dispute -> Refund, plus emergency pause coverage.

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};

/// Shared test environment registering all four contracts.
struct E2eEnv {
    env: Env,
    payment: Address,
    escrow: Address,
    dispute: Address,
    emergency_pause: Address,
    admin: Address,
    payer: Address,
    payee: Address,
}

impl E2eEnv {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);

        let payment = env.register_contract(None, payment::PaymentContract);
        let escrow = env.register_contract(None, escrow::EscrowContract);
        let dispute = env.register_contract(None, dispute::DisputeContract);
        let emergency_pause =
            env.register_contract(None, emergency_pause::EmergencyPauseContract);

        Self {
            env,
            payment,
            escrow,
            dispute,
            emergency_pause,
            admin,
            payer,
            payee,
        }
    }
}

#[test]
fn full_payment_escrow_dispute_refund_flow() {
    let t = E2eEnv::new();

    // 1. Payment: deposit funds from payer to payee.
    let amount: i128 = 1_000;
    let payment_client = payment::PaymentContractClient::new(&t.env, &t.payment);
    payment_client.deposit(&t.payer, &t.payee, &amount);

    // 2. Escrow: lock the deposited funds under an escrow agreement.
    let escrow_client = escrow::EscrowContractClient::new(&t.env, &t.escrow);
    let escrow_id = escrow_client.create_escrow(&t.payer, &t.payee, &amount);
    escrow_client.fund_escrow(&escrow_id, &t.payer);

    // 3. Dispute: raise a dispute against the escrow.
    let dispute_client = dispute::DisputeContractClient::new(&t.env, &t.dispute);
    let dispute_id = dispute_client.create_dispute(&t.payer, &escrow_id);
    dispute_client.resolve_dispute(&dispute_id, &t.admin, &true);

    // 4. Refund: execute the refund back to the payer.
    escrow_client.refund(&escrow_id, &t.payer);

    let escrow_state = escrow_client.get_escrow(&escrow_id);
    assert_eq!(escrow_state.amount, amount);
    assert!(escrow_state.refunded);
}

#[test]
fn emergency_pause_blocks_escrow_flow() {
    let t = E2eEnv::new();

    let pause_client =
        emergency_pause::EmergencyPauseContractClient::new(&t.env, &t.emergency_pause);
    pause_client.pause(&t.admin);
    assert!(pause_client.is_paused());

    let escrow_client = escrow::EscrowContractClient::new(&t.env, &t.escrow);
    let result = escrow_client.try_create_escrow(&t.payer, &t.payee, &500);
    assert!(result.is_err());

    pause_client.unpause(&t.admin);
    assert!(!pause_client.is_paused());
}
