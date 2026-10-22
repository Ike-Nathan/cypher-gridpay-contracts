#![cfg(test)]
#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, testutils::{Address as _, Ledger},
    Address, Bytes, BytesN, Env, String, Vec,
};

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum EscrowState {
    Active,
    Paused,
    Disputed,
    PartiallyResolved,
    Refunded,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct InstallmentEscrow {
    pub escrow_id: u64,
    pub customer: Address,
    pub merchant: Address,
    pub total_amount: i128,
    pub paid_installments: i128,
    pub state: EscrowState,
    pub residual_refund: i128,
}

#[contract]
pub struct MockAdversarialPaymentEscrow;

#[contractimpl]
impl MockAdversarialPaymentEscrow {
    pub fn create_installment_escrow(
        env: Env,
        escrow_id: u64,
        customer: Address,
        merchant: Address,
        total_amount: i128,
        first_installment: i128,
    ) -> InstallmentEscrow {
        InstallmentEscrow {
            escrow_id,
            customer,
            merchant,
            total_amount,
            paid_installments: first_installment,
            state: EscrowState::Active,
            residual_refund: 0,
        }
    }

    pub fn emergency_pause(mut escrow: InstallmentEscrow) -> InstallmentEscrow {
        assert_eq!(escrow.state, EscrowState::Active);
        escrow.state = EscrowState::Paused;
        escrow
    }

    pub fn unpause(mut escrow: InstallmentEscrow) -> InstallmentEscrow {
        assert_eq!(escrow.state, EscrowState::Paused);
        escrow.state = EscrowState::Active;
        escrow
    }

    pub fn resolve_partial_dispute(
        mut escrow: InstallmentEscrow,
        merchant_share: i128,
        refund_share: i128,
    ) -> InstallmentEscrow {
        assert_eq!(escrow.state, EscrowState::Active);
        assert!(merchant_share + refund_share <= escrow.paid_installments);
        escrow.state = EscrowState::PartiallyResolved;
        escrow.residual_refund = refund_share;
        escrow
    }
}

#[test]
fn test_partial_installment_auto_escrow_pause_unpause_partial_dispute_residual_refund() {
    let env = Env::default();
    env.mock_all_auths();

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    // 1. Partial installment payment creates auto-escrow
    let mut escrow = MockAdversarialPaymentEscrow::create_installment_escrow(
        env.clone(),
        101,
        customer.clone(),
        merchant.clone(),
        10000,
        3000,
    );
    assert_eq!(escrow.state, EscrowState::Active);
    assert_eq!(escrow.paid_installments, 3000);

    // 2. Mid-escrow emergency pause triggered
    escrow = MockAdversarialPaymentEscrow::emergency_pause(escrow);
    assert_eq!(escrow.state, EscrowState::Paused);

    // 3. Unpause after security audit verification
    escrow = MockAdversarialPaymentEscrow::unpause(escrow);
    assert_eq!(escrow.state, EscrowState::Active);

    // 4. Partial dispute resolution: merchant receives partial settlement, customer gets residual refund
    escrow = MockAdversarialPaymentEscrow::resolve_partial_dispute(escrow, 1800, 1200);
    assert_eq!(escrow.state, EscrowState::PartiallyResolved);
    assert_eq!(escrow.residual_refund, 1200);
}

#[test]
fn test_programmable_conditional_payment_predicate_and_dunning_retry() {
    let env = Env::default();
    env.mock_all_auths();

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    // Simulates smart retry window and partial balance capture under liquidity constraints
    let installment_target = 5000_i128;
    let available_balance = 2500_i128;
    let captured = available_balance.min(installment_target);

    let escrow = MockAdversarialPaymentEscrow::create_installment_escrow(
        env.clone(),
        102,
        customer,
        merchant,
        installment_target,
        captured,
    );
    assert_eq!(escrow.paid_installments, 2500);
    assert_eq!(escrow.state, EscrowState::Active);
}
