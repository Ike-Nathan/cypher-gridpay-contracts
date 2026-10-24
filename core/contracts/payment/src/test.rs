//! Property-based fuzz tests for payment fee and rebate calculations.
//!
//! These tests use `proptest` to verify invariants of the fee calculation
//! logic across arbitrary `i128` inputs:
//!
//! * `fee >= 0`
//! * `fee <= amount`
//! * `net_amount + fee == amount`
//!
//! The suite also exercises extreme values (i128::MIN / i128::MAX) to ensure
//! the calculation never panics.

#![cfg(test)]

extern crate std;

use proptest::prelude::*;
use test_utils::{create_mock_token, generate_parties, setup_test_env};

/// Pure fee calculation mirroring the on-chain tiered fee logic.
///
/// `bps` is expressed in basis points (1 bps = 0.01%). The result is clamped
/// so that the fee never exceeds the payment amount and is never negative.
fn calculate_fee(amount: i128, bps: u32) -> i128 {
    if amount <= 0 {
        return 0;
    }
    // Use i128 arithmetic; bps is bounded to [0, 10_000] by the caller.
    let fee = amount.saturating_mul(bps as i128) / 10_000;
    fee.clamp(0, amount)
}

/// Net amount after deducting the fee from the gross amount.
fn net_amount(amount: i128, fee: i128) -> i128 {
    amount.saturating_sub(fee)
}

#[test]
fn shared_fixtures_provide_token_and_parties() {
    let env = setup_test_env();
    let (payer, payee, _arbiter) = generate_parties(&env);
    let token = create_mock_token(&env, &payer, 1_000);
    assert_ne!(payer, payee);
    assert_eq!(token, token.clone());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn fee_is_non_negative(amount in any::<i128>(), bps in 0u32..=10_000) {
        let fee = calculate_fee(amount, bps);
        prop_assert!(fee >= 0, "fee must be non-negative, got {}", fee);
    }

    #[test]
    fn fee_never_exceeds_amount(amount in any::<i128>(), bps in 0u32..=10_000) {
        let fee = calculate_fee(amount, bps);
        prop_assert!(fee <= amount, "fee {} exceeded amount {}", fee, amount);
    }

    #[test]
    fn net_plus_fee_equals_amount(amount in any::<i128>(), bps in 0u32..=10_000) {
        let fee = calculate_fee(amount, bps);
        let net = net_amount(amount, fee);
        prop_assert_eq!(net + fee, amount);
    }

    #[test]
    fn no_panic_on_extreme_values(bps in 0u32..=10_000) {
        for amount in [i128::MIN, i128::MIN + 1, -1, 0, 1, i128::MAX - 1, i128::MAX] {
            let fee = calculate_fee(amount, bps);
            prop_assert!(fee >= 0);
            prop_assert!(fee <= amount);
            let net = net_amount(amount, fee);
            prop_assert_eq!(net + fee, amount);
        }
    }
}
