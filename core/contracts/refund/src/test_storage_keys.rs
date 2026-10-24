#![cfg(test)]

//! Issue #86 — storage key collision prevention audit.
//!
//! Soroban `#[contracttype]` key enums serialize to
//! `Vec[Symbol("<variant name>"), fields..]`. The discriminator is therefore the
//! *variant name*, not the variant's position in the enum, and every key enum
//! used by this contract shares the same `instance()` storage namespace. Two
//! variants that share a name in two different enums therefore alias the exact
//! same storage slot, which corrupts state silently.
//!
//! These tests exhaustively enumerate every variant of every key enum used by
//! `RefundContract` and assert that each one produces a unique serialized XDR
//! representation. `EXPECTED_*_VARIANTS` constants make the enumeration drift
//! visible: adding, removing or renaming a variant without updating the lists
//! below fails the suite.

use super::*;
use soroban_sdk::{
    testutils::Address as _,
    xdr::{FromXdr, ToXdr},
    Address, Env, Symbol, TryFromVal, Val,
};

/// Number of variants in each key enum. Update alongside the enumerations.
const EXPECTED_DATA_KEY_VARIANTS: usize = 35;
const EXPECTED_ARBITRATION_KEY_VARIANTS: usize = 17;
const EXPECTED_POLICY_KEY_VARIANTS: usize = 4;
const EXPECTED_SYSTEM_KEY_VARIANTS: usize = 31;
const EXPECTED_EVIDENCE_KEY_VARIANTS: usize = 3;
const EXPECTED_VOUCHER_KEY_VARIANTS: usize = 5;
const EXPECTED_TOKEN_KEY_VARIANTS: usize = 3;
const EXPECTED_REFUND_EXT_KEY_VARIANTS: usize = 5;
const EXPECTED_ELIGIBILITY_KEY_VARIANTS: usize = 3;

/// Asserts that every entry in `keys` has a distinct serialized XDR form.
///
/// Returns the number of keys checked so callers can cross-check it against the
/// expected variant counts.
fn assert_unique_keys(env: &Env, namespace: &str, keys: &[(&str, Bytes)]) -> usize {
    let mut seen: std::vec::Vec<(&str, Bytes)> = std::vec::Vec::new();
    for (label, xdr) in keys {
        if let Some((other, _)) = seen.iter().find(|(_, b)| b == xdr) {
            panic!(
                "storage key collision in {namespace}: `{other}` and `{label}` both serialize to the same XDR"
            );
        }
        seen.push((label, xdr.clone()));
    }
    seen.len()
}

fn data_keys(env: &Env, a: &Address, b: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    std::vec![
        ("DataKey::Admin", DataKey::Admin.clone().to_xdr(env)),
        ("DataKey::Refund(1)", DataKey::Refund(1).to_xdr(env)),
        ("DataKey::RefundCounter", DataKey::RefundCounter.to_xdr(env),),
        (
            "DataKey::RefundsByStatus",
            DataKey::RefundsByStatus(RefundStatus::Requested, 0).to_xdr(env),
        ),
        (
            "DataKey::RefundStatusCount",
            DataKey::RefundStatusCount(RefundStatus::Approved).to_xdr(env),
        ),
        (
            "DataKey::RefundStatusIndex",
            DataKey::RefundStatusIndex(0).to_xdr(env),
        ),
        (
            "DataKey::MerchantRefunds",
            DataKey::MerchantRefunds(a.clone(), 0).to_xdr(env),
        ),
        (
            "DataKey::MerchantRefundQuota",
            DataKey::MerchantRefundQuota(a.clone()).to_xdr(env),
        ),
        (
            "DataKey::MerchantRefundCount",
            DataKey::MerchantRefundCount(a.clone()).to_xdr(env),
        ),
        (
            "DataKey::CustomerRefunds",
            DataKey::CustomerRefunds(b.clone(), 0).to_xdr(env),
        ),
        (
            "DataKey::CustomerRefundCount",
            DataKey::CustomerRefundCount(b.clone()).to_xdr(env),
        ),
        (
            "DataKey::CustomerRefundHistoryStart",
            DataKey::CustomerRefundHistoryStart(b.clone()).to_xdr(env),
        ),
        (
            "DataKey::CustomerRefundsArchive",
            DataKey::CustomerRefundsArchive(b.clone(), 0).to_xdr(env),
        ),
        (
            "DataKey::PaymentRefunds",
            DataKey::PaymentRefunds(1, 0).to_xdr(env),
        ),
        (
            "DataKey::PaymentRefundCount",
            DataKey::PaymentRefundCount(1).to_xdr(env),
        ),
        ("DataKey::PoolToken", DataKey::PoolToken(1).to_xdr(env)),
        (
            "DataKey::DefaultRefundPolicy",
            DataKey::DefaultRefundPolicy.to_xdr(env),
        ),
        (
            "DataKey::RefundPolicy",
            DataKey::RefundPolicy(a.clone()).to_xdr(env),
        ),
        (
            "DataKey::RefundPolicyTemplate",
            DataKey::RefundPolicyTemplate(1).to_xdr(env),
        ),
        (
            "DataKey::RefundPolicyTemplateCount",
            DataKey::RefundPolicyTemplateCount.to_xdr(env),
        ),
        (
            "DataKey::PaymentContractAddress",
            DataKey::PaymentContractAddress.to_xdr(env),
        ),
        (
            "DataKey::BatchRefundLimit",
            DataKey::BatchRefundLimit.to_xdr(env),
        ),
        (
            "DataKey::RefundAnalyticsKey",
            DataKey::RefundAnalyticsKey.to_xdr(env),
        ),
        (
            "DataKey::CustomerRefundRateLimit",
            DataKey::CustomerRefundRateLimit(b.clone()).to_xdr(env),
        ),
        (
            "DataKey::GlobalRefundRateLimit",
            DataKey::GlobalRefundRateLimit.to_xdr(env),
        ),
        (
            "DataKey::AdminOverrideHistory",
            DataKey::AdminOverrideHistory(1).to_xdr(env),
        ),
        (
            "DataKey::AdminOverrideHistoryCount",
            DataKey::AdminOverrideHistoryCount.to_xdr(env),
        ),
        (
            "DataKey::PaymentRefundCap",
            DataKey::PaymentRefundCap(1).to_xdr(env),
        ),
        (
            "DataKey::PaymentRefundUsage",
            DataKey::PaymentRefundUsage(1).to_xdr(env),
        ),
        (
            "DataKey::AutoApproveBelowCeiling",
            DataKey::AutoApproveBelowCeiling.to_xdr(env),
        ),
        (
            "DataKey::CustomerTier",
            DataKey::CustomerTier(b.clone()).to_xdr(env),
        ),
        (
            "DataKey::CustomerTierPolicy",
            DataKey::CustomerTierPolicy(b.clone(), 1).to_xdr(env),
        ),
        (
            "DataKey::StrictTierPolicy",
            DataKey::StrictTierPolicy(a.clone()).to_xdr(env),
        ),
        (
            "DataKey::AppealWindowSeconds",
            DataKey::AppealWindowSeconds.to_xdr(env),
        ),
        ("DataKey::PendingAdmin", DataKey::PendingAdmin.to_xdr(env)),
    ]
}

fn arbitration_keys(env: &Env, a: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    std::vec![
        (
            "ArbitrationKey::ArbitrationCase",
            ArbitrationKey::ArbitrationCase(1).to_xdr(env),
        ),
        (
            "ArbitrationKey::ArbitrationCounter",
            ArbitrationKey::ArbitrationCounter.to_xdr(env),
        ),
        (
            "ArbitrationKey::ArbitratorList",
            ArbitrationKey::ArbitratorList.to_xdr(env),
        ),
        (
            "ArbitrationKey::ArbitratorsVoted",
            ArbitrationKey::ArbitratorsVoted(1).to_xdr(env),
        ),
        (
            "ArbitrationKey::ArbitratorVote",
            ArbitrationKey::ArbitratorVote(1, a.clone()).to_xdr(env),
        ),
        (
            "ArbitrationKey::ArbitrationFeeConfig",
            ArbitrationKey::ArbitrationFeeConfig.to_xdr(env),
        ),
        (
            "ArbitrationKey::AccumulatedTreasuryFees",
            ArbitrationKey::AccumulatedTreasuryFees.to_xdr(env),
        ),
        (
            "ArbitrationKey::ArbitrationStakeConfig",
            ArbitrationKey::ArbitrationStakeConfig.to_xdr(env),
        ),
        (
            "ArbitrationKey::ArbitrationStake",
            ArbitrationKey::ArbitrationStake(1).to_xdr(env),
        ),
        (
            "ArbitrationKey::ArbitratorReputation",
            ArbitrationKey::ArbitratorReputation(a.clone()).to_xdr(env),
        ),
        (
            "ArbitrationKey::ArbitratorScoreIndex",
            ArbitrationKey::ArbitratorScoreIndex(1, 0).to_xdr(env),
        ),
        (
            "ArbitrationKey::ArbitratorScoreCount",
            ArbitrationKey::ArbitratorScoreCount.to_xdr(env),
        ),
        (
            "ArbitrationKey::ArbitrationTimeoutConfig",
            ArbitrationKey::ArbitrationTimeoutConfig.to_xdr(env),
        ),
        (
            "ArbitrationKey::SeniorArbitratorList",
            ArbitrationKey::SeniorArbitratorList.to_xdr(env),
        ),
        (
            "ArbitrationKey::ArbitrationTierConfig",
            ArbitrationKey::ArbitrationTierConfig.to_xdr(env),
        ),
        (
            "ArbitrationKey::CaseEscalated",
            ArbitrationKey::CaseEscalated(1).to_xdr(env),
        ),
        (
            "ArbitrationKey::CaseByRefund",
            ArbitrationKey::CaseByRefund(1).to_xdr(env),
        ),
    ]
}

fn policy_keys(env: &Env, a: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    std::vec![
        (
            "PolicyKey::RefundPolicyVersion",
            PolicyKey::RefundPolicyVersion(a.clone(), 1).to_xdr(env),
        ),
        (
            "PolicyKey::RefundPolicyVersionCount",
            PolicyKey::RefundPolicyVersionCount(a.clone()).to_xdr(env),
        ),
        (
            "PolicyKey::AutoRefundTrigger",
            PolicyKey::AutoRefundTrigger(1).to_xdr(env),
        ),
        (
            "PolicyKey::AutoRefundTriggerCounter",
            PolicyKey::AutoRefundTriggerCounter.to_xdr(env),
        ),
    ]
}

fn system_keys(env: &Env, a: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    std::vec![
        (
            "SystemKey::PauseStateKey",
            SystemKey::PauseStateKey.to_xdr(env),
        ),
        (
            "SystemKey::PauseHistoryEntry",
            SystemKey::PauseHistoryEntry(1).to_xdr(env),
        ),
        (
            "SystemKey::PauseHistoryCount",
            SystemKey::PauseHistoryCount.to_xdr(env),
        ),
        (
            "SystemKey::CircuitBreakerConfigKey",
            SystemKey::CircuitBreakerConfigKey.to_xdr(env),
        ),
        (
            "SystemKey::CircuitBreakerStateKey",
            SystemKey::CircuitBreakerStateKey.to_xdr(env),
        ),
        ("SystemKey::WindowStart", SystemKey::WindowStart.to_xdr(env),),
        (
            "SystemKey::WindowRefundVolume",
            SystemKey::WindowRefundVolume.to_xdr(env),
        ),
        (
            "SystemKey::WindowPaymentVolume",
            SystemKey::WindowPaymentVolume.to_xdr(env),
        ),
        (
            "SystemKey::FraudSignal",
            SystemKey::FraudSignal(a.clone()).to_xdr(env),
        ),
        ("SystemKey::FraudConfig", SystemKey::FraudConfig.to_xdr(env)),
        (
            "SystemKey::FlaggedAddressesIndex",
            SystemKey::FlaggedAddressesIndex.to_xdr(env),
        ),
        (
            "SystemKey::FlaggedAddress",
            SystemKey::FlaggedAddress(1).to_xdr(env),
        ),
        (
            "SystemKey::RefundRejectedAt",
            SystemKey::RefundRejectedAt(1).to_xdr(env),
        ),
        ("SystemKey::Appeal", SystemKey::Appeal(1).to_xdr(env)),
        (
            "SystemKey::AppealCounter",
            SystemKey::AppealCounter.to_xdr(env),
        ),
        (
            "SystemKey::AppealByRefund",
            SystemKey::AppealByRefund(1).to_xdr(env),
        ),
        (
            "SystemKey::AppealByCustomer",
            SystemKey::AppealByCustomer(a.clone(), 0).to_xdr(env),
        ),
        (
            "SystemKey::AppealByCustomerCount",
            SystemKey::AppealByCustomerCount(a.clone()).to_xdr(env),
        ),
        (
            "SystemKey::NotificationHook",
            SystemKey::NotificationHook(1).to_xdr(env),
        ),
        (
            "SystemKey::NotificationHookCounter",
            SystemKey::NotificationHookCounter.to_xdr(env),
        ),
        (
            "SystemKey::HooksByEvent",
            SystemKey::HooksByEvent(RefundEventType::Processed, 0).to_xdr(env),
        ),
        (
            "SystemKey::HooksByEventCount",
            SystemKey::HooksByEventCount(RefundEventType::Processed).to_xdr(env),
        ),
        (
            "SystemKey::SubscriberHooks",
            SystemKey::SubscriberHooks(a.clone(), 0).to_xdr(env),
        ),
        (
            "SystemKey::SubscriberHookCount",
            SystemKey::SubscriberHookCount(a.clone()).to_xdr(env),
        ),
        (
            "SystemKey::RefundFeeConfig",
            SystemKey::RefundFeeConfig.to_xdr(env),
        ),
        (
            "SystemKey::AccumulatedRefundFees",
            SystemKey::AccumulatedRefundFees.to_xdr(env),
        ),
        (
            "SystemKey::CustomerRefundCooldown",
            SystemKey::CustomerRefundCooldown(a.clone()).to_xdr(env),
        ),
        (
            "SystemKey::RefundCooldownConfig",
            SystemKey::RefundCooldownConfig.to_xdr(env),
        ),
        (
            "SystemKey::SchemaVersion",
            SystemKey::SchemaVersion.to_xdr(env),
        ),
        (
            "SystemKey::AnalyticsCache",
            SystemKey::AnalyticsCache(1, 2).to_xdr(env),
        ),
        (
            "SystemKey::AnalyticsCacheWindows",
            SystemKey::AnalyticsCacheWindows.to_xdr(env),
        ),
    ]
}

fn evidence_keys(env: &Env, a: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    std::vec![
        (
            "EvidenceKey::Evidence",
            EvidenceKey::Evidence(1, a.clone()).to_xdr(env),
        ),
        (
            "EvidenceKey::EvidenceIndex",
            EvidenceKey::EvidenceIndex(1, 0).to_xdr(env),
        ),
        (
            "EvidenceKey::EvidenceCount",
            EvidenceKey::EvidenceCount(1).to_xdr(env),
        ),
    ]
}

fn voucher_keys(env: &Env, a: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    std::vec![
        ("VoucherKey::Voucher", VoucherKey::Voucher(1).to_xdr(env)),
        (
            "VoucherKey::VoucherCounter",
            VoucherKey::VoucherCounter.to_xdr(env),
        ),
        (
            "VoucherKey::CustomerVoucher",
            VoucherKey::CustomerVoucher(a.clone(), 0).to_xdr(env),
        ),
        (
            "VoucherKey::CustomerVoucherCount",
            VoucherKey::CustomerVoucherCount(a.clone()).to_xdr(env),
        ),
        (
            "VoucherKey::RefundVoucherIssued",
            VoucherKey::RefundVoucherIssued(1).to_xdr(env),
        ),
    ]
}

fn token_keys(env: &Env, a: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    std::vec![
        (
            "TokenKey::SupportedToken",
            TokenKey::SupportedToken(a.clone()).to_xdr(env),
        ),
        ("TokenKey::TokenCount", TokenKey::TokenCount.to_xdr(env)),
        (
            "TokenKey::TokenByIndex",
            TokenKey::TokenByIndex(1).to_xdr(env),
        ),
    ]
}

fn refund_ext_keys(env: &Env, a: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    std::vec![
        (
            "RefundExtKey::CategoryWindow",
            RefundExtKey::CategoryWindow(a.clone(), 1).to_xdr(env),
        ),
        (
            "RefundExtKey::PaymentCategoryTag",
            RefundExtKey::PaymentCategoryTag(1).to_xdr(env),
        ),
        (
            "RefundExtKey::AssignmentConfig",
            RefundExtKey::AssignmentConfig.to_xdr(env),
        ),
        (
            "RefundExtKey::RotationIndex",
            RefundExtKey::RotationIndex.to_xdr(env),
        ),
        (
            "RefundExtKey::RefundTTLConfig",
            RefundExtKey::RefundTTLConfig.to_xdr(env),
        ),
    ]
}

fn eligibility_keys(env: &Env, a: &Address, b: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    std::vec![
        (
            "EligibilityKey::Entry",
            EligibilityKey::Entry(a.clone(), b.clone()).to_xdr(env),
        ),
        (
            "EligibilityKey::MerchantCustomerIndex",
            EligibilityKey::MerchantCustomerIndex(a.clone(), 0).to_xdr(env),
        ),
        (
            "EligibilityKey::MerchantCustomerCount",
            EligibilityKey::MerchantCustomerCount(a.clone()).to_xdr(env),
        ),
    ]
}

/// Every key the refund contract can write, in one flat namespace.
///
/// All refund key enums are written straight to `env.storage().instance()`, so
/// this list doubles as the contract-wide collision check.
fn all_keys(env: &Env, a: &Address, b: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    let mut all = data_keys(env, a, b);
    all.extend(arbitration_keys(env, a));
    all.extend(policy_keys(env, a));
    all.extend(system_keys(env, a));
    all.extend(evidence_keys(env, a));
    all.extend(voucher_keys(env, a));
    all.extend(token_keys(env, a));
    all.extend(refund_ext_keys(env, a));
    all.extend(eligibility_keys(env, a, b));
    all
}

#[test]
fn test_data_key_variants_are_unique() {
    let env = Env::default();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let keys = data_keys(&env, &a, &b);
    assert_eq!(keys.len(), EXPECTED_DATA_KEY_VARIANTS);
    assert_unique_keys(&env, "DataKey", &keys);
}

#[test]
fn test_arbitration_key_variants_are_unique() {
    let env = Env::default();
    let a = Address::generate(&env);
    let keys = arbitration_keys(&env, &a);
    assert_eq!(keys.len(), EXPECTED_ARBITRATION_KEY_VARIANTS);
    assert_unique_keys(&env, "ArbitrationKey", &keys);
}

#[test]
fn test_policy_key_variants_are_unique() {
    let env = Env::default();
    let a = Address::generate(&env);
    let keys = policy_keys(&env, &a);
    assert_eq!(keys.len(), EXPECTED_POLICY_KEY_VARIANTS);
    assert_unique_keys(&env, "PolicyKey", &keys);
}

#[test]
fn test_system_key_variants_are_unique() {
    let env = Env::default();
    let a = Address::generate(&env);
    let keys = system_keys(&env, &a);
    assert_eq!(keys.len(), EXPECTED_SYSTEM_KEY_VARIANTS);
    assert_unique_keys(&env, "SystemKey", &keys);
}

#[test]
fn test_evidence_key_variants_are_unique() {
    let env = Env::default();
    let a = Address::generate(&env);
    let keys = evidence_keys(&env, &a);
    assert_eq!(keys.len(), EXPECTED_EVIDENCE_KEY_VARIANTS);
    assert_unique_keys(&env, "EvidenceKey", &keys);
}

#[test]
fn test_voucher_key_variants_are_unique() {
    let env = Env::default();
    let a = Address::generate(&env);
    let keys = voucher_keys(&env, &a);
    assert_eq!(keys.len(), EXPECTED_VOUCHER_KEY_VARIANTS);
    assert_unique_keys(&env, "VoucherKey", &keys);
}

#[test]
fn test_token_key_variants_are_unique() {
    let env = Env::default();
    let a = Address::generate(&env);
    let keys = token_keys(&env, &a);
    assert_eq!(keys.len(), EXPECTED_TOKEN_KEY_VARIANTS);
    assert_unique_keys(&env, "TokenKey", &keys);
}

#[test]
fn test_refund_ext_key_variants_are_unique() {
    let env = Env::default();
    let a = Address::generate(&env);
    let keys = refund_ext_keys(&env, &a);
    assert_eq!(keys.len(), EXPECTED_REFUND_EXT_KEY_VARIANTS);
    assert_unique_keys(&env, "RefundExtKey", &keys);
}

#[test]
fn test_eligibility_key_variants_are_unique() {
    let env = Env::default();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let keys = eligibility_keys(&env, &a, &b);
    assert_eq!(keys.len(), EXPECTED_ELIGIBILITY_KEY_VARIANTS);
    assert_unique_keys(&env, "EligibilityKey", &keys);
}

/// The headline audit: no two keys anywhere in the refund contract's instance
/// storage may serialize to the same bytes. This is the check that caught the
/// `DataKey::RefundPolicyVersion` / `PolicyKey::RefundPolicyVersion` alias.
#[test]
fn test_no_storage_key_collisions_across_all_key_enums() {
    let env = Env::default();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let keys = all_keys(&env, &a, &b);
    assert_eq!(
        keys.len(),
        EXPECTED_DATA_KEY_VARIANTS
            + EXPECTED_ARBITRATION_KEY_VARIANTS
            + EXPECTED_POLICY_KEY_VARIANTS
            + EXPECTED_SYSTEM_KEY_VARIANTS
            + EXPECTED_EVIDENCE_KEY_VARIANTS
            + EXPECTED_VOUCHER_KEY_VARIANTS
            + EXPECTED_TOKEN_KEY_VARIANTS
            + EXPECTED_REFUND_EXT_KEY_VARIANTS
            + EXPECTED_ELIGIBILITY_KEY_VARIANTS
    );
    assert_unique_keys(&env, "RefundContract", &keys);
}

/// Regression guard for the specific issue #86 collision: writing a versioned
/// policy through `PolicyKey` must not be observable through any other key, and
/// `store_refund_policy` / `set_refund_policy` must keep sharing one version
/// sequence (they read/write the same slots by design, now via a single owner).
#[test]
fn test_versioned_policy_namespace_is_owned_by_policy_key() {
    let env = Env::default();
    let merchant = Address::generate(&env);
    let key = PolicyKey::RefundPolicyVersion(merchant.clone(), 1);
    let xdr = key.clone().to_xdr(&env);
    assert_ne!(
        xdr,
        PolicyKey::RefundPolicyVersionCount(merchant.clone()).to_xdr(&env)
    );
    assert_ne!(xdr, DataKey::RefundPolicy(merchant.clone()).to_xdr(&env));
    assert_ne!(xdr, DataKey::Admin.to_xdr(&env));

    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);

    let tiers = soroban_sdk::vec![
        &env,
        RefundTier {
            days_from_purchase: 0,
            max_refund_bps: 5_000,
        }
    ];
    client.set_refund_policy(&merchant, &tiers);
    client.set_refund_policy(&merchant, &tiers);

    // Two writes through the single owner produce version 1 and 2.
    assert!(client.get_refund_policy_version(&merchant, &1).is_some());
    assert!(client.get_refund_policy_version(&merchant, &2).is_some());
    assert!(client.get_refund_policy_version(&merchant, &3).is_none());
    assert_eq!(client.get_refund_policy_history(&merchant).len(), 2);

    // The counter lives in its own slot and is not confused with any other key.
    let count: u32 = env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .get(&PolicyKey::RefundPolicyVersionCount(merchant.clone()))
            .unwrap()
    });
    assert_eq!(count, 2);
    let missing: Option<RefundPolicyVersion> = env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .get(&PolicyKey::RefundPolicyVersion(merchant.clone(), 3))
    });
    assert!(missing.is_none());
}

/// A representative write/read pair proving that distinct keys really do hold
/// distinct values in live storage, not just distinct byte encodings.
#[test]
fn test_distinct_keys_hold_distinct_values() {
    let env = Env::default();
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let contract_id = env.register(RefundContract, ());
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&DataKey::RefundCounter, &7u64);
        env.storage()
            .instance()
            .set(&ArbitrationKey::ArbitrationCounter, &9u64);
        env.storage()
            .instance()
            .set(&PolicyKey::AutoRefundTriggerCounter, &11u64);
        env.storage()
            .instance()
            .set(&SystemKey::AppealCounter, &13u64);
        env.storage()
            .instance()
            .set(&VoucherKey::VoucherCounter, &15u64);
        env.storage().instance().set(&TokenKey::TokenCount, &17u64);
        env.storage().instance().set(&DataKey::Admin, &merchant);
        env.storage()
            .instance()
            .set(&DataKey::PendingAdmin, &customer);
    });

    let (counter, arbitration, triggers, appeals, vouchers, tokens) =
        env.as_contract(&contract_id, || {
            (
                env.storage()
                    .instance()
                    .get::<DataKey, u64>(&DataKey::RefundCounter)
                    .unwrap(),
                env.storage()
                    .instance()
                    .get::<ArbitrationKey, u64>(&ArbitrationKey::ArbitrationCounter)
                    .unwrap(),
                env.storage()
                    .instance()
                    .get::<PolicyKey, u64>(&PolicyKey::AutoRefundTriggerCounter)
                    .unwrap(),
                env.storage()
                    .instance()
                    .get::<SystemKey, u64>(&SystemKey::AppealCounter)
                    .unwrap(),
                env.storage()
                    .instance()
                    .get::<VoucherKey, u64>(&VoucherKey::VoucherCounter)
                    .unwrap(),
                env.storage()
                    .instance()
                    .get::<TokenKey, u64>(&TokenKey::TokenCount)
                    .unwrap(),
            )
        });

    assert_eq!(counter, 7);
    assert_eq!(arbitration, 9);
    assert_eq!(triggers, 11);
    assert_eq!(appeals, 13);
    assert_eq!(vouchers, 15);
    assert_eq!(tokens, 17);

    // Address-bearing keys are equally isolated.
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::CustomerRefundCount(merchant.clone()), &3u64);
        env.storage()
            .instance()
            .set(&SystemKey::FraudSignal(merchant.clone()), &4u64);
        env.storage().instance().set(&DataKey::Refund(3), &5u64);
    });
    let (cust, fraud, refund) = env.as_contract(&contract_id, || {
        (
            env.storage()
                .instance()
                .get::<DataKey, u64>(&DataKey::CustomerRefundCount(merchant.clone()))
                .unwrap(),
            env.storage()
                .instance()
                .get::<SystemKey, u64>(&SystemKey::FraudSignal(merchant.clone()))
                .unwrap(),
            env.storage()
                .instance()
                .get::<DataKey, u64>(&DataKey::Refund(3))
                .unwrap(),
        )
    });
    assert_eq!(cust, 3);
    assert_eq!(fraud, 4);
    assert_eq!(refund, 5);
}

/// The serialized form of a key is `Vec[Symbol(variant_name), fields..]`, so the
/// *name* — not the position in the enum — is the on-chain identifier. Assert
/// the leading symbol explicitly so a rename is caught as a loud test failure
/// instead of silently orphaning already-deployed state.
#[test]
fn test_key_encoding_is_keyed_on_variant_name() {
    let env = Env::default();
    let a = Address::generate(&env);

    let leading_symbol = |xdr: Bytes| -> Symbol {
        let val: Val = Val::from_xdr(&env, &xdr).expect("key decodes to a Val");
        let outer = <soroban_sdk::Vec<Val> as TryFromVal<Env, Val>>::try_from_val(&env, &val)
            .expect("key encoding is a ScVal vec");
        Symbol::try_from_val(&env, &outer.get(0).expect("key vec is non-empty"))
            .expect("key discriminant is a symbol")
    };

    assert_eq!(
        leading_symbol(PolicyKey::RefundPolicyVersion(a.clone(), 1).to_xdr(&env)),
        Symbol::new(&env, "RefundPolicyVersion")
    );
    assert_eq!(
        leading_symbol(PolicyKey::RefundPolicyVersionCount(a.clone()).to_xdr(&env)),
        Symbol::new(&env, "RefundPolicyVersionCount")
    );
    assert_eq!(
        leading_symbol(DataKey::Admin.to_xdr(&env)),
        Symbol::new(&env, "Admin")
    );
    assert_eq!(
        leading_symbol(DataKey::PendingAdmin.to_xdr(&env)),
        Symbol::new(&env, "PendingAdmin")
    );
    assert_eq!(
        leading_symbol(ArbitrationKey::CaseByRefund(1).to_xdr(&env)),
        Symbol::new(&env, "CaseByRefund")
    );
    assert_eq!(
        leading_symbol(SystemKey::AnalyticsCache(1, 2).to_xdr(&env)),
        Symbol::new(&env, "AnalyticsCache")
    );

    // The versioned-policy namespace is spelled exactly once across every enum
    // the contract writes (issue #86 regression guard).
    let versioned = PolicyKey::RefundPolicyVersion(a.clone(), 1).to_xdr(&env);
    let versioned_count = PolicyKey::RefundPolicyVersionCount(a.clone()).to_xdr(&env);
    for (label, key) in all_keys(&env, &a, &a) {
        let xdr = key;
        assert!(
            xdr != versioned || label == "PolicyKey::RefundPolicyVersion",
            "`{label}` aliases the versioned-policy slot"
        );
        assert!(
            xdr != versioned_count || label == "PolicyKey::RefundPolicyVersionCount",
            "`{label}` aliases the versioned-policy counter slot"
        );
    }
}
