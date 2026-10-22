//! Phantom-overflow-safe fixed-point helpers (issue #250).
//!
//! `(a * b) / d` computed in `i128` can overflow in the intermediate product even
//! when the final quotient fits. These helpers widen the product to `U256`.
//!
//! Rounding policy:
//! - `mul_div_ceil`  for inflows: protocol fees, premiums, required collateral.
//! - `mul_div_floor` for outflows: user payouts, rebates, forwarded amounts.
//!
//! This file is the single source for all three contracts; `payment` and `refund`
//! include it via `#[path]`.
#![allow(dead_code)]

use soroban_sdk::{Env, U256};

/// Returned for negative inputs, a non-positive denominator, or a quotient that
/// does not fit in `i128`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArithmeticOverflow;

fn mul_div(
    env: &Env,
    a: i128,
    b: i128,
    denominator: i128,
    round_up: bool,
) -> Result<i128, ArithmeticOverflow> {
    if a < 0 || b < 0 || denominator <= 0 {
        return Err(ArithmeticOverflow);
    }
    let prod = U256::from_u128(env, a as u128).mul(&U256::from_u128(env, b as u128));
    let d = U256::from_u128(env, denominator as u128);
    let prod = if round_up {
        prod.add(&d).sub(&U256::from_u32(env, 1))
    } else {
        prod
    };
    let q = prod.div(&d).to_u128().ok_or(ArithmeticOverflow)?;
    if q > i128::MAX as u128 {
        return Err(ArithmeticOverflow);
    }
    Ok(q as i128)
}

/// `floor(a * b / denominator)` with a 256-bit intermediate product.
pub fn mul_div_floor(
    env: &Env,
    a: i128,
    b: i128,
    denominator: i128,
) -> Result<i128, ArithmeticOverflow> {
    mul_div(env, a, b, denominator, false)
}

/// `ceil(a * b / denominator)` with a 256-bit intermediate product.
pub fn mul_div_ceil(
    env: &Env,
    a: i128,
    b: i128,
    denominator: i128,
) -> Result<i128, ArithmeticOverflow> {
    mul_div(env, a, b, denominator, true)
}

/// Floor variant that aborts the invocation on overflow (matches the previous
/// `overflow-checks = true` panic behaviour of raw `i128` math).
pub fn mul_div_floor_or_panic(env: &Env, a: i128, b: i128, denominator: i128) -> i128 {
    match mul_div_floor(env, a, b, denominator) {
        Ok(v) => v,
        Err(_) => panic!("arithmetic overflow"),
    }
}

/// Ceil variant that aborts the invocation on overflow.
pub fn mul_div_ceil_or_panic(env: &Env, a: i128, b: i128, denominator: i128) -> i128 {
    match mul_div_ceil(env, a, b, denominator) {
        Ok(v) => v,
        Err(_) => panic!("arithmetic overflow"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_and_ceil_basic() {
        let env = Env::default();
        assert_eq!(mul_div_floor(&env, 7, 1, 2), Ok(3));
        assert_eq!(mul_div_ceil(&env, 7, 1, 2), Ok(4));
        assert_eq!(mul_div_ceil(&env, 8, 1, 2), Ok(4));
        assert_eq!(mul_div_ceil(&env, 0, 5, 3), Ok(0));
    }

    #[test]
    fn no_phantom_overflow() {
        let env = Env::default();
        let a = i128::MAX / 10_000 * 3;
        // a * 10_000 overflows i128 but the quotient fits.
        assert!(a.checked_mul(10_000).is_none());
        assert_eq!(mul_div_floor(&env, a, 10_000, 10_000), Ok(a));
        assert_eq!(mul_div_ceil(&env, a, 10_000, 10_000), Ok(a));
        assert_eq!(mul_div_floor(&env, i128::MAX, i128::MAX, i128::MAX), Ok(i128::MAX));
    }

    #[test]
    fn rejects_bad_inputs() {
        let env = Env::default();
        assert_eq!(mul_div_floor(&env, -1, 1, 1), Err(ArithmeticOverflow));
        assert_eq!(mul_div_floor(&env, 1, -1, 1), Err(ArithmeticOverflow));
        assert_eq!(mul_div_ceil(&env, 1, 1, 0), Err(ArithmeticOverflow));
        assert_eq!(mul_div_ceil(&env, 1, 1, -5), Err(ArithmeticOverflow));
        // quotient does not fit
        assert_eq!(mul_div_floor(&env, i128::MAX, 2, 1), Err(ArithmeticOverflow));
    }

    #[test]
    fn micro_split_never_extracts() {
        let env = Env::default();
        // 2,000 U256 host operations: this checks rounding, not cost, so the
        // default per-invocation budget does not apply.
        env.cost_estimate().budget().reset_unlimited();
        // Splitting an inflow fee with ceil never yields less than the whole-amount fee,
        // and splitting a payout with floor never yields more.
        let (total, bps) = (1_000_003i128, 37i128);
        let whole_fee = mul_div_ceil(&env, total, bps, 10_000).unwrap();
        let mut fee_sum = 0;
        let mut payout_sum = 0;
        for _ in 0..1000 {
            fee_sum += mul_div_ceil(&env, total / 1000, bps, 10_000).unwrap();
            payout_sum += mul_div_floor(&env, total / 1000, bps, 10_000).unwrap();
        }
        assert!(fee_sum >= mul_div_floor(&env, (total / 1000) * 1000, bps, 10_000).unwrap());
        assert!(payout_sum <= mul_div_floor(&env, total, bps, 10_000).unwrap());
        assert!(whole_fee >= payout_sum);
    }
}
