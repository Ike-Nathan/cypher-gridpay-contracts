//! Property tests for the U256-backed mul_div helpers (issue #250).
use crate::fixed_math::{mul_div_ceil, mul_div_floor};
use proptest::prelude::*;
use soroban_sdk::Env;

const NEAR_MAX: i128 = i128::MAX / 10_000;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn floor_le_ceil_and_differ_by_at_most_one(
        a in (NEAR_MAX - 1_000_000)..=i128::MAX / 2,
        bps in 0i128..=10_000,
    ) {
        let env = Env::default();
        let f = mul_div_floor(&env, a, bps, 10_000).unwrap();
        let c = mul_div_ceil(&env, a, bps, 10_000).unwrap();
        prop_assert!(f <= c);
        prop_assert!(c - f <= 1);
        prop_assert!(f <= a);
    }

    #[test]
    fn fee_plus_payout_never_exceeds_inflow(
        a in 0i128..=i128::MAX / 10_000,
        bps in 0i128..=10_000,
    ) {
        let env = Env::default();
        let fee = mul_div_ceil(&env, a, bps, 10_000).unwrap();
        prop_assert!(fee <= a);
        let payout = a - fee;
        prop_assert!(payout + fee <= a);
    }

    #[test]
    fn matches_checked_i128_when_it_does_not_overflow(
        a in 0i128..=1_000_000_000_000_000_000i128,
        b in 0i128..=1_000_000_000i128,
        d in 1i128..=1_000_000_000i128,
    ) {
        let env = Env::default();
        let expected = a.checked_mul(b).unwrap() / d;
        prop_assert_eq!(mul_div_floor(&env, a, b, d).unwrap(), expected);
    }
}
