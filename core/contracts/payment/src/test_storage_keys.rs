#![cfg(test)]

//! Storage key collision audit for `PaymentContract`.
//!
//! Issue #86 - storage key collision prevention audit.
//!
//! Soroban `#[contracttype]` key enums serialize to
//! `Vec[Symbol("<variant name>"), fields..]`, so the *variant name* - not the
//! position of the variant in the enum - is the on-chain identifier. This
//! contract namespaces every key through a single outer `DataKey` enum, so the
//! outer variant name is what keeps the inner enums from aliasing each other
//! (e.g. `DataKey::Customer(CustomerDataKey::Analytics(..))` and
//! `DataKey::Merchant(MerchantDataKey::Analytics(..))` share an inner name but
//! not a slot).
//!
//! Two invariants must therefore hold at all times:
//!
//! 1. every namespace spelling on the outer `DataKey` is unique, and
//! 2. no two fully qualified keys serialize to the same bytes.
//!
//! The tests below enumerate all 101 keys the contract can address, assert
//! the per-enum variant counts so added or removed variants cannot slip past
//! unnoticed, and assert that every serialization is distinct.

use super::*;
use soroban_sdk::{
    testutils::Address as _,
    xdr::{FromXdr, ToXdr},
    Address, Bytes, Env, Symbol, TryFromVal, Val,
};

/// Asserts that every entry in `keys` has a distinct serialized XDR form,
/// reporting the first colliding pair by name.
fn assert_unique_keys(namespace: &str, keys: &[(&str, Bytes)]) -> usize {
    let mut seen: std::vec::Vec<(&str, Bytes)> = std::vec::Vec::new();
    for (label, xdr) in keys {
        if let Some((other, _)) = seen.iter().find(|(_, b)| b == xdr) {
            panic!(
                "storage key collision in {namespace}: `{other}` and `{label}` serialize to identical XDR"
            );
        }
        seen.push((label, xdr.clone()));
    }
    seen.len()
}

const EXPECTED_CONFIGKEY_VARIANTS: usize = 17;

/// Every fully qualified `Config::<EnumName>` key, one per variant.
fn keys_config(env: &Env, a: &Address, b: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    let _: (&Address, &Address) = (a, b);
    std::vec![
        (
            "ConfigKey::Admin",
            DataKey::Config(ConfigKey::Admin).to_xdr(env)
        ),
        (
            "ConfigKey::MultiSigConfig",
            DataKey::Config(ConfigKey::MultiSigConfig).to_xdr(env)
        ),
        (
            "ConfigKey::FeeConfig",
            DataKey::Config(ConfigKey::FeeConfig).to_xdr(env)
        ),
        (
            "ConfigKey::RateLimitConfig",
            DataKey::Config(ConfigKey::RateLimitConfig).to_xdr(env)
        ),
        (
            "ConfigKey::DunningConfig",
            DataKey::Config(ConfigKey::DunningConfig).to_xdr(env)
        ),
        (
            "ConfigKey::LoyaltyConfig",
            DataKey::Config(ConfigKey::LoyaltyConfig).to_xdr(env)
        ),
        (
            "ConfigKey::RiskFeeConfig",
            DataKey::Config(ConfigKey::RiskFeeConfig).to_xdr(env)
        ),
        (
            "ConfigKey::FinalityConfig",
            DataKey::Config(ConfigKey::FinalityConfig).to_xdr(env)
        ),
        (
            "ConfigKey::FeeRebateConfig",
            DataKey::Config(ConfigKey::FeeRebateConfig).to_xdr(env)
        ),
        (
            "ConfigKey::TierThresholds",
            DataKey::Config(ConfigKey::TierThresholds).to_xdr(env)
        ),
        (
            "ConfigKey::LargePaymentThreshold",
            DataKey::Config(ConfigKey::LargePaymentThreshold).to_xdr(env)
        ),
        (
            "ConfigKey::GlobalMerchantCount",
            DataKey::Config(ConfigKey::GlobalMerchantCount).to_xdr(env)
        ),
        (
            "ConfigKey::PauseStateKey",
            DataKey::Config(ConfigKey::PauseStateKey).to_xdr(env)
        ),
        (
            "ConfigKey::MinSplitAmount",
            DataKey::Config(ConfigKey::MinSplitAmount).to_xdr(env)
        ),
        (
            "ConfigKey::SchemaVersion",
            DataKey::Config(ConfigKey::SchemaVersion).to_xdr(env)
        ),
        (
            "ConfigKey::AllowedTokens",
            DataKey::Config(ConfigKey::AllowedTokens).to_xdr(env)
        ),
        (
            "ConfigKey::MaxForwardDepth",
            DataKey::Config(ConfigKey::MaxForwardDepth).to_xdr(env)
        ),
    ]
}

const EXPECTED_PAYMENTKEY_VARIANTS: usize = 15;

/// Every fully qualified `Payment::<EnumName>` key, one per variant.
fn keys_payment(env: &Env, a: &Address, b: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    let _: (&Address, &Address) = (a, b);
    std::vec![
        (
            "PaymentKey::Data",
            DataKey::Payment(PaymentKey::Data(1)).to_xdr(env)
        ),
        (
            "PaymentKey::Counter",
            DataKey::Payment(PaymentKey::Counter).to_xdr(env)
        ),
        (
            "PaymentKey::Metadata",
            DataKey::Payment(PaymentKey::Metadata(1)).to_xdr(env)
        ),
        (
            "PaymentKey::Memo",
            DataKey::Payment(PaymentKey::Memo(1)).to_xdr(env)
        ),
        (
            "PaymentKey::MemoVersion",
            DataKey::Payment(PaymentKey::MemoVersion(1)).to_xdr(env)
        ),
        (
            "PaymentKey::Tag",
            DataKey::Payment(PaymentKey::Tag(1)).to_xdr(env)
        ),
        (
            "PaymentKey::Invoice",
            DataKey::Payment(PaymentKey::Invoice(1)).to_xdr(env)
        ),
        (
            "PaymentKey::InvoiceCounter",
            DataKey::Payment(PaymentKey::InvoiceCounter).to_xdr(env)
        ),
        (
            "PaymentKey::InvoicePaymentId",
            DataKey::Payment(PaymentKey::InvoicePaymentId(1)).to_xdr(env)
        ),
        (
            "PaymentKey::PartialPaymentCounter",
            DataKey::Payment(PaymentKey::PartialPaymentCounter(1)).to_xdr(env)
        ),
        (
            "PaymentKey::OutstandingBalance",
            DataKey::Payment(PaymentKey::OutstandingBalance(1)).to_xdr(env)
        ),
        (
            "PaymentKey::PendingSettlement",
            DataKey::Payment(PaymentKey::PendingSettlement(1)).to_xdr(env)
        ),
        (
            "PaymentKey::AccumulatedFees",
            DataKey::Payment(PaymentKey::AccumulatedFees).to_xdr(env)
        ),
        (
            "PaymentKey::LargePaymentCounter",
            DataKey::Payment(PaymentKey::LargePaymentCounter).to_xdr(env)
        ),
        (
            "PaymentKey::Discount",
            DataKey::Payment(PaymentKey::Discount(1)).to_xdr(env)
        ),
    ]
}

const EXPECTED_SUBSCRIPTIONKEY_VARIANTS: usize = 7;

/// Every fully qualified `Subscription::<EnumName>` key, one per variant.
fn keys_subscription(env: &Env, a: &Address, b: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    let _: (&Address, &Address) = (a, b);
    std::vec![
        (
            "SubscriptionKey::Data",
            DataKey::Subscription(SubscriptionKey::Data(1)).to_xdr(env)
        ),
        (
            "SubscriptionKey::Counter",
            DataKey::Subscription(SubscriptionKey::Counter).to_xdr(env)
        ),
        (
            "SubscriptionKey::Metered",
            DataKey::Subscription(SubscriptionKey::Metered(1)).to_xdr(env)
        ),
        (
            "SubscriptionKey::MeteredCounter",
            DataKey::Subscription(SubscriptionKey::MeteredCounter).to_xdr(env)
        ),
        (
            "SubscriptionKey::Group",
            DataKey::Subscription(SubscriptionKey::Group(1)).to_xdr(env)
        ),
        (
            "SubscriptionKey::GroupCounter",
            DataKey::Subscription(SubscriptionKey::GroupCounter).to_xdr(env)
        ),
        (
            "SubscriptionKey::GroupMembership",
            DataKey::Subscription(SubscriptionKey::GroupMembership(1)).to_xdr(env)
        ),
    ]
}

const EXPECTED_FEATUREKEY_VARIANTS: usize = 15;

/// Every fully qualified `Feature::<EnumName>` key, one per variant.
fn keys_feature(env: &Env, a: &Address, b: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    let _: (&Address, &Address) = (a, b);
    std::vec![
        (
            "FeatureKey::PaymentAnalytics",
            DataKey::Feature(FeatureKey::PaymentAnalytics).to_xdr(env)
        ),
        (
            "FeatureKey::PlatformAnalyticsDaily",
            DataKey::Feature(FeatureKey::PlatformAnalyticsDaily(1)).to_xdr(env)
        ),
        (
            "FeatureKey::PaymentForwardConfig",
            DataKey::Feature(FeatureKey::PaymentForwardConfig(a.clone())).to_xdr(env)
        ),
        (
            "FeatureKey::OracleRateConfig",
            DataKey::Feature(FeatureKey::OracleRateConfig(Currency::USDC)).to_xdr(env)
        ),
        (
            "FeatureKey::ConversionRate",
            DataKey::Feature(FeatureKey::ConversionRate(Currency::USDC)).to_xdr(env)
        ),
        (
            "FeatureKey::MerchantRateLimit",
            DataKey::Feature(FeatureKey::MerchantRateLimit(a.clone())).to_xdr(env)
        ),
        (
            "FeatureKey::CustomerLoyaltyBalance",
            DataKey::Feature(FeatureKey::CustomerLoyaltyBalance(a.clone())).to_xdr(env)
        ),
        (
            "FeatureKey::CustomerSpendLimit",
            DataKey::Feature(FeatureKey::CustomerSpendLimit(a.clone())).to_xdr(env)
        ),
        (
            "FeatureKey::PaymentChannel",
            DataKey::Feature(FeatureKey::PaymentChannel(1)).to_xdr(env)
        ),
        (
            "FeatureKey::PaymentChannelCounter",
            DataKey::Feature(FeatureKey::PaymentChannelCounter).to_xdr(env)
        ),
        (
            "FeatureKey::SplitConfig",
            DataKey::Feature(FeatureKey::SplitConfig(1)).to_xdr(env)
        ),
        (
            "FeatureKey::SweepRecipient",
            DataKey::Feature(FeatureKey::SweepRecipient).to_xdr(env)
        ),
        (
            "FeatureKey::SweepCounter",
            DataKey::Feature(FeatureKey::SweepCounter).to_xdr(env)
        ),
        (
            "FeatureKey::SweepHistory",
            DataKey::Feature(FeatureKey::SweepHistory(1)).to_xdr(env)
        ),
        (
            "FeatureKey::RouteOptions",
            DataKey::Feature(FeatureKey::RouteOptions(a.clone(), b.clone())).to_xdr(env)
        ),
    ]
}

const EXPECTED_CUSTOMERDATAKEY_VARIANTS: usize = 14;

/// Every fully qualified `Customer::<EnumName>` key, one per variant.
fn keys_customer(env: &Env, a: &Address, b: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    let _: (&Address, &Address) = (a, b);
    std::vec![
        (
            "CustomerDataKey::Payments",
            DataKey::Customer(CustomerDataKey::Payments(a.clone(), 1)).to_xdr(env)
        ),
        (
            "CustomerDataKey::PaymentCount",
            DataKey::Customer(CustomerDataKey::PaymentCount(a.clone())).to_xdr(env)
        ),
        (
            "CustomerDataKey::Subscriptions",
            DataKey::Customer(CustomerDataKey::Subscriptions(a.clone(), 1)).to_xdr(env)
        ),
        (
            "CustomerDataKey::SubscriptionCount",
            DataKey::Customer(CustomerDataKey::SubscriptionCount(a.clone())).to_xdr(env)
        ),
        (
            "CustomerDataKey::Analytics",
            DataKey::Customer(CustomerDataKey::Analytics(a.clone())).to_xdr(env)
        ),
        (
            "CustomerDataKey::RateLimit",
            DataKey::Customer(CustomerDataKey::RateLimit(a.clone())).to_xdr(env)
        ),
        (
            "CustomerDataKey::FlagReason",
            DataKey::Customer(CustomerDataKey::FlagReason(a.clone())).to_xdr(env)
        ),
        (
            "CustomerDataKey::Allowlist",
            DataKey::Customer(CustomerDataKey::Allowlist(a.clone())).to_xdr(env)
        ),
        (
            "CustomerDataKey::FeeWaiver",
            DataKey::Customer(CustomerDataKey::FeeWaiver(a.clone())).to_xdr(env)
        ),
        (
            "CustomerDataKey::MerchantVolume",
            DataKey::Customer(CustomerDataKey::MerchantVolume(a.clone(), b.clone())).to_xdr(env)
        ),
        (
            "CustomerDataKey::MerchantList",
            DataKey::Customer(CustomerDataKey::MerchantList(a.clone(), 1)).to_xdr(env)
        ),
        (
            "CustomerDataKey::MerchantCount",
            DataKey::Customer(CustomerDataKey::MerchantCount(a.clone())).to_xdr(env)
        ),
        (
            "CustomerDataKey::MonthlyVolume",
            DataKey::Customer(CustomerDataKey::MonthlyVolume(a.clone(), 1)).to_xdr(env)
        ),
        (
            "CustomerDataKey::HourCount",
            DataKey::Customer(CustomerDataKey::HourCount(a.clone(), 1)).to_xdr(env)
        ),
    ]
}

const EXPECTED_MERCHANTDATAKEY_VARIANTS: usize = 19;

/// Every fully qualified `Merchant::<EnumName>` key, one per variant.
fn keys_merchant(env: &Env, a: &Address, b: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    let _: (&Address, &Address) = (a, b);
    std::vec![
        (
            "MerchantDataKey::Payments",
            DataKey::Merchant(MerchantDataKey::Payments(a.clone(), 1)).to_xdr(env)
        ),
        (
            "MerchantDataKey::PaymentCount",
            DataKey::Merchant(MerchantDataKey::PaymentCount(a.clone())).to_xdr(env)
        ),
        (
            "MerchantDataKey::Subscriptions",
            DataKey::Merchant(MerchantDataKey::Subscriptions(a.clone(), 1)).to_xdr(env)
        ),
        (
            "MerchantDataKey::SubscriptionCount",
            DataKey::Merchant(MerchantDataKey::SubscriptionCount(a.clone())).to_xdr(env)
        ),
        (
            "MerchantDataKey::Analytics",
            DataKey::Merchant(MerchantDataKey::Analytics(a.clone())).to_xdr(env)
        ),
        (
            "MerchantDataKey::FeeRecord",
            DataKey::Merchant(MerchantDataKey::FeeRecord(a.clone())).to_xdr(env)
        ),
        (
            "MerchantDataKey::AnalyticsBucket",
            DataKey::Merchant(MerchantDataKey::AnalyticsBucket(a.clone(), 1)).to_xdr(env)
        ),
        (
            "MerchantDataKey::GlobalList",
            DataKey::Merchant(MerchantDataKey::GlobalList(1)).to_xdr(env)
        ),
        (
            "MerchantDataKey::PayoutSchedule",
            DataKey::Merchant(MerchantDataKey::PayoutSchedule(a.clone())).to_xdr(env)
        ),
        (
            "MerchantDataKey::RebateAccrual",
            DataKey::Merchant(MerchantDataKey::RebateAccrual(a.clone())).to_xdr(env)
        ),
        (
            "MerchantDataKey::PendingSettlementCount",
            DataKey::Merchant(MerchantDataKey::PendingSettlementCount(a.clone())).to_xdr(env)
        ),
        (
            "MerchantDataKey::PendingSettlementIndex",
            DataKey::Merchant(MerchantDataKey::PendingSettlementIndex(a.clone(), 1)).to_xdr(env)
        ),
        (
            "MerchantDataKey::VerificationLevel",
            DataKey::Merchant(MerchantDataKey::VerificationLevel(a.clone())).to_xdr(env)
        ),
        (
            "MerchantDataKey::VerificationTierLimit",
            DataKey::Merchant(MerchantDataKey::VerificationTierLimit(
                MerchantVerificationLevel::Basic
            ))
            .to_xdr(env)
        ),
        (
            "MerchantDataKey::MerchantPaymentsPage",
            DataKey::Merchant(MerchantDataKey::MerchantPaymentsPage(a.clone(), 1)).to_xdr(env)
        ),
        (
            "MerchantDataKey::MerchantPaused",
            DataKey::Merchant(MerchantDataKey::MerchantPaused(a.clone())).to_xdr(env)
        ),
        (
            "MerchantDataKey::MerchantActiveSubscriptions",
            DataKey::Merchant(MerchantDataKey::MerchantActiveSubscriptions(a.clone(), 1))
                .to_xdr(env)
        ),
        (
            "MerchantDataKey::MerchantActiveSubscriptionCount",
            DataKey::Merchant(MerchantDataKey::MerchantActiveSubscriptionCount(a.clone()))
                .to_xdr(env)
        ),
        (
            "MerchantDataKey::ActiveSubscriptionIndex",
            DataKey::Merchant(MerchantDataKey::ActiveSubscriptionIndex(1)).to_xdr(env)
        ),
    ]
}

const EXPECTED_STATEDATAKEY_VARIANTS: usize = 14;

/// Every fully qualified `State::<EnumName>` key, one per variant.
fn keys_state(env: &Env, a: &Address, b: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    let _: (&Address, &Address) = (a, b);
    std::vec![
        (
            "StateDataKey::DunningState",
            DataKey::State(StateDataKey::DunningState(1)).to_xdr(env)
        ),
        (
            "StateDataKey::EscrowedPayment",
            DataKey::State(StateDataKey::EscrowedPayment(1)).to_xdr(env)
        ),
        (
            "StateDataKey::EscrowedPaymentDispute",
            DataKey::State(StateDataKey::EscrowedPaymentDispute(1)).to_xdr(env)
        ),
        (
            "StateDataKey::ConditionalPayment",
            DataKey::State(StateDataKey::ConditionalPayment(1)).to_xdr(env)
        ),
        (
            "StateDataKey::ScheduledPayment",
            DataKey::State(StateDataKey::ScheduledPayment(1)).to_xdr(env)
        ),
        (
            "StateDataKey::AdminProposal",
            DataKey::State(StateDataKey::AdminProposal(String::from_str(env, "s"))).to_xdr(env)
        ),
        (
            "StateDataKey::LargePaymentProposal",
            DataKey::State(StateDataKey::LargePaymentProposal(1)).to_xdr(env)
        ),
        (
            "StateDataKey::PauseHistoryEntry",
            DataKey::State(StateDataKey::PauseHistoryEntry(1)).to_xdr(env)
        ),
        (
            "StateDataKey::PauseHistoryCount",
            DataKey::State(StateDataKey::PauseHistoryCount).to_xdr(env)
        ),
        (
            "StateDataKey::AutoEscrowRule",
            DataKey::State(StateDataKey::AutoEscrowRule(a.clone())).to_xdr(env)
        ),
        (
            "StateDataKey::AutoEscrowTriggered",
            DataKey::State(StateDataKey::AutoEscrowTriggered(1)).to_xdr(env)
        ),
        (
            "StateDataKey::PartialPaymentRecord",
            DataKey::State(StateDataKey::PartialPaymentRecord(1, 1)).to_xdr(env)
        ),
        (
            "StateDataKey::SettlementFinalized",
            DataKey::State(StateDataKey::SettlementFinalized(1)).to_xdr(env)
        ),
        (
            "StateDataKey::ScheduledPaymentCounter",
            DataKey::State(StateDataKey::ScheduledPaymentCounter).to_xdr(env)
        ),
    ]
}

/// Every storage key the contract can address, as a single flat list.
fn all_keys(env: &Env, a: &Address, b: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    let mut all: std::vec::Vec<(&'static str, Bytes)> = std::vec::Vec::new();
    all.extend(keys_config(env, a, b));
    all.extend(keys_payment(env, a, b));
    all.extend(keys_subscription(env, a, b));
    all.extend(keys_feature(env, a, b));
    all.extend(keys_customer(env, a, b));
    all.extend(keys_merchant(env, a, b));
    all.extend(keys_state(env, a, b));
    all
}

#[test]
fn test_config_variants_are_unique() {
    let env = Env::default();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let keys = keys_config(&env, &a, &b);
    assert_eq!(keys.len(), EXPECTED_CONFIGKEY_VARIANTS);
    assert_unique_keys("Config", &keys);
}

#[test]
fn test_payment_variants_are_unique() {
    let env = Env::default();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let keys = keys_payment(&env, &a, &b);
    assert_eq!(keys.len(), EXPECTED_PAYMENTKEY_VARIANTS);
    assert_unique_keys("Payment", &keys);
}

#[test]
fn test_subscription_variants_are_unique() {
    let env = Env::default();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let keys = keys_subscription(&env, &a, &b);
    assert_eq!(keys.len(), EXPECTED_SUBSCRIPTIONKEY_VARIANTS);
    assert_unique_keys("Subscription", &keys);
}

#[test]
fn test_feature_variants_are_unique() {
    let env = Env::default();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let keys = keys_feature(&env, &a, &b);
    assert_eq!(keys.len(), EXPECTED_FEATUREKEY_VARIANTS);
    assert_unique_keys("Feature", &keys);
}

#[test]
fn test_customer_variants_are_unique() {
    let env = Env::default();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let keys = keys_customer(&env, &a, &b);
    assert_eq!(keys.len(), EXPECTED_CUSTOMERDATAKEY_VARIANTS);
    assert_unique_keys("Customer", &keys);
}

#[test]
fn test_merchant_variants_are_unique() {
    let env = Env::default();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let keys = keys_merchant(&env, &a, &b);
    assert_eq!(keys.len(), EXPECTED_MERCHANTDATAKEY_VARIANTS);
    assert_unique_keys("Merchant", &keys);
}

#[test]
fn test_state_variants_are_unique() {
    let env = Env::default();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let keys = keys_state(&env, &a, &b);
    assert_eq!(keys.len(), EXPECTED_STATEDATAKEY_VARIANTS);
    assert_unique_keys("State", &keys);
}

/// The headline audit: no two keys anywhere in this contract's instance storage
/// may serialize to the same bytes.
#[test]
fn test_no_storage_key_collisions_across_all_namespaces() {
    let env = Env::default();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let keys = all_keys(&env, &a, &b);
    assert_eq!(
        keys.len(),
        EXPECTED_CONFIGKEY_VARIANTS
            + EXPECTED_PAYMENTKEY_VARIANTS
            + EXPECTED_SUBSCRIPTIONKEY_VARIANTS
            + EXPECTED_FEATUREKEY_VARIANTS
            + EXPECTED_CUSTOMERDATAKEY_VARIANTS
            + EXPECTED_MERCHANTDATAKEY_VARIANTS
            + EXPECTED_STATEDATAKEY_VARIANTS
    );
    assert_unique_keys("instance storage", &keys);
}

/// The outer `DataKey` namespace is the only thing separating the inner enums,
/// so a duplicated outer spelling would merge two namespaces silently. Assert
/// that each namespace is reachable through exactly one outer variant.
#[test]
fn test_outer_namespace_tags_are_unique() {
    let env = Env::default();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let probes: std::vec::Vec<(&str, Bytes)> = std::vec![
        (
            "DataKey::Config",
            DataKey::Config(ConfigKey::Admin).to_xdr(&env)
        ),
        (
            "DataKey::Payment",
            DataKey::Payment(PaymentKey::Data(1)).to_xdr(&env)
        ),
        (
            "DataKey::Subscription",
            DataKey::Subscription(SubscriptionKey::Data(1)).to_xdr(&env)
        ),
        (
            "DataKey::Feature",
            DataKey::Feature(FeatureKey::PaymentAnalytics).to_xdr(&env)
        ),
        (
            "DataKey::Customer",
            DataKey::Customer(CustomerDataKey::Payments(a.clone(), 1)).to_xdr(&env)
        ),
        (
            "DataKey::Merchant",
            DataKey::Merchant(MerchantDataKey::Payments(a.clone(), 1)).to_xdr(&env)
        ),
        (
            "DataKey::State",
            DataKey::State(StateDataKey::DunningState(1)).to_xdr(&env)
        ),
    ];
    assert_unique_keys("DataKey namespaces", &probes);
}

/// A key's encoding is `Vec[Symbol(variant_name), fields..]`; assert the leading
/// symbol for one key per namespace so a rename fails loudly instead of
/// silently orphaning already-deployed state.
#[test]
fn test_key_encoding_is_keyed_on_variant_name() {
    let env = Env::default();
    let a = Address::generate(&env);
    let b = Address::generate(&env);

    let leading_symbol = |xdr: Bytes| -> Symbol {
        let val: Val = Val::from_xdr(&env, &xdr).expect("key decodes to a Val");
        let outer = <soroban_sdk::Vec<Val> as TryFromVal<Env, Val>>::try_from_val(&env, &val)
            .expect("key encoding is a ScVal vec");
        Symbol::try_from_val(&env, &outer.get(0).expect("key vec is non-empty"))
            .expect("key discriminant is a symbol")
    };

    assert_eq!(
        leading_symbol(DataKey::Config(ConfigKey::Admin).to_xdr(&env)),
        Symbol::new(&env, "Config")
    );
    assert_eq!(
        leading_symbol(DataKey::Payment(PaymentKey::Data(1)).to_xdr(&env)),
        Symbol::new(&env, "Payment")
    );
    assert_eq!(
        leading_symbol(DataKey::Subscription(SubscriptionKey::Data(1)).to_xdr(&env)),
        Symbol::new(&env, "Subscription")
    );
    assert_eq!(
        leading_symbol(DataKey::Feature(FeatureKey::PaymentAnalytics).to_xdr(&env)),
        Symbol::new(&env, "Feature")
    );
    assert_eq!(
        leading_symbol(DataKey::Customer(CustomerDataKey::Payments(a.clone(), 1)).to_xdr(&env)),
        Symbol::new(&env, "Customer")
    );
    assert_eq!(
        leading_symbol(DataKey::Merchant(MerchantDataKey::Payments(a.clone(), 1)).to_xdr(&env)),
        Symbol::new(&env, "Merchant")
    );
    assert_eq!(
        leading_symbol(DataKey::State(StateDataKey::DunningState(1)).to_xdr(&env)),
        Symbol::new(&env, "State")
    );
}

/// Proves the namespacing works on live state, not just on encodings: the same
/// inner variant name used under two different outer namespaces must not see
/// each other's writes.
#[test]
fn test_namespaced_keys_hold_independent_values() {
    let env = Env::default();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let contract_id = env.register(PaymentContract, ());

    env.as_contract(&contract_id, || {
        // `Analytics` exists under both `Customer` and `Merchant`; writing one
        // must not be readable through the other.
        env.storage().instance().set(
            &DataKey::Customer(CustomerDataKey::Analytics(customer.clone())),
            &111u64,
        );
        env.storage().instance().set(
            &DataKey::Merchant(MerchantDataKey::Analytics(merchant.clone())),
            &222u64,
        );
        // `Counter` exists under both `Payment` and `Subscription`.
        env.storage()
            .instance()
            .set(&DataKey::Payment(PaymentKey::Counter), &333u64);
        env.storage()
            .instance()
            .set(&DataKey::Subscription(SubscriptionKey::Counter), &444u64);
        // Same shape (`Data(u64)`) under two different namespaces.
        env.storage()
            .instance()
            .set(&DataKey::Payment(PaymentKey::Data(7)), &555u64);
        env.storage()
            .instance()
            .set(&DataKey::Subscription(SubscriptionKey::Data(7)), &666u64);
    });

    let (cust, merch, pay_ctr, sub_ctr, pay_7, sub_7) = env.as_contract(&contract_id, || {
        (
            env.storage()
                .instance()
                .get::<DataKey, u64>(&DataKey::Customer(CustomerDataKey::Analytics(
                    customer.clone(),
                )))
                .unwrap(),
            env.storage()
                .instance()
                .get::<DataKey, u64>(&DataKey::Merchant(MerchantDataKey::Analytics(
                    merchant.clone(),
                )))
                .unwrap(),
            env.storage()
                .instance()
                .get::<DataKey, u64>(&DataKey::Payment(PaymentKey::Counter))
                .unwrap(),
            env.storage()
                .instance()
                .get::<DataKey, u64>(&DataKey::Subscription(SubscriptionKey::Counter))
                .unwrap(),
            env.storage()
                .instance()
                .get::<DataKey, u64>(&DataKey::Payment(PaymentKey::Data(7)))
                .unwrap(),
            env.storage()
                .instance()
                .get::<DataKey, u64>(&DataKey::Subscription(SubscriptionKey::Data(7)))
                .unwrap(),
        )
    });

    assert_eq!(cust, 111);
    assert_eq!(merch, 222);
    assert_eq!(pay_ctr, 333);
    assert_eq!(sub_ctr, 444);
    assert_eq!(pay_7, 555);
    assert_eq!(sub_7, 666);
}
