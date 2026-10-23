#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, testutils::Events, Address, BytesN, Env, String};

// ── helpers ──────────────────────────────────────────────────────────────────

fn setup() -> (Env, Address, RefundContractClient<'static>) {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    env.mock_all_auths();
    // Disable fraud checks so they don't interfere with eligibility tests
    client.set_fraud_config(
        &admin,
        &FraudConfig {
            max_refund_rate_bps: 10000,
            min_transactions_for_check: 1000,
            enabled: false,
        },
    );
    (env, admin, client)
}

fn zero_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0u8; 32])
}

fn reason_hash(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

/// Request a refund and return its id. Uses unique payment_ids to avoid
/// cumulative-refund limits.
fn request_refund(
    client: &RefundContractClient<'_>,
    env: &Env,
    merchant: &Address,
    customer: &Address,
    payment_id: u64,
) -> u64 {
    let token = Address::generate(env);
    client.request_refund(
        merchant,
        &payment_id,
        customer,
        &100i128,
        &100i128,
        &token,
        &String::from_str(env, "test"),
        &RefundReasonCode::Other,
        &0_u64,
    )
}

// ── set_refund_eligibility ────────────────────────────────────────────────────

#[test]
fn test_set_eligibility_block_stores_entry() {
    let (env, _admin, client) = setup();
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    client.set_refund_eligibility(
        &merchant,
        &customer,
        &EligibilityRule::Block,
        &reason_hash(&env, 1),
    );

    let rule = client.check_refund_eligibility(&merchant, &customer);
    assert_eq!(rule, EligibilityRule::Block);
}

#[test]
fn test_set_eligibility_allow_stores_entry() {
    let (env, _admin, client) = setup();
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    client.set_refund_eligibility(
        &merchant,
        &customer,
        &EligibilityRule::Allow,
        &zero_hash(&env),
    );

    let rule = client.check_refund_eligibility(&merchant, &customer);
    assert_eq!(rule, EligibilityRule::Allow);
}

#[test]
fn test_set_eligibility_overwrites_existing_rule() {
    let (env, _admin, client) = setup();
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    // First block the customer
    client.set_refund_eligibility(
        &merchant,
        &customer,
        &EligibilityRule::Block,
        &reason_hash(&env, 2),
    );
    assert_eq!(
        client.check_refund_eligibility(&merchant, &customer),
        EligibilityRule::Block
    );

    // Admin overrides to Allow
    client.set_refund_eligibility(
        &merchant,
        &customer,
        &EligibilityRule::Allow,
        &reason_hash(&env, 3),
    );
    assert_eq!(
        client.check_refund_eligibility(&merchant, &customer),
        EligibilityRule::Allow
    );
}

#[test]
fn test_set_eligibility_emits_event() {
    let (env, _admin, client) = setup();
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    client.set_refund_eligibility(
        &merchant,
        &customer,
        &EligibilityRule::Block,
        &zero_hash(&env),
    );

    let events = env.events().all();
    assert!(events.len() > 0);
}

// ── check_refund_eligibility ──────────────────────────────────────────────────

#[test]
fn test_check_eligibility_defaults_to_allow_when_no_entry() {
    let (env, _admin, client) = setup();
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    // No entry set — should default to Allow
    let rule = client.check_refund_eligibility(&merchant, &customer);
    assert_eq!(rule, EligibilityRule::Allow);
}

#[test]
fn test_check_eligibility_is_merchant_scoped() {
    let (env, _admin, client) = setup();
    let merchant_a = Address::generate(&env);
    let merchant_b = Address::generate(&env);
    let customer = Address::generate(&env);

    // Block under merchant_a only
    client.set_refund_eligibility(
        &merchant_a,
        &customer,
        &EligibilityRule::Block,
        &zero_hash(&env),
    );

    assert_eq!(
        client.check_refund_eligibility(&merchant_a, &customer),
        EligibilityRule::Block
    );
    // merchant_b has no entry → Allow
    assert_eq!(
        client.check_refund_eligibility(&merchant_b, &customer),
        EligibilityRule::Allow
    );
}

// ── block enforcement in request_refund ──────────────────────────────────────

#[test]
#[should_panic]
fn test_blocked_customer_cannot_request_refund() {
    let (env, _admin, client) = setup();
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    client.set_refund_eligibility(
        &merchant,
        &customer,
        &EligibilityRule::Block,
        &reason_hash(&env, 10),
    );

    // This should panic with CustomerBlockedFromRefund
    request_refund(&client, &env, &merchant, &customer, 1);
}

#[test]
fn test_allowed_customer_can_request_refund() {
    let (env, _admin, client) = setup();
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    client.set_refund_eligibility(
        &merchant,
        &customer,
        &EligibilityRule::Allow,
        &zero_hash(&env),
    );

    let refund_id = request_refund(&client, &env, &merchant, &customer, 1);
    assert!(refund_id > 0);
}

#[test]
fn test_customer_with_no_entry_can_request_refund() {
    let (env, _admin, client) = setup();
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    // No eligibility entry set at all
    let refund_id = request_refund(&client, &env, &merchant, &customer, 1);
    assert!(refund_id > 0);
}

#[test]
fn test_block_only_affects_specific_merchant_customer_pair() {
    let (env, _admin, client) = setup();
    let merchant = Address::generate(&env);
    let blocked_customer = Address::generate(&env);
    let allowed_customer = Address::generate(&env);

    client.set_refund_eligibility(
        &merchant,
        &blocked_customer,
        &EligibilityRule::Block,
        &reason_hash(&env, 11),
    );

    // allowed_customer is unaffected
    let refund_id = request_refund(&client, &env, &merchant, &allowed_customer, 2);
    assert!(refund_id > 0);
}

// ── admin override ────────────────────────────────────────────────────────────

#[test]
fn test_admin_can_override_merchant_block_with_allow() {
    let (env, admin, client) = setup();
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    // Merchant blocks the customer
    client.set_refund_eligibility(
        &merchant,
        &customer,
        &EligibilityRule::Block,
        &reason_hash(&env, 20),
    );
    assert_eq!(
        client.check_refund_eligibility(&merchant, &customer),
        EligibilityRule::Block
    );

    // Admin overrides with Allow (admin calls set_refund_eligibility on behalf of merchant)
    // In tests mock_all_auths covers both merchant and admin auth
    client.set_refund_eligibility(
        &merchant,
        &customer,
        &EligibilityRule::Allow,
        &reason_hash(&env, 21),
    );
    assert_eq!(
        client.check_refund_eligibility(&merchant, &customer),
        EligibilityRule::Allow
    );

    // Customer can now request a refund
    let refund_id = request_refund(&client, &env, &merchant, &customer, 5);
    assert!(refund_id > 0);

    let _ = admin; // suppress unused warning
}

#[test]
fn test_block_then_allow_then_block_again() {
    let (env, _admin, client) = setup();
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    client.set_refund_eligibility(
        &merchant,
        &customer,
        &EligibilityRule::Block,
        &zero_hash(&env),
    );
    assert_eq!(
        client.check_refund_eligibility(&merchant, &customer),
        EligibilityRule::Block
    );

    client.set_refund_eligibility(
        &merchant,
        &customer,
        &EligibilityRule::Allow,
        &zero_hash(&env),
    );
    assert_eq!(
        client.check_refund_eligibility(&merchant, &customer),
        EligibilityRule::Allow
    );

    client.set_refund_eligibility(
        &merchant,
        &customer,
        &EligibilityRule::Block,
        &zero_hash(&env),
    );
    assert_eq!(
        client.check_refund_eligibility(&merchant, &customer),
        EligibilityRule::Block
    );
}

// ── remove_refund_eligibility ─────────────────────────────────────────────────

#[test]
fn test_remove_existing_entry_succeeds() {
    let (env, _admin, client) = setup();
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    client.set_refund_eligibility(
        &merchant,
        &customer,
        &EligibilityRule::Block,
        &zero_hash(&env),
    );

    client.remove_refund_eligibility(&merchant, &customer);

    // After removal defaults back to Allow
    assert_eq!(
        client.check_refund_eligibility(&merchant, &customer),
        EligibilityRule::Allow
    );
}

#[test]
fn test_remove_entry_allows_customer_to_request_refund_again() {
    let (env, _admin, client) = setup();
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    client.set_refund_eligibility(
        &merchant,
        &customer,
        &EligibilityRule::Block,
        &zero_hash(&env),
    );

    client.remove_refund_eligibility(&merchant, &customer);

    let refund_id = request_refund(&client, &env, &merchant, &customer, 10);
    assert!(refund_id > 0);
}

#[test]
#[should_panic]
fn test_remove_nonexistent_entry_returns_error() {
    let (env, _admin, client) = setup();
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    // No entry was ever set — should panic with EligibilityEntryNotFound
    client.remove_refund_eligibility(&merchant, &customer);
}

#[test]
fn test_remove_emits_event() {
    let (env, _admin, client) = setup();
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    client.set_refund_eligibility(
        &merchant,
        &customer,
        &EligibilityRule::Block,
        &zero_hash(&env),
    );

    client.remove_refund_eligibility(&merchant, &customer);

    // After removal the events list should contain at least the EligibilityRemoved event
    let events = env.events().all();
    assert!(events.len() > 0);
}

// ── get_merchant_eligibility_list ─────────────────────────────────────────────

#[test]
fn test_get_eligibility_list_returns_all_entries() {
    let (env, _admin, client) = setup();
    let merchant = Address::generate(&env);
    let customer_a = Address::generate(&env);
    let customer_b = Address::generate(&env);
    let customer_c = Address::generate(&env);

    client.set_refund_eligibility(
        &merchant,
        &customer_a,
        &EligibilityRule::Block,
        &reason_hash(&env, 1),
    );
    client.set_refund_eligibility(
        &merchant,
        &customer_b,
        &EligibilityRule::Allow,
        &reason_hash(&env, 2),
    );
    client.set_refund_eligibility(
        &merchant,
        &customer_c,
        &EligibilityRule::Block,
        &reason_hash(&env, 3),
    );

    let list = client.get_merchant_eligibility_list(&merchant);
    assert_eq!(list.len(), 3);
}

#[test]
fn test_get_eligibility_list_empty_for_new_merchant() {
    let (env, _admin, client) = setup();
    let merchant = Address::generate(&env);

    let list = client.get_merchant_eligibility_list(&merchant);
    assert_eq!(list.len(), 0);
}

#[test]
fn test_get_eligibility_list_is_merchant_scoped() {
    let (env, _admin, client) = setup();
    let merchant_a = Address::generate(&env);
    let merchant_b = Address::generate(&env);
    let customer = Address::generate(&env);

    client.set_refund_eligibility(
        &merchant_a,
        &customer,
        &EligibilityRule::Block,
        &zero_hash(&env),
    );

    let list_a = client.get_merchant_eligibility_list(&merchant_a);
    let list_b = client.get_merchant_eligibility_list(&merchant_b);

    assert_eq!(list_a.len(), 1);
    assert_eq!(list_b.len(), 0);
}

#[test]
fn test_get_eligibility_list_reflects_correct_rules() {
    let (env, _admin, client) = setup();
    let merchant = Address::generate(&env);
    let customer_a = Address::generate(&env);
    let customer_b = Address::generate(&env);

    client.set_refund_eligibility(
        &merchant,
        &customer_a,
        &EligibilityRule::Block,
        &reason_hash(&env, 5),
    );
    client.set_refund_eligibility(
        &merchant,
        &customer_b,
        &EligibilityRule::Allow,
        &reason_hash(&env, 6),
    );

    let list = client.get_merchant_eligibility_list(&merchant);
    assert_eq!(list.len(), 2);

    // Verify each entry has the correct rule
    let mut found_block = false;
    let mut found_allow = false;
    for i in 0..list.len() {
        let entry = list.get(i).unwrap();
        match entry.rule {
            EligibilityRule::Block => found_block = true,
            EligibilityRule::Allow => found_allow = true,
        }
    }
    assert!(found_block);
    assert!(found_allow);
}

#[test]
fn test_get_eligibility_list_shrinks_after_removal() {
    let (env, _admin, client) = setup();
    let merchant = Address::generate(&env);
    let customer_a = Address::generate(&env);
    let customer_b = Address::generate(&env);

    client.set_refund_eligibility(
        &merchant,
        &customer_a,
        &EligibilityRule::Block,
        &zero_hash(&env),
    );
    client.set_refund_eligibility(
        &merchant,
        &customer_b,
        &EligibilityRule::Block,
        &zero_hash(&env),
    );

    assert_eq!(client.get_merchant_eligibility_list(&merchant).len(), 2);

    client.remove_refund_eligibility(&merchant, &customer_a);

    assert_eq!(client.get_merchant_eligibility_list(&merchant).len(), 1);
    // Remaining entry should be customer_b
    let list = client.get_merchant_eligibility_list(&merchant);
    assert_eq!(list.get(0).unwrap().customer, customer_b);
}

#[test]
fn test_update_existing_entry_does_not_duplicate_in_list() {
    let (env, _admin, client) = setup();
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    client.set_refund_eligibility(
        &merchant,
        &customer,
        &EligibilityRule::Block,
        &zero_hash(&env),
    );
    // Update the same customer — should not add a second entry
    client.set_refund_eligibility(
        &merchant,
        &customer,
        &EligibilityRule::Allow,
        &zero_hash(&env),
    );

    let list = client.get_merchant_eligibility_list(&merchant);
    assert_eq!(list.len(), 1);
    assert_eq!(list.get(0).unwrap().rule, EligibilityRule::Allow);
}

// ── merchant standing (MerchantStatus) ───────────────────────────────────────

fn try_request_refund(
    client: &RefundContractClient<'_>,
    env: &Env,
    merchant: &Address,
    customer: &Address,
    payment_id: u64,
) -> Result<u64, Error> {
    let token = Address::generate(env);
    client
        .try_request_refund(
            merchant,
            &payment_id,
            customer,
            &100i128,
            &100i128,
            &token,
            &String::from_str(env, "test"),
            &RefundReasonCode::Other,
            &0_u64,
        )
        .map(|r| r.unwrap())
        .map_err(|e| e.unwrap())
}

#[test]
fn test_merchant_status_defaults_to_active() {
    let (env, _admin, client) = setup();
    let merchant = Address::generate(&env);

    assert_eq!(
        client.get_merchant_status(&merchant),
        MerchantStatus::Active
    );
}

#[test]
fn test_suspended_merchant_cannot_request_refund() {
    let (env, admin, client) = setup();
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    client.set_merchant_status(&admin, &merchant, &MerchantStatus::Suspended);
    assert_eq!(
        client.get_merchant_status(&merchant),
        MerchantStatus::Suspended
    );

    let result = try_request_refund(&client, &env, &merchant, &customer, 1);
    assert_eq!(result, Err(Error::Ext(ExtError::MerchantNotEligible)));
}

#[test]
fn test_sanctioned_merchant_cannot_request_refund() {
    let (env, admin, client) = setup();
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    client.set_merchant_status(&admin, &merchant, &MerchantStatus::Sanctioned);

    let result = try_request_refund(&client, &env, &merchant, &customer, 1);
    assert_eq!(result, Err(Error::Ext(ExtError::MerchantNotEligible)));
}

#[test]
fn test_reactivated_merchant_can_request_refund() {
    let (env, admin, client) = setup();
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    client.set_merchant_status(&admin, &merchant, &MerchantStatus::Suspended);
    client.set_merchant_status(&admin, &merchant, &MerchantStatus::Active);

    assert!(try_request_refund(&client, &env, &merchant, &customer, 1).is_ok());
}

#[test]
fn test_suspended_merchant_refund_not_processed() {
    let (env, admin, client) = setup();
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    let refund_id = request_refund(&client, &env, &merchant, &customer, 1);
    client.approve_refund(&admin, &refund_id);

    // Merchant is suspended after approval but before payout.
    client.set_merchant_status(&admin, &merchant, &MerchantStatus::Suspended);

    let result = client.try_process_refund(&admin, &refund_id);
    assert_eq!(result, Err(Ok(Error::Ext(ExtError::MerchantNotEligible))));
    assert_eq!(client.get_refund(&refund_id).status, RefundStatus::Approved);
}

#[test]
fn test_non_admin_cannot_set_merchant_status() {
    let (env, _admin, client) = setup();
    let merchant = Address::generate(&env);
    let impostor = Address::generate(&env);

    let result = client.try_set_merchant_status(&impostor, &merchant, &MerchantStatus::Suspended);
    assert_eq!(result, Err(Ok(Error::Core(CoreError::Unauthorized))));
    assert_eq!(
        client.get_merchant_status(&merchant),
        MerchantStatus::Active
    );
}

// ── Refund-risk auto-throttling ──────────────────────────────────────────────

mod auto_throttle {
    use super::*;
    use soroban_sdk::testutils::Ledger;
    use soroban_sdk::{token, Map, Symbol, TryFromVal, Val};

    struct Shop {
        env: Env,
        admin: Address,
        client: RefundContractClient<'static>,
        merchant: Address,
        token: Address,
        next_payment: core::cell::Cell<u64>,
    }

    impl Shop {
        /// A refund for a fresh customer and payment (no rate limit applies).
        fn request(&self, reason: RefundReasonCode) -> u64 {
            let payment_id = self.next_payment.get();
            self.next_payment.set(payment_id + 1);
            self.client.request_refund(
                &self.merchant,
                &payment_id,
                &Address::generate(&self.env),
                &100,
                &1_000,
                &self.token,
                &String::from_str(&self.env, "r"),
                &reason,
                &0,
            )
        }

        /// A refund that is approved and paid out (substantiated).
        fn substantiate(&self, reason: RefundReasonCode) -> u64 {
            let id = self.request(reason);
            self.client.approve_refund(&self.admin, &id);
            self.client.process_refund(&self.admin, &id);
            id
        }

        fn status(&self) -> MerchantStatus {
            self.client.get_merchant_status(&self.merchant)
        }

        /// `ratio_bps` and whether `from == to`, for each MerchantAutoThrottled
        /// event of the last invocation.
        fn throttle_events(&self) -> soroban_sdk::Vec<(u32, bool)> {
            let mut found = soroban_sdk::Vec::new(&self.env);
            let name = Symbol::new(&self.env, "merchant_auto_throttled");
            for (_, topics, data) in self.env.events().all().iter() {
                let topic = topics.get(0).and_then(|t| Symbol::try_from_val(&self.env, &t).ok());
                if topic != Some(name.clone()) {
                    continue;
                }
                let fields = Map::<Symbol, Val>::try_from_val(&self.env, &data).unwrap();
                let get = |k: &str| fields.get(Symbol::new(&self.env, k)).unwrap();
                let ratio = u32::try_from_val(&self.env, &get("ratio_bps")).unwrap();
                let from = MerchantStatus::try_from_val(&self.env, &get("from")).unwrap();
                let to = MerchantStatus::try_from_val(&self.env, &get("to")).unwrap();
                found.push_back((ratio, from == to));
            }
            found
        }
    }

    fn shop() -> Shop {
        let (env, admin, client) = setup();
        let token = env.register_stellar_asset_contract_v2(admin.clone()).address();
        // Fund the refund pool so processed refunds can pay out.
        token::StellarAssetClient::new(&env, &token).mint(&client.address, &1_000_000);
        client.set_merchant_risk_thresholds(
            &admin,
            &MerchantRiskThresholds {
                warning_ratio_bps: 2_000,
                restricted_ratio_bps: 4_000,
                suspended_ratio_bps: 7_000,
                min_sample_tx_count: 20,
            },
        );
        let merchant = Address::generate(&env);
        Shop {
            env,
            admin,
            client,
            merchant,
            token,
            next_payment: core::cell::Cell::new(1),
        }
    }

    #[test]
    fn no_throttling_below_the_sample_floor() {
        let s = shop();
        // 19 substantiated refunds, every one high-risk: 100% ratio, but the
        // sample is below the 20-transaction floor.
        for _ in 0..19 {
            s.substantiate(RefundReasonCode::Unauthorized);
        }
        assert_eq!(s.status(), MerchantStatus::Active);
        assert_eq!(s.client.get_merchant_risk_window(&s.merchant).refund_count, 19);

        // The 20th meets the floor and suspends.
        s.substantiate(RefundReasonCode::Unauthorized);
        assert_eq!(s.status(), MerchantStatus::Suspended);
    }

    #[test]
    fn high_risk_ratio_restricts_then_suspends() {
        let s = shop();
        for i in 0..20 {
            let reason = if i % 2 == 0 {
                RefundReasonCode::ProductDefect
            } else {
                RefundReasonCode::CustomerRequest
            };
            s.substantiate(reason);
        }
        // 10 / 20 = 50%: restricted (>= 40%), not suspended (< 70%).
        assert_eq!(s.status(), MerchantStatus::Restricted);
        let window = s.client.get_merchant_risk_window(&s.merchant);
        assert_eq!((window.refund_count, window.high_risk_refund_count), (20, 10));
        assert_eq!(window.total_refund_volume, 20 * 100);

        // Restricted: customers can still request, and the admin's processing
        // is the sign-off that lets a refund settle...
        let id = s.substantiate(RefundReasonCode::CustomerRequest);
        assert_eq!(s.client.get_refund(&id).status, RefundStatus::Processed);
        // ...but the merchant's own overrides are blocked.
        let denied = s.request(RefundReasonCode::Other);
        s.client
            .reject_refund(&s.admin, &denied, &String::from_str(&s.env, "no"));
        assert_eq!(
            s.client.try_merchant_override_denial(&s.merchant, &denied),
            Err(Ok(Error::Ext(ExtError::MerchantNotEligible)))
        );

        // More high-risk refunds push it past 70%: suspended.
        for _ in 0..30 {
            if s.status() == MerchantStatus::Suspended {
                break;
            }
            s.substantiate(RefundReasonCode::NonDelivery);
        }
        assert_eq!(s.status(), MerchantStatus::Suspended);
    }

    #[test]
    fn unsubstantiated_requests_cannot_grief_a_merchant() {
        let s = shop();
        // 40 high-risk requests: half denied, half left pending.
        for i in 0..40 {
            let id = s.request(RefundReasonCode::Unauthorized);
            if i % 2 == 0 {
                s.client
                    .reject_refund(&s.admin, &id, &String::from_str(&s.env, "spam"));
            }
        }
        assert_eq!(s.client.get_merchant_risk_window(&s.merchant).refund_count, 0);
        assert_eq!(s.status(), MerchantStatus::Active);
    }

    #[test]
    fn warning_ratio_emits_an_event_without_changing_status() {
        let s = shop();
        // 20 low-risk refunds (DuplicateCharge is not high-risk)...
        for i in 0..20 {
            let reason = if i < 4 {
                RefundReasonCode::DuplicateCharge
            } else {
                RefundReasonCode::CustomerRequest
            };
            s.substantiate(reason);
        }
        // ...then 5 high-risk: 5 / 25 = 20%, the warning ratio.
        for _ in 0..5 {
            s.substantiate(RefundReasonCode::Unauthorized);
        }
        // Read the events before any other invocation replaces them.
        assert_eq!(s.throttle_events().last(), Some((2_000, true)));
        assert_eq!(s.status(), MerchantStatus::Active);
    }

    #[test]
    fn counters_reset_when_the_30_day_window_rolls_over() {
        let s = shop();
        for _ in 0..10 {
            s.substantiate(RefundReasonCode::Unauthorized);
        }
        s.env
            .ledger()
            .set_timestamp(s.env.ledger().timestamp() + MERCHANT_RISK_WINDOW_SECS);
        for _ in 0..10 {
            s.substantiate(RefundReasonCode::Unauthorized);
        }
        // Only the new window's 10 count: below the floor, still active.
        assert_eq!(s.client.get_merchant_risk_window(&s.merchant).refund_count, 10);
        assert_eq!(s.status(), MerchantStatus::Active);
    }

    #[test]
    fn thresholds_are_validated_and_admin_only() {
        let s = shop();
        let bad = MerchantRiskThresholds {
            warning_ratio_bps: 5_000,
            restricted_ratio_bps: 4_000,
            suspended_ratio_bps: 7_000,
            min_sample_tx_count: 20,
        };
        assert!(s.client.try_set_merchant_risk_thresholds(&s.admin, &bad).is_err());
        let ok = MerchantRiskThresholds {
            warning_ratio_bps: 1_000,
            ..bad
        };
        assert_eq!(
            s.client
                .try_set_merchant_risk_thresholds(&Address::generate(&s.env), &ok),
            Err(Ok(Error::Core(CoreError::Unauthorized)))
        );
        s.client.set_merchant_risk_thresholds(&s.admin, &ok);
    }
}
