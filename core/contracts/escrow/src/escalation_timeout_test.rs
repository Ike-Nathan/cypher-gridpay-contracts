#![cfg(test)]

use crate::*;
use crate::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, String,
};

fn setup(env: &Env) -> (EscrowContractClient, Address, Address, Address, Address) {
    env.mock_all_auths();
    let id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(env, &id);
    let admin = Address::generate(env);
    client.initialize(&admin);

    let token_addr = env.register_stellar_asset_contract(admin.clone());
    let token_admin = token::StellarAssetClient::new(env, &token_addr);
    let customer = Address::generate(env);
    token_admin.mint(&customer, &10_000i128);
    token_admin.mint(&id, &10_000i128);

    (client, admin, customer, Address::generate(env), token_addr)
}

fn make_disputed_escrow(
    env: &Env,
    client: &EscrowContractClient,
    customer: &Address,
    merchant: &Address,
    token: &Address,
) -> u64 {
    env.ledger().set_timestamp(1000);
    let escrow_id = client.create_escrow(
        customer, merchant, &1000i128, token, &9000u64, &0u64, &0u64, &false,
    );
    client.dispute_escrow(customer, &escrow_id);
    escrow_id
}

#[test]
fn test_trigger_timeout_fails_if_not_escalated() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    env.ledger().set_timestamp(100_000);
    // Never called escalate_dispute, so escalated_at is None → InvalidStatus
    let result = client.try_trigger_timeout_resolution(&escrow_id);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::InvalidStatus))));
}

#[test]
fn test_trigger_timeout_fails_before_deadline() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    client.set_escalation_config(&admin, &300u64, &AutoResolveFavor::Customer);

    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    env.ledger().set_timestamp(1500);
    client.escalate_dispute(&customer, &escrow_id);

    // Only 100s elapsed, timeout=300
    env.ledger().set_timestamp(1600);
    let result = client.try_trigger_timeout_resolution(&escrow_id);
    assert_eq!(
        result,
        Err(Ok(Error::Escrow(EscrowError::TimeoutNotReached)))
    );
}

#[test]
fn test_trigger_timeout_resolves_in_favor_of_customer() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    client.set_escalation_config(&admin, &300u64, &AutoResolveFavor::Customer);

    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    env.ledger().set_timestamp(1500);
    client.escalate_dispute(&customer, &escrow_id);

    env.ledger().set_timestamp(1500 + 301);
    client.trigger_timeout_resolution(&escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Resolved);
}

#[test]
fn test_trigger_timeout_split_equal() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    client.set_escalation_config(&admin, &300u64, &AutoResolveFavor::SplitEqual);

    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let token_client = token::Client::new(&env, &token);
    let customer_before = token_client.balance(&customer);
    let merchant_before = token_client.balance(&merchant);

    env.ledger().set_timestamp(1500);
    client.escalate_dispute(&customer, &escrow_id);
    env.ledger().set_timestamp(1500 + 301);
    client.trigger_timeout_resolution(&escrow_id);

    let customer_after = token_client.balance(&customer);
    let merchant_after = token_client.balance(&merchant);

    // escrow amount=1000 split evenly
    assert_eq!(customer_after - customer_before, 500i128);
    assert_eq!(merchant_after - merchant_before, 500i128);
}

#[test]
fn test_check_escalation_timeout_false_before_escalation() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    env.ledger().set_timestamp(999_999);
    assert!(!client.check_escalation_timeout(&escrow_id));
}

#[test]
fn test_process_escalation_timeouts_uses_queue() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    client.set_escalation_config(&admin, &300u64, &AutoResolveFavor::Customer);

    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    env.ledger().set_timestamp(1500);
    client.escalate_dispute(&customer, &escrow_id);
    env.ledger().set_timestamp(1500 + 301);

    let processed = client.process_escalation_timeouts(&10_u32);
    assert_eq!(processed, 1);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Resolved);
}

#[test]
fn test_process_escalation_timeouts_skips_future_deadlines() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    client.set_escalation_config(&admin, &300u64, &AutoResolveFavor::Customer);

    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    env.ledger().set_timestamp(2000);
    client.escalate_dispute(&customer, &escrow_id);
    env.ledger().set_timestamp(2200);

    let processed = client.process_escalation_timeouts(&10_u32);
    assert_eq!(processed, 0);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Disputed);
}

#[test]
fn test_trigger_timeout_prefers_party_with_evidence() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    // Configured default favors the customer; evidence should override it.
    client.set_escalation_config(&admin, &300u64, &AutoResolveFavor::Customer);

    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    env.ledger().set_timestamp(1500);
    client.submit_evidence(
        &merchant,
        &escrow_id,
        &String::from_str(&env, "ipfs://merchant-evidence"),
    );
    client.escalate_dispute(&customer, &escrow_id);

    let token_client = token::Client::new(&env, &token);
    let merchant_before = token_client.balance(&merchant);

    env.ledger().set_timestamp(1500 + 301);
    client.trigger_timeout_resolution(&escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
    assert_eq!(
        token_client.balance(&merchant) - merchant_before,
        1000i128,
        "the only party that filed evidence should win the timeout resolution"
    );
}

#[test]
fn test_trigger_timeout_requires_arbitration_when_both_submit_evidence() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    client.set_escalation_config(&admin, &300u64, &AutoResolveFavor::Customer);

    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    env.ledger().set_timestamp(1500);
    client.submit_evidence(
        &customer,
        &escrow_id,
        &String::from_str(&env, "ipfs://customer-evidence"),
    );
    client.submit_evidence(
        &merchant,
        &escrow_id,
        &String::from_str(&env, "ipfs://merchant-evidence"),
    );
    client.escalate_dispute(&customer, &escrow_id);

    env.ledger().set_timestamp(1500 + 301);
    let result = client.try_trigger_timeout_resolution(&escrow_id);
    assert_eq!(
        result,
        Err(Ok(Error::Action(ActionError::ArbitrationRequired))),
        "contested evidence must not be auto-resolved without an arbitrator"
    );

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Disputed);
}

// ── Issue #218: DisputeRubricEvaluation tests ────────────────────────────────

/// Helper: sets up the dispute oracle and returns the oracle address.
fn setup_oracle(
    env: &Env,
    client: &EscrowContractClient,
    admin: &Address,
) -> Address {
    let oracle = Address::generate(env);
    client.set_dispute_oracle(admin, &oracle, &9_000u32);
    oracle
}

/// Helper: creates a rubric hash (32-byte zeroes for test purposes).
fn dummy_rubric(env: &Env) -> soroban_sdk::BytesN<32> {
    soroban_sdk::BytesN::from_array(env, &[0u8; 32])
}

#[test]
fn test_submit_dispute_recommendation_success() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    let oracle = setup_oracle(&env, &client, &admin);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    env.ledger().set_timestamp(2000);
    client.submit_dispute_recommendation(
        &oracle,
        &escrow_id,
        &7_000u32, // buyer gets 70 %
        &3_000u32, // seller gets 30 %
        &9_500u32, // high confidence
        &dummy_rubric(&env),
        &3_600u64, // 1-hour challenge window
    );

    // Escrow still Disputed (not yet finalized).
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Disputed);
}

#[test]
fn test_submit_dispute_recommendation_wrong_oracle_rejected() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    setup_oracle(&env, &client, &admin);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    // A random address — not the registered oracle — should be rejected.
    let impostor = Address::generate(&env);
    let result = client.try_submit_dispute_recommendation(
        &impostor,
        &escrow_id,
        &5_000u32,
        &5_000u32,
        &9_000u32,
        &dummy_rubric(&env),
        &3_600u64,
    );
    assert_eq!(
        result,
        Err(Ok(Error::Action(ActionError::NotDisputeOracle)))
    );
}

#[test]
fn test_submit_dispute_recommendation_invalid_split_rejected() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    let oracle = setup_oracle(&env, &client, &admin);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    // 6_000 + 3_000 = 9_000 ≠ 10_000 → invalid.
    let result = client.try_submit_dispute_recommendation(
        &oracle,
        &escrow_id,
        &6_000u32,
        &3_000u32,
        &9_000u32,
        &dummy_rubric(&env),
        &3_600u64,
    );
    assert_eq!(
        result,
        Err(Ok(Error::Action(ActionError::InvalidRubricSplit)))
    );
}

#[test]
fn test_finalize_recommended_resolution_distributes_funds() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    let oracle = setup_oracle(&env, &client, &admin);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let token_client = token::Client::new(&env, &token);
    let customer_before = token_client.balance(&customer);
    let merchant_before = token_client.balance(&merchant);

    env.ledger().set_timestamp(2000);
    // Submit: 70 % to buyer, 30 % to seller, high confidence, 1-hour window.
    client.submit_dispute_recommendation(
        &oracle,
        &escrow_id,
        &7_000u32,
        &3_000u32,
        &9_500u32,
        &dummy_rubric(&env),
        &3_600u64,
    );

    // Advance past the challenge window.
    env.ledger().set_timestamp(2000 + 3_601);
    client.finalize_recommended_resolution(&escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Resolved);

    // escrow amount = 1 000; buyer 70 % = 700, seller 30 % = 300.
    assert_eq!(token_client.balance(&customer) - customer_before, 700i128);
    assert_eq!(token_client.balance(&merchant) - merchant_before, 300i128);
}

#[test]
fn test_finalize_fails_before_challenge_window_closes() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    let oracle = setup_oracle(&env, &client, &admin);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    env.ledger().set_timestamp(2000);
    client.submit_dispute_recommendation(
        &oracle,
        &escrow_id,
        &5_000u32,
        &5_000u32,
        &9_500u32,
        &dummy_rubric(&env),
        &3_600u64,
    );

    // Still within the 1-hour window.
    env.ledger().set_timestamp(2000 + 1_000);
    let result = client.try_finalize_recommended_resolution(&escrow_id);
    assert_eq!(
        result,
        Err(Ok(Error::Action(ActionError::ChallengeWindowOpen)))
    );
}

#[test]
fn test_finalize_fails_when_confidence_below_threshold() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    let oracle = setup_oracle(&env, &client, &admin);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    env.ledger().set_timestamp(2000);
    // Low confidence (8 000 bps < 9 000 threshold).
    client.submit_dispute_recommendation(
        &oracle,
        &escrow_id,
        &5_000u32,
        &5_000u32,
        &8_000u32,
        &dummy_rubric(&env),
        &3_600u64,
    );

    env.ledger().set_timestamp(2000 + 3_601);
    let result = client.try_finalize_recommended_resolution(&escrow_id);
    assert_eq!(
        result,
        Err(Ok(Error::Action(ActionError::ConfidenceBelowFastTrack)))
    );
}

#[test]
fn test_buyer_veto_escalates_to_arbitration() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    let oracle = setup_oracle(&env, &client, &admin);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    env.ledger().set_timestamp(2000);
    client.submit_dispute_recommendation(
        &oracle,
        &escrow_id,
        &3_000u32,  // buyer only gets 30 % — buyer will veto
        &7_000u32,
        &9_500u32,
        &dummy_rubric(&env),
        &3_600u64,
    );

    // Buyer vetoes within the window.
    env.ledger().set_timestamp(2000 + 100);
    client.reject_dispute_recommendation(&customer, &escrow_id);

    // After window closes, finalize must fail because the eval was vetoed.
    env.ledger().set_timestamp(2000 + 3_601);
    let result = client.try_finalize_recommended_resolution(&escrow_id);
    assert_eq!(
        result,
        Err(Ok(Error::Action(ActionError::RecommendationVetoed)))
    );

    // Escrow must still be Disputed (standard arbitration path remains open).
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Disputed);
}

#[test]
fn test_seller_veto_escalates_to_arbitration() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    let oracle = setup_oracle(&env, &client, &admin);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    env.ledger().set_timestamp(2000);
    client.submit_dispute_recommendation(
        &oracle,
        &escrow_id,
        &9_000u32,
        &1_000u32,  // seller only gets 10 % — seller will veto
        &9_500u32,
        &dummy_rubric(&env),
        &3_600u64,
    );

    // Seller vetoes within the window.
    env.ledger().set_timestamp(2000 + 200);
    client.reject_dispute_recommendation(&merchant, &escrow_id);

    env.ledger().set_timestamp(2000 + 3_601);
    let result = client.try_finalize_recommended_resolution(&escrow_id);
    assert_eq!(
        result,
        Err(Ok(Error::Action(ActionError::RecommendationVetoed)))
    );

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Disputed);
}

#[test]
fn test_veto_rejected_after_challenge_window_closes() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    let oracle = setup_oracle(&env, &client, &admin);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    env.ledger().set_timestamp(2000);
    client.submit_dispute_recommendation(
        &oracle,
        &escrow_id,
        &5_000u32,
        &5_000u32,
        &9_500u32,
        &dummy_rubric(&env),
        &3_600u64,
    );

    // Attempt to veto after the challenge window has already closed.
    env.ledger().set_timestamp(2000 + 3_601);
    let result = client.try_reject_dispute_recommendation(&customer, &escrow_id);
    assert_eq!(
        result,
        Err(Ok(Error::Action(ActionError::RecommendationVetoed)))
    );
}

#[test]
fn test_duplicate_recommendation_rejected() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    let oracle = setup_oracle(&env, &client, &admin);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    env.ledger().set_timestamp(2000);
    // First submission succeeds.
    client.submit_dispute_recommendation(
        &oracle,
        &escrow_id,
        &5_000u32,
        &5_000u32,
        &9_500u32,
        &dummy_rubric(&env),
        &3_600u64,
    );

    // Second submission while first is still pending must fail.
    let result = client.try_submit_dispute_recommendation(
        &oracle,
        &escrow_id,
        &6_000u32,
        &4_000u32,
        &9_500u32,
        &dummy_rubric(&env),
        &3_600u64,
    );
    assert_eq!(
        result,
        Err(Ok(Error::Action(ActionError::RecommendationNotPending)))
    );
}

#[test]
fn test_full_seller_payout_sets_released_status() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    let oracle = setup_oracle(&env, &client, &admin);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let token_client = token::Client::new(&env, &token);
    let merchant_before = token_client.balance(&merchant);

    env.ledger().set_timestamp(2000);
    // Oracle recommends the entire amount goes to the seller (buyer_bps = 0).
    client.submit_dispute_recommendation(
        &oracle,
        &escrow_id,
        &0u32,
        &10_000u32,
        &9_999u32,
        &dummy_rubric(&env),
        &3_600u64,
    );

    env.ledger().set_timestamp(2000 + 3_601);
    client.finalize_recommended_resolution(&escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    // buyer_amount == 0 → status should be Released (merchant wins).
    assert_eq!(escrow.status, EscrowStatus::Released);
    assert_eq!(
        token_client.balance(&merchant) - merchant_before,
        1000i128,
        "full escrow amount must go to merchant"
    );
}
