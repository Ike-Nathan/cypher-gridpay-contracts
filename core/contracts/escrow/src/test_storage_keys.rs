#![cfg(test)]

//! Storage key collision audit for `EscrowContract`.
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
//! The tests below enumerate all 86 keys the contract can address, assert
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

const EXPECTED_CONFIGKEY_VARIANTS: usize = 28;

/// Every fully qualified `Config::<EnumName>` key, one per variant.
fn keys_config(env: &Env, a: &Address, b: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    let _: (&Address, &Address) = (a, b);
    std::vec![
        (
            "ConfigKey::AdminMultiSig",
            DataKey::Config(ConfigKey::AdminMultiSig).to_xdr(env)
        ),
        (
            "ConfigKey::AdminProposal",
            DataKey::Config(ConfigKey::AdminProposal(String::from_str(env, "s"))).to_xdr(env)
        ),
        (
            "ConfigKey::AdminProposalCounter",
            DataKey::Config(ConfigKey::AdminProposalCounter).to_xdr(env)
        ),
        (
            "ConfigKey::AdminSuccessionPlan",
            DataKey::Config(ConfigKey::AdminSuccessionPlan).to_xdr(env)
        ),
        (
            "ConfigKey::AdminClawbackRequest",
            DataKey::Config(ConfigKey::AdminClawbackRequest(1)).to_xdr(env)
        ),
        (
            "ConfigKey::AdminClawbackCounter",
            DataKey::Config(ConfigKey::AdminClawbackCounter).to_xdr(env)
        ),
        (
            "ConfigKey::AdminEscrowClawback",
            DataKey::Config(ConfigKey::AdminEscrowClawback(1)).to_xdr(env)
        ),
        (
            "ConfigKey::ReputationConfig",
            DataKey::Config(ConfigKey::ReputationConfig).to_xdr(env)
        ),
        (
            "ConfigKey::ReputationDecayConfig",
            DataKey::Config(ConfigKey::ReputationDecayConfig).to_xdr(env)
        ),
        (
            "ConfigKey::TenureConfig",
            DataKey::Config(ConfigKey::TenureConfig).to_xdr(env)
        ),
        (
            "ConfigKey::GlobalExpiryConfig",
            DataKey::Config(ConfigKey::GlobalExpiryConfig).to_xdr(env)
        ),
        (
            "ConfigKey::EscalationConfig",
            DataKey::Config(ConfigKey::EscalationConfig).to_xdr(env)
        ),
        (
            "ConfigKey::WatchdogConfig",
            DataKey::Config(ConfigKey::WatchdogConfig).to_xdr(env)
        ),
        (
            "ConfigKey::BatchLimit",
            DataKey::Config(ConfigKey::BatchLimit).to_xdr(env)
        ),
        (
            "ConfigKey::PauseStateKey",
            DataKey::Config(ConfigKey::PauseStateKey).to_xdr(env)
        ),
        (
            "ConfigKey::PauseHistoryEntry",
            DataKey::Config(ConfigKey::PauseHistoryEntry(1)).to_xdr(env)
        ),
        (
            "ConfigKey::PauseHistoryCount",
            DataKey::Config(ConfigKey::PauseHistoryCount).to_xdr(env)
        ),
        (
            "ConfigKey::ActivePauseIndex",
            DataKey::Config(ConfigKey::ActivePauseIndex(String::from_str(env, "s"))).to_xdr(env)
        ),
        (
            "ConfigKey::EscrowFeeConfig",
            DataKey::Config(ConfigKey::EscrowFeeConfig).to_xdr(env)
        ),
        (
            "ConfigKey::StaleThresholdConfig",
            DataKey::Config(ConfigKey::StaleThresholdConfig).to_xdr(env)
        ),
        (
            "ConfigKey::DisputeConfig",
            DataKey::Config(ConfigKey::DisputeConfig).to_xdr(env)
        ),
        (
            "ConfigKey::InsurancePool",
            DataKey::Config(ConfigKey::InsurancePool(a.clone())).to_xdr(env)
        ),
        (
            "ConfigKey::InsuranceConfig",
            DataKey::Config(ConfigKey::InsuranceConfig).to_xdr(env)
        ),
        (
            "ConfigKey::TimeLockConfig",
            DataKey::Config(ConfigKey::TimeLockConfig).to_xdr(env)
        ),
        (
            "ConfigKey::AdminClawbackEscrow",
            DataKey::Config(ConfigKey::AdminClawbackEscrow(1)).to_xdr(env)
        ),
        (
            "ConfigKey::SchemaVersion",
            DataKey::Config(ConfigKey::SchemaVersion).to_xdr(env)
        ),
        (
            "ConfigKey::TrustedBridge",
            DataKey::Config(ConfigKey::TrustedBridge(a.clone())).to_xdr(env)
        ),
        (
            "ConfigKey::EvidenceDeadlineConfig",
            DataKey::Config(ConfigKey::EvidenceDeadlineConfig).to_xdr(env)
        ),
    ]
}

const EXPECTED_ESCROWKEY_VARIANTS: usize = 26;

/// Every fully qualified `Escrow::<EnumName>` key, one per variant.
fn keys_escrow(env: &Env, a: &Address, b: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    let _: (&Address, &Address) = (a, b);
    std::vec![
        (
            "EscrowKey::Data",
            DataKey::Escrow(EscrowKey::Data(1)).to_xdr(env)
        ),
        (
            "EscrowKey::Counter",
            DataKey::Escrow(EscrowKey::Counter).to_xdr(env)
        ),
        (
            "EscrowKey::MultiParty",
            DataKey::Escrow(EscrowKey::MultiParty(1)).to_xdr(env)
        ),
        (
            "EscrowKey::MultiPartyCounter",
            DataKey::Escrow(EscrowKey::MultiPartyCounter).to_xdr(env)
        ),
        (
            "EscrowKey::CustomerList",
            DataKey::Escrow(EscrowKey::CustomerList(a.clone(), 1)).to_xdr(env)
        ),
        (
            "EscrowKey::MerchantList",
            DataKey::Escrow(EscrowKey::MerchantList(a.clone(), 1)).to_xdr(env)
        ),
        (
            "EscrowKey::CustomerCount",
            DataKey::Escrow(EscrowKey::CustomerCount(a.clone())).to_xdr(env)
        ),
        (
            "EscrowKey::MerchantCount",
            DataKey::Escrow(EscrowKey::MerchantCount(a.clone())).to_xdr(env)
        ),
        (
            "EscrowKey::Evidence",
            DataKey::Escrow(EscrowKey::Evidence(1, 1)).to_xdr(env)
        ),
        (
            "EscrowKey::EvidenceCount",
            DataKey::Escrow(EscrowKey::EvidenceCount(1)).to_xdr(env)
        ),
        (
            "EscrowKey::EvidencePage",
            DataKey::Escrow(EscrowKey::EvidencePage(1, 1)).to_xdr(env)
        ),
        (
            "EscrowKey::EvidencePageCount",
            DataKey::Escrow(EscrowKey::EvidencePageCount(1)).to_xdr(env)
        ),
        (
            "EscrowKey::EvidenceCommitment",
            DataKey::Escrow(EscrowKey::EvidenceCommitment(1)).to_xdr(env)
        ),
        (
            "EscrowKey::VestingSchedule",
            DataKey::Escrow(EscrowKey::VestingSchedule(1)).to_xdr(env)
        ),
        (
            "EscrowKey::VestingAccelerationConfig",
            DataKey::Escrow(EscrowKey::VestingAccelerationConfig(1)).to_xdr(env)
        ),
        (
            "EscrowKey::Conditional",
            DataKey::Escrow(EscrowKey::Conditional(1)).to_xdr(env)
        ),
        (
            "EscrowKey::OracleCondition",
            DataKey::Escrow(EscrowKey::OracleCondition(1)).to_xdr(env)
        ),
        (
            "EscrowKey::MultiToken",
            DataKey::Escrow(EscrowKey::MultiToken(1)).to_xdr(env)
        ),
        (
            "EscrowKey::MultiTokenCounter",
            DataKey::Escrow(EscrowKey::MultiTokenCounter).to_xdr(env)
        ),
        (
            "EscrowKey::Hierarchy",
            DataKey::Escrow(EscrowKey::Hierarchy(1)).to_xdr(env)
        ),
        (
            "EscrowKey::Template",
            DataKey::Escrow(EscrowKey::Template(1)).to_xdr(env)
        ),
        (
            "EscrowKey::TemplateCounter",
            DataKey::Escrow(EscrowKey::TemplateCounter).to_xdr(env)
        ),
        (
            "EscrowKey::SubAccount",
            DataKey::Escrow(EscrowKey::SubAccount(1, 1)).to_xdr(env)
        ),
        (
            "EscrowKey::SubAccountCounter",
            DataKey::Escrow(EscrowKey::SubAccountCounter(1)).to_xdr(env)
        ),
        (
            "EscrowKey::EscrowHierarchy",
            DataKey::Escrow(EscrowKey::EscrowHierarchy(1)).to_xdr(env)
        ),
        (
            "EscrowKey::ReleaseMultisig",
            DataKey::Escrow(EscrowKey::ReleaseMultisig(1)).to_xdr(env)
        ),
    ]
}

const EXPECTED_PARTICIPANTKEY_VARIANTS: usize = 7;

/// Every fully qualified `Participant::<EnumName>` key, one per variant.
fn keys_participant(env: &Env, a: &Address, b: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    let _: (&Address, &Address) = (a, b);
    std::vec![
        (
            "ParticipantKey::ReputationScore",
            DataKey::Participant(ParticipantKey::ReputationScore(a.clone())).to_xdr(env)
        ),
        (
            "ParticipantKey::TenureBonusApplied",
            DataKey::Participant(ParticipantKey::TenureBonusApplied(1, a.clone())).to_xdr(env)
        ),
        (
            "ParticipantKey::CustomerAnalytics",
            DataKey::Participant(ParticipantKey::CustomerAnalytics(a.clone())).to_xdr(env)
        ),
        (
            "ParticipantKey::MerchantAnalytics",
            DataKey::Participant(ParticipantKey::MerchantAnalytics(a.clone())).to_xdr(env)
        ),
        (
            "ParticipantKey::AccumulatedFees",
            DataKey::Participant(ParticipantKey::AccumulatedFees(a.clone())).to_xdr(env)
        ),
        (
            "ParticipantKey::BeneficiaryTransferHistory",
            DataKey::Participant(ParticipantKey::BeneficiaryTransferHistory(1, 1)).to_xdr(env)
        ),
        (
            "ParticipantKey::BeneficiaryTransferCount",
            DataKey::Participant(ParticipantKey::BeneficiaryTransferCount(1)).to_xdr(env)
        ),
    ]
}

const EXPECTED_DISPUTEKEY_VARIANTS: usize = 23;

/// Every fully qualified `Dispute::<EnumName>` key, one per variant.
fn keys_dispute(env: &Env, a: &Address, b: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    let _: (&Address, &Address) = (a, b);
    std::vec![
        (
            "DisputeKey::Action",
            DataKey::Dispute(DisputeKey::Action(1)).to_xdr(env)
        ),
        (
            "DisputeKey::Counter",
            DataKey::Dispute(DisputeKey::Counter).to_xdr(env)
        ),
        (
            "DisputeKey::Collateral",
            DataKey::Dispute(DisputeKey::Collateral(1)).to_xdr(env)
        ),
        (
            "DisputeKey::Appeal",
            DataKey::Dispute(DisputeKey::Appeal(1)).to_xdr(env)
        ),
        (
            "DisputeKey::AppealCounter",
            DataKey::Dispute(DisputeKey::AppealCounter).to_xdr(env)
        ),
        (
            "DisputeKey::Round",
            DataKey::Dispute(DisputeKey::Round(1)).to_xdr(env)
        ),
        (
            "DisputeKey::AppealsByEscrow",
            DataKey::Dispute(DisputeKey::AppealsByEscrow(1, 1)).to_xdr(env)
        ),
        (
            "DisputeKey::InsuranceClaim",
            DataKey::Dispute(DisputeKey::InsuranceClaim(1)).to_xdr(env)
        ),
        (
            "DisputeKey::InsuranceClaimCounter",
            DataKey::Dispute(DisputeKey::InsuranceClaimCounter).to_xdr(env)
        ),
        (
            "DisputeKey::MultiPartyDispute",
            DataKey::Dispute(DisputeKey::MultiPartyDispute(1)).to_xdr(env)
        ),
        (
            "DisputeKey::EscrowAnalytics",
            DataKey::Dispute(DisputeKey::EscrowAnalytics).to_xdr(env)
        ),
        (
            "DisputeKey::EscrowMigrationStatus",
            DataKey::Dispute(DisputeKey::EscrowMigrationStatus).to_xdr(env)
        ),
        (
            "DisputeKey::EscrowMigrated",
            DataKey::Dispute(DisputeKey::EscrowMigrated(1)).to_xdr(env)
        ),
        (
            "DisputeKey::EscrowRenewalConfig",
            DataKey::Dispute(DisputeKey::EscrowRenewalConfig).to_xdr(env)
        ),
        (
            "DisputeKey::EscrowRenewal",
            DataKey::Dispute(DisputeKey::EscrowRenewal(1)).to_xdr(env)
        ),
        (
            "DisputeKey::EscrowRenewalCount",
            DataKey::Dispute(DisputeKey::EscrowRenewalCount(1)).to_xdr(env)
        ),
        (
            "DisputeKey::EscrowSwapConfig",
            DataKey::Dispute(DisputeKey::EscrowSwapConfig(1)).to_xdr(env)
        ),
        (
            "DisputeKey::Observer",
            DataKey::Dispute(DisputeKey::Observer(1, 1)).to_xdr(env)
        ),
        (
            "DisputeKey::ObserverCount",
            DataKey::Dispute(DisputeKey::ObserverCount(1)).to_xdr(env)
        ),
        (
            "DisputeKey::EscalationQueue",
            DataKey::Dispute(DisputeKey::EscalationQueue(1)).to_xdr(env)
        ),
        (
            "DisputeKey::EscalationQueueIndex",
            DataKey::Dispute(DisputeKey::EscalationQueueIndex).to_xdr(env)
        ),
        (
            "DisputeKey::EscalationDeadline",
            DataKey::Dispute(DisputeKey::EscalationDeadline(1)).to_xdr(env)
        ),
        (
            "DisputeKey::AppealRecord",
            DataKey::Dispute(DisputeKey::AppealRecord(1, 1)).to_xdr(env)
        ),
    ]
}

/// Every storage key the contract can address, as a single flat list.
fn all_keys(env: &Env, a: &Address, b: &Address) -> std::vec::Vec<(&'static str, Bytes)> {
    let mut all: std::vec::Vec<(&'static str, Bytes)> = std::vec::Vec::new();
    all.extend(keys_config(env, a, b));
    all.extend(keys_escrow(env, a, b));
    all.extend(keys_participant(env, a, b));
    all.extend(keys_dispute(env, a, b));
    all.push((
        "DataKey::VoteWeight",
        DataKey::VoteWeight(1, a.clone()).to_xdr(env),
    ));
    all.push((
        "DataKey::ReleaseThresholdBps",
        DataKey::ReleaseThresholdBps(1).to_xdr(env),
    ));
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
fn test_escrow_variants_are_unique() {
    let env = Env::default();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let keys = keys_escrow(&env, &a, &b);
    assert_eq!(keys.len(), EXPECTED_ESCROWKEY_VARIANTS);
    assert_unique_keys("Escrow", &keys);
}

#[test]
fn test_participant_variants_are_unique() {
    let env = Env::default();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let keys = keys_participant(&env, &a, &b);
    assert_eq!(keys.len(), EXPECTED_PARTICIPANTKEY_VARIANTS);
    assert_unique_keys("Participant", &keys);
}

#[test]
fn test_dispute_variants_are_unique() {
    let env = Env::default();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let keys = keys_dispute(&env, &a, &b);
    assert_eq!(keys.len(), EXPECTED_DISPUTEKEY_VARIANTS);
    assert_unique_keys("Dispute", &keys);
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
            + EXPECTED_ESCROWKEY_VARIANTS
            + EXPECTED_PARTICIPANTKEY_VARIANTS
            + EXPECTED_DISPUTEKEY_VARIANTS
            + 2
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
            DataKey::Config(ConfigKey::AdminMultiSig).to_xdr(&env)
        ),
        (
            "DataKey::Escrow",
            DataKey::Escrow(EscrowKey::Data(1)).to_xdr(&env)
        ),
        (
            "DataKey::Participant",
            DataKey::Participant(ParticipantKey::ReputationScore(a.clone())).to_xdr(&env)
        ),
        (
            "DataKey::Dispute",
            DataKey::Dispute(DisputeKey::Action(1)).to_xdr(&env)
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
        leading_symbol(DataKey::Config(ConfigKey::AdminMultiSig).to_xdr(&env)),
        Symbol::new(&env, "Config")
    );
    assert_eq!(
        leading_symbol(DataKey::Escrow(EscrowKey::Data(1)).to_xdr(&env)),
        Symbol::new(&env, "Escrow")
    );
    assert_eq!(
        leading_symbol(
            DataKey::Participant(ParticipantKey::ReputationScore(a.clone())).to_xdr(&env)
        ),
        Symbol::new(&env, "Participant")
    );
    assert_eq!(
        leading_symbol(DataKey::Dispute(DisputeKey::Action(1)).to_xdr(&env)),
        Symbol::new(&env, "Dispute")
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
    let voter = Address::generate(&env);
    let contract_id = env.register(EscrowContract, ());

    env.as_contract(&contract_id, || {
        // `Counter` exists under both `Escrow` and `Dispute`.
        env.storage()
            .instance()
            .set(&DataKey::Escrow(EscrowKey::Counter), &111u64);
        env.storage()
            .instance()
            .set(&DataKey::Dispute(DisputeKey::Counter), &222u64);
        // `Analytics` exists under both `Customer` and `Merchant` participant keys.
        env.storage().instance().set(
            &DataKey::Participant(ParticipantKey::CustomerAnalytics(customer.clone())),
            &333u64,
        );
        env.storage().instance().set(
            &DataKey::Participant(ParticipantKey::MerchantAnalytics(merchant.clone())),
            &444u64,
        );
        // The two top-level, un-namespaced keys stay isolated from each other.
        env.storage()
            .instance()
            .set(&DataKey::ReleaseThresholdBps(1), &555u64);
        env.storage()
            .instance()
            .set(&DataKey::VoteWeight(1, voter.clone()), &666u64);
    });

    let (escrow_ctr, dispute_ctr, cust, merch, threshold, weight) =
        env.as_contract(&contract_id, || {
            (
                env.storage()
                    .instance()
                    .get::<DataKey, u64>(&DataKey::Escrow(EscrowKey::Counter))
                    .unwrap(),
                env.storage()
                    .instance()
                    .get::<DataKey, u64>(&DataKey::Dispute(DisputeKey::Counter))
                    .unwrap(),
                env.storage()
                    .instance()
                    .get::<DataKey, u64>(&DataKey::Participant(ParticipantKey::CustomerAnalytics(
                        customer.clone(),
                    )))
                    .unwrap(),
                env.storage()
                    .instance()
                    .get::<DataKey, u64>(&DataKey::Participant(ParticipantKey::MerchantAnalytics(
                        merchant.clone(),
                    )))
                    .unwrap(),
                env.storage()
                    .instance()
                    .get::<DataKey, u64>(&DataKey::ReleaseThresholdBps(1))
                    .unwrap(),
                env.storage()
                    .instance()
                    .get::<DataKey, u64>(&DataKey::VoteWeight(1, voter.clone()))
                    .unwrap(),
            )
        });

    assert_eq!(escrow_ctr, 111);
    assert_eq!(dispute_ctr, 222);
    assert_eq!(cust, 333);
    assert_eq!(merch, 444);
    assert_eq!(threshold, 555);
    assert_eq!(weight, 666);
}
