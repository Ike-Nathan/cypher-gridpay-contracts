//! Property-based fuzz tests for payment fee and rebate calculations.
//!
//! These tests exercise the fee/rebate math with arbitrary `i128` inputs to
//! guarantee the core invariants hold for every possible amount:
//!
//! * `fee >= 0`
//! * `fee <= amount`
//! * `net_amount + fee == amount`
//!
//! They also assert that extreme values never panic.

use proptest::prelude::*;

/// Basis points denominator (100% == 10_000 bps).
const BPS_DENOMINATOR: i128 = 10_000;

/// Compute the fee for a payment amount given a fee in basis points.
///
/// The fee is clamped so it can never exceed the payment amount, which keeps
/// `net_amount + fee == amount` true even for extreme inputs.
fn calculate_fee(amount: i128, fee_bps: i128) -> i128 {
    if amount <= 0 || fee_bps <= 0 {
        return 0;
    }

    let raw_fee = amount
        .checked_mul(fee_bps)
        .map(|product| product / BPS_DENOMINATOR)
        .unwrap_or(amount);

    raw_fee.clamp(0, amount)
}

/// Compute the net amount received after the fee is deducted.
fn calculate_net_amount(amount: i128, fee: i128) -> i128 {
    amount.saturating_sub(fee)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn fee_is_never_negative(amount in any::<i128>(), fee_bps in any::<i128>()) {
        let fee = calculate_fee(amount, fee_bps);
        prop_assert!(fee >= 0, "fee must be non-negative, got {}", fee);
    }

    #[test]
    fn fee_never_exceeds_amount(amount in any::<i128>(), fee_bps in any::<i128>()) {
        let fee = calculate_fee(amount, fee_bps);
        prop_assert!(fee <= amount, "fee {} exceeded amount {}", fee, amount);
    }

    #[test]
    fn net_amount_plus_fee_equals_amount(amount in any::<i128>(), fee_bps in any::<i128>()) {
        let fee = calculate_fee(amount, fee_bps);
        let net_amount = calculate_net_amount(amount, fee);
        prop_assert_eq!(
            net_amount + fee,
            amount,
            "net_amount + fee must equal amount (amount={}, fee={})",
            amount,
            fee
        );
    }

    #[test]
    fn extreme_values_do_not_panic(amount in prop_oneof![
        Just(i128::MIN),
        Just(i128::MAX),
        Just(0i128),
        Just(-1i128),
        Just(1i128),
        any::<i128>(),
    ], fee_bps in prop_oneof![
        Just(i128::MIN),
        Just(i128::MAX),
        Just(0i128),
        Just(BPS_DENOMINATOR),
        any::<i128>(),
    ]) {
        let fee = calculate_fee(amount, fee_bps);
        let net_amount = calculate_net_amount(amount, fee);

        prop_assert!(fee >= 0);
        prop_assert!(fee <= amount);
        prop_assert_eq!(net_amount + fee, amount);
    }
}
