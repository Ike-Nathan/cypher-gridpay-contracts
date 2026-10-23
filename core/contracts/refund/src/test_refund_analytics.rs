#![cfg(test)]
use super::*;
use soroban_sdk::{
    testutils::Address as _,
    Address, Env, String,
};

// Disabled: Calls get_refund_analytics_range / get_merchant_refund_analytics,
// which were never merged. The epoch reason-code tests below cover the
// analytics that exist.
#[cfg(any())]
#[test]
fn test_bucket_accuracy_and_range_aggregation() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let merchant2 = Address::generate(&env);
    let customer = Address::generate(&env);

    let contract = env.register_stellar_asset_contract_v2(admin.clone());
    let contract_address = contract.address();
    let token_client = token::Client::new(&env, &contract_address);
    let token_admin = token::StellarAssetClient::new(&env, &contract_address);
    token_admin.mint(&merchant, &1000000);
    token_admin.mint(&merchant2, &1000000);
    token_admin.mint(&customer, &1000000);

    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    // Disable fraud checks to avoid needing a payment contract.
    client.set_fraud_config(
        &admin,
        &FraudConfig {
            max_refund_rate_bps: 10000,
            min_transactions_for_check: 1000,
            enabled: false,
        },
    );

    let day = 86400u64;
    let day_start1 = 1 * day;
    let day_start2 = 2 * day;

    // Day 1 timestamps
    env.ledger().with_mut(|li| li.timestamp = day_start1 + 10);
    let r1 = client.request_refund(
        &merchant,
        &1,
        &customer,
        &100,
        &100,
        &token_client.address,
        &String::from_str(&env, "r1"),
        &RefundReasonCode::Other,
        &(env.ledger().timestamp() - 5),
    );

    client.approve_refund(&admin, &r1);
    env.ledger().with_mut(|li| li.timestamp = day_start1 + 20);
    client.process_refund(&admin, &r1);

    // Day 1 rejected
    env.ledger().with_mut(|li| li.timestamp = day_start1 + 200);
    let r2 = client.request_refund(
        &merchant,
        &2,
        &customer,
        &200,
        &200,
        &token_client.address,
        &String::from_str(&env, "r2"),
        &RefundReasonCode::Other,
        &(env.ledger().timestamp() - 5),
    );
    client.reject_refund(&admin, &r2, &String::from_str(&env, "no"));

    // Day 2 approved+processed for merchant2
    env.ledger().with_mut(|li| li.timestamp = day_start2 + 30);
    let r3 = client.request_refund(
        &merchant2,
        &3,
        &customer,
        &300,
        &300,
        &token_client.address,
        &String::from_str(&env, "r3"),
        &RefundReasonCode::Other,
        &(env.ledger().timestamp() - 5),
    );
    client.approve_refund(&admin, &r3);
    env.ledger().with_mut(|li| li.timestamp = day_start2 + 50);
    client.process_refund(&admin, &r3);

    // Range query day1..day2
    let range = client.get_refund_analytics_range(&(day_start1), &(day_start2 + day - 1));
    assert!(range.len() >= 2);

    let b1 = range.iter().find(|b| b.bucket_start == day_start1).unwrap();
    assert_eq!(b1.total_requests, 2);
    assert_eq!(b1.approved_count, 1);
    assert_eq!(b1.rejected_count, 1);
    assert_eq!(b1.total_amount, 100 + 200);

    let b2 = range.iter().find(|b| b.bucket_start == day_start2).unwrap();
    assert_eq!(b2.total_requests, 1);
    assert_eq!(b2.approved_count, 1);
    assert_eq!(b2.rejected_count, 0);
    assert_eq!(b2.total_amount, 300);

    // Merchant isolation
    let m1 = client.get_merchant_refund_analytics(&merchant);
    assert_eq!(m1.total_requested, 2);
    assert_eq!(m1.total_approved, 1);
    assert_eq!(m1.total_rejected, 1);
    assert_eq!(m1.total_amount_refunded, 100);

    let m2 = client.get_merchant_refund_analytics(&merchant2);
    assert_eq!(m2.total_requested, 1);
    assert_eq!(m2.total_approved, 1);
    assert_eq!(m2.total_rejected, 0);
    assert_eq!(m2.total_amount_refunded, 300);
}

// Disabled: Calls get_refund_analytics_range / get_merchant_refund_analytics,
// which were never merged. The epoch reason-code tests below cover the
// analytics that exist.
#[cfg(any())]
#[test]
fn test_bucket_boundaries() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    let contract = env.register_stellar_asset_contract_v2(admin.clone());
    let contract_address = contract.address();
    let token_client = token::Client::new(&env, &contract_address);
    let token_admin = token::StellarAssetClient::new(&env, &contract_address);
    token_admin.mint(&merchant, &1000000);
    token_admin.mint(&customer, &1000000);

    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    client.set_fraud_config(
        &admin,
        &FraudConfig {
            max_refund_rate_bps: 10000,
            min_transactions_for_check: 1000,
            enabled: false,
        },
    );

    let day = 86400u64;
    let ds = 5 * day;

    // start-of-day
    env.ledger().with_mut(|li| li.timestamp = ds);
    let r_start = client.request_refund(
        &merchant,
        &1,
        &customer,
        &100,
        &100,
        &token_client.address,
        &String::from_str(&env, "start"),
        &RefundReasonCode::Other,
        &(env.ledger().timestamp()),
    );
    client.approve_refund(&admin, &r_start);

    env.ledger().with_mut(|li| li.timestamp = ds + 1);
    client.process_refund(&admin, &r_start);

    // last-second-of-day
    env.ledger().with_mut(|li| li.timestamp = ds + day - 1);
    let r_end = client.request_refund(
        &merchant,
        &2,
        &customer,
        &200,
        &200,
        &token_client.address,
        &String::from_str(&env, "end"),
        &RefundReasonCode::Other,
        &(env.ledger().timestamp()),
    );
    client.approve_refund(&admin, &r_end);

    env.ledger().with_mut(|li| li.timestamp = ds + day - 1);
    client.process_refund(&admin, &r_end);

    let range = client.get_refund_analytics_range(&ds, &(ds + day - 1));
    let b = range.iter().find(|b| b.bucket_start == ds).unwrap();
    assert_eq!(b.total_requests, 2);
    assert_eq!(b.approved_count, 2);
    assert_eq!(b.rejected_count, 0);
}

// ── Per-merchant, per-epoch reason-code analytics ───────────────────────────

mod epoch_reason_analytics {
    use super::*;
    use crate::storage::{ALL_REASON_CODES, REASON_EPOCH_SECS};
    use soroban_sdk::testutils::Ledger;

    struct Desk {
        env: Env,
        client: RefundContractClient<'static>,
        admin: Address,
        token: Address,
        next_payment: core::cell::Cell<u64>,
    }

    /// Starts at the beginning of epoch 10, with a funded refund pool.
    fn desk() -> Desk {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(10 * REASON_EPOCH_SECS);
        let admin = Address::generate(&env);
        let client = RefundContractClient::new(&env, &env.register(RefundContract, ()));
        client.initialize(&admin);
        let token = env.register_stellar_asset_contract_v2(admin.clone()).address();
        token::StellarAssetClient::new(&env, &token).mint(&client.address, &1_000_000);
        Desk { env, client, admin, token, next_payment: core::cell::Cell::new(1) }
    }

    impl Desk {
        fn request(&self, merchant: &Address, reason: RefundReasonCode, amount: i128) -> u64 {
            let payment_id = self.next_payment.get();
            self.next_payment.set(payment_id + 1);
            self.client.request_refund(
                merchant,
                &payment_id,
                &Address::generate(&self.env),
                &amount,
                &1_000,
                &self.token,
                &String::from_str(&self.env, "r"),
                &reason,
                &self.env.ledger().timestamp(),
            )
        }

        fn requests(&self, merchant: &Address, reason: RefundReasonCode, n: u32) {
            for _ in 0..n {
                self.request(merchant, reason.clone(), 10);
            }
        }

        fn pay(&self, id: u64) {
            if self.client.get_refund(&id).status == RefundStatus::Requested {
                self.client.approve_refund(&self.admin, &id);
            }
            self.client.process_refund(&self.admin, &id);
        }

        fn epoch(&self) -> u32 {
            self.client.get_reason_epoch(&self.env.ledger().timestamp())
        }

        fn next_epoch(&self) {
            self.env
                .ledger()
                .set_timestamp(self.env.ledger().timestamp() + REASON_EPOCH_SECS);
        }

        fn stats(&self, merchant: &Address, epoch: u32, reason: RefundReasonCode) -> EpochReasonCodeStats {
            self.client
                .get_merchant_reason_analytics(merchant, &epoch)
                .iter()
                .find(|s| s.reason_code == reason)
                .unwrap()
        }

        fn delta(&self, merchant: &Address, epoch: u32, reason: RefundReasonCode) -> i64 {
            self.client
                .get_reason_code_velocity_delta(merchant, &epoch)
                .iter()
                .find(|(r, _)| *r == reason)
                .unwrap()
                .1
        }
    }

    #[test]
    fn analytics_always_cover_all_six_reason_codes_in_order() {
        let d = desk();
        let m = Address::generate(&d.env);
        let empty = d.client.get_merchant_reason_analytics(&m, &d.epoch());
        assert_eq!(empty.len(), 6);
        for (i, s) in empty.iter().enumerate() {
            assert_eq!(s.reason_code, ALL_REASON_CODES[i]);
            assert_eq!(s.epoch_id, d.epoch());
            assert_eq!((s.count, s.approved_count, s.total_amount), (0, 0, 0));
        }
        assert_eq!(d.client.get_reason_code_velocity_delta(&m, &d.epoch()).len(), 6);
    }

    #[test]
    fn stats_are_scoped_per_merchant_with_a_platform_total() {
        let d = desk();
        let m1 = Address::generate(&d.env);
        let m2 = Address::generate(&d.env);
        d.requests(&m1, RefundReasonCode::Unauthorized, 2);
        d.requests(&m2, RefundReasonCode::Unauthorized, 1);
        d.requests(&m2, RefundReasonCode::NonDelivery, 3);

        let e = d.epoch();
        assert_eq!(d.stats(&m1, e, RefundReasonCode::Unauthorized).count, 2);
        assert_eq!(d.stats(&m1, e, RefundReasonCode::NonDelivery).count, 0);
        assert_eq!(d.stats(&m2, e, RefundReasonCode::Unauthorized).count, 1);
        assert_eq!(d.stats(&m2, e, RefundReasonCode::NonDelivery).count, 3);

        let platform = d.client.get_platform_reason_analytics(&e);
        let unauthorized = platform
            .iter()
            .find(|s| s.reason_code == RefundReasonCode::Unauthorized)
            .unwrap();
        assert_eq!(unauthorized.count, 3);
    }

    #[test]
    fn payouts_add_approved_count_and_volume() {
        let d = desk();
        let m = Address::generate(&d.env);
        let a = d.request(&m, RefundReasonCode::ProductDefect, 100);
        let b = d.request(&m, RefundReasonCode::ProductDefect, 50);
        d.request(&m, RefundReasonCode::ProductDefect, 70); // never paid
        d.pay(a);
        d.pay(b);

        let s = d.stats(&m, d.epoch(), RefundReasonCode::ProductDefect);
        assert_eq!((s.count, s.approved_count, s.total_amount), (3, 2, 150));
    }

    #[test]
    fn epochs_roll_over_and_payouts_stay_with_the_request_epoch() {
        let d = desk();
        let m = Address::generate(&d.env);
        let first = d.epoch();
        let late = d.request(&m, RefundReasonCode::Other, 40);
        d.requests(&m, RefundReasonCode::Other, 1);

        d.next_epoch();
        let second = d.epoch();
        assert_eq!(second, first + 1);
        d.requests(&m, RefundReasonCode::Other, 3);
        // Paid in the second epoch, but requested in the first.
        d.pay(late);

        let s1 = d.stats(&m, first, RefundReasonCode::Other);
        let s2 = d.stats(&m, second, RefundReasonCode::Other);
        assert_eq!((s1.count, s1.approved_count, s1.total_amount), (2, 1, 40));
        assert_eq!((s2.count, s2.approved_count, s2.total_amount), (3, 0, 0));
    }

    #[test]
    fn velocity_delta_in_basis_points() {
        let d = desk();
        let m = Address::generate(&d.env);
        d.requests(&m, RefundReasonCode::Unauthorized, 2);
        d.requests(&m, RefundReasonCode::CustomerRequest, 4);
        d.requests(&m, RefundReasonCode::NonDelivery, 3);
        d.next_epoch();
        d.requests(&m, RefundReasonCode::Unauthorized, 5);
        d.requests(&m, RefundReasonCode::CustomerRequest, 2);
        d.requests(&m, RefundReasonCode::Other, 3);
        d.requests(&m, RefundReasonCode::NonDelivery, 3);

        let e = d.epoch();
        assert_eq!(d.delta(&m, e, RefundReasonCode::Unauthorized), 15_000); // 2 -> 5
        assert_eq!(d.delta(&m, e, RefundReasonCode::CustomerRequest), -5_000); // 4 -> 2
        assert_eq!(d.delta(&m, e, RefundReasonCode::NonDelivery), 0); // 3 -> 3
        // Zero prior: treated as one rather than dividing by zero.
        assert_eq!(d.delta(&m, e, RefundReasonCode::Other), 30_000); // 0 -> 3
        assert_eq!(d.delta(&m, e, RefundReasonCode::ProductDefect), 0); // 0 -> 0

        // Another merchant's spike doesn't leak in.
        let other = Address::generate(&d.env);
        assert_eq!(d.delta(&other, e, RefundReasonCode::Unauthorized), 0);
        // Epoch 0 has no prior epoch.
        assert_eq!(d.delta(&m, 0, RefundReasonCode::Unauthorized), 0);
    }
}
