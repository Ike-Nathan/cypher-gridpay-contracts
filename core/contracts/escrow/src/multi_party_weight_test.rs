#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, token, Address, Env, Vec};

#[test]
fn test_multi_party_escrow_weight_governance() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let token_client = token::StellarAssetClient::new(&env, &token_id);
    let token_user_client = token::Client::new(&env, &token_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let customer = Address::generate(&env);
    token_client.mint(&customer, &10000);

    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    let p3 = Address::generate(&env);

    let mut participants = Vec::new(&env);
    // p1 has 33% share — at or near the MAX_PARTICIPANT_WEIGHT_BPS cap (3334).
    participants.push_back(Participant {
        address: p1.clone(),
        role: ParticipantRole::Merchant,
        share_bps: 3334,
        weight_bps: 3334,
        approved: false,
        approved_at: None,
    });
    // p2 has 33% share.
    participants.push_back(Participant {
        address: p2.clone(),
        role: ParticipantRole::ServiceProvider,
        share_bps: 3333,
        weight_bps: 3333,
        approved: false,
        approved_at: None,
    });
    // p3 has 33% share.
    participants.push_back(Participant {
        address: p3.clone(),
        role: ParticipantRole::Arbitrator,
        share_bps: 3333,
        weight_bps: 3333,
        approved: false,
        approved_at: None,
    });

    let release_timestamp = 1000_u64;
    env.ledger().set_timestamp(500);

    let escrow_id = client.create_multi_party_escrow(
        &customer,
        &participants,
        &10000,
        &token_id,
        &release_timestamp,
    );

    // Verify default threshold is 10000 bps
    let (approved_wt, threshold_bps) = client.get_approval_weight(&escrow_id);
    assert_eq!(approved_wt, 0);
    assert_eq!(threshold_bps, 10000);

    // p1 approves (weight capped at 3334 bps).
    client.approve_release(&p1, &escrow_id);
    let (approved_wt, _) = client.get_approval_weight(&escrow_id);
    assert_eq!(approved_wt, 3334);

    // At 3334 bps approved (threshold 10000) release must fail.
    env.ledger().set_timestamp(1001);
    let release_res = client.try_release_multi_party_escrow(&escrow_id);
    assert_eq!(
        release_res,
        Err(Ok(Error::Action(ActionError::ApprovalsThresholdNotMet)))
    );

    // p2 also approves: total = 3334 + 3333 = 6667 bps (>= 2/3 quorum).
    client.approve_release(&p2, &escrow_id);
    let (approved_wt, _) = client.get_approval_weight(&escrow_id);
    assert_eq!(approved_wt, 6667);

    // Update threshold to 6667 bps (≈2/3 majority).
    client.update_approval_threshold_bps(&admin, &escrow_id, &6667);
    let (_, threshold_bps) = client.get_approval_weight(&escrow_id);
    assert_eq!(threshold_bps, 6667);

    // Now release must succeed (6667 >= 6667).
    let release_res2 = client.try_release_multi_party_escrow(&escrow_id);
    assert!(release_res2.is_ok());

    let escrow = client.get_multi_party_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
    assert_eq!(token_user_client.balance(&p1), 3334);
    assert_eq!(token_user_client.balance(&p2), 3333);
    assert_eq!(token_user_client.balance(&p3), 3333);
}

#[test]
fn test_multi_party_escrow_set_participant_weight() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let token_client = token::StellarAssetClient::new(&env, &token_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let customer = Address::generate(&env);
    token_client.mint(&customer, &10000);

    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);

    let mut participants = Vec::new(&env);
    participants.push_back(Participant {
        address: p1.clone(),
        role: ParticipantRole::Merchant,
        share_bps: 5000,
        weight_bps: 5000,
        approved: false,
        approved_at: None,
    });
    participants.push_back(Participant {
        address: p2.clone(),
        role: ParticipantRole::ServiceProvider,
        share_bps: 5000,
        weight_bps: 5000,
        approved: false,
        approved_at: None,
    });

    let release_timestamp = 1000_u64;
    let escrow_id = client.create_multi_party_escrow(
        &customer,
        &participants,
        &10000,
        &token_id,
        &release_timestamp,
    );

    // Both participants are capped at 3334 bps (share_bps 5000 > cap 3334).
    let (wt, _) = client.get_approval_weight(&escrow_id);
    assert_eq!(wt, 0); // nothing approved yet

    // Try to set p1's weight to the cap (3334): should succeed.
    client.set_participant_weight(&admin, &escrow_id, &p1, &3334);

    // Try to set p1's weight above the cap (3335): must fail.
    let over_cap = client.try_set_participant_weight(&admin, &escrow_id, &p1, &3335);
    assert_eq!(
        over_cap,
        Err(Ok(Error::Action(ActionError::ParticipantWeightExceedsCap)))
    );

    // Set threshold to 3334 bps and let p1 alone release.
    client.update_approval_threshold_bps(&admin, &escrow_id, &3334);

    // Approve from p1.
    client.approve_release(&p1, &escrow_id);
    let (approved_wt, threshold_bps) = client.get_approval_weight(&escrow_id);
    assert_eq!(approved_wt, 3334);
    assert_eq!(threshold_bps, 3334);

    // Advance time and release.
    env.ledger().set_timestamp(1001);
    let release_res = client.try_release_multi_party_escrow(&escrow_id);
    assert!(release_res.is_ok());
}

/// Regression test for issue #27: odd-number voter weight aggregation must not
/// lose a >50% majority to integer truncation during quorum calculation.
///
/// Three voters with equal shares (3333/3333/3334 bps) are used. Two of the
/// three approve, giving an approved weight of 6666 bps against a 6666 bps
/// threshold (2/3 majority). With naive truncation the aggregation could fall
/// short; the precision-scaled quorum calculation must round in favor of
/// quorum attainment so the release succeeds.
#[test]
fn test_multi_party_escrow_odd_voter_weight_aggregation() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let token_client = token::StellarAssetClient::new(&env, &token_id);
    let token_user_client = token::Client::new(&env, &token_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let customer = Address::generate(&env);
    token_client.mint(&customer, &10000);

    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    let p3 = Address::generate(&env);

    let mut participants = Vec::new(&env);
    // Odd-number voter set: shares do not divide evenly into 10000 bps.
    participants.push_back(Participant {
        address: p1.clone(),
        role: ParticipantRole::Merchant,
        share_bps: 3333,
        weight_bps: 0,
        approved: false,
        approved_at: None,
    });
    participants.push_back(Participant {
        address: p2.clone(),
        role: ParticipantRole::ServiceProvider,
        share_bps: 3333,
        weight_bps: 0,
        approved: false,
        approved_at: None,
    });
    participants.push_back(Participant {
        address: p3.clone(),
        role: ParticipantRole::Arbitrator,
        share_bps: 3334,
        weight_bps: 0,
        approved: false,
        approved_at: None,
    });

    let release_timestamp = 1000_u64;
    env.ledger().set_timestamp(500);

    let escrow_id = client.create_multi_party_escrow(
        &customer,
        &participants,
        &10000,
        &token_id,
        &release_timestamp,
    );

    // Two of three voters approve: 3333 + 3333 = 6666 bps.
    client.approve_release(&p1, &escrow_id);
    client.approve_release(&p2, &escrow_id);
    let (approved_wt, _) = client.get_approval_weight(&escrow_id);
    assert_eq!(approved_wt, 6666);

    // Set the quorum threshold to exactly the aggregated weight (2/3 majority).
    client.update_approval_threshold_bps(&admin, &escrow_id, &6666);
    let (_, threshold_bps) = client.get_approval_weight(&escrow_id);
    assert_eq!(threshold_bps, 6666);

    // The >50% majority must be honored: release succeeds without truncation loss.
    env.ledger().set_timestamp(1001);
    let release_res = client.try_release_multi_party_escrow(&escrow_id);
    assert!(release_res.is_ok());

    let escrow = client.get_multi_party_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
    assert_eq!(token_user_client.balance(&p1), 3333);
    assert_eq!(token_user_client.balance(&p2), 3333);
    assert_eq!(token_user_client.balance(&p3), 3334);
}

/// Whale-override prevention test.
///
/// In a consortium GridPay infrastructure escrow where 1 large utility company
/// (60% stake) and 10 community micro-grid nodes (4% each) co-fund an escrow,
/// linear capital weighting would let the whale override all 10 community
/// nodes.  The MAX_PARTICIPANT_WEIGHT_BPS cap (3334 bps) ensures the whale's
/// voting power is capped at ~33%, requiring at least one community node to
/// co-approve before any release threshold ≥ 3335 is met.
#[test]
fn test_whale_cannot_unilaterally_release_at_majority_threshold() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let token_client = token::StellarAssetClient::new(&env, &token_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let customer = Address::generate(&env);
    token_client.mint(&customer, &10000);

    let whale = Address::generate(&env);
    // Build 10 community nodes (4% each = 400 bps).
    let mut nodes: soroban_sdk::Vec<Address> = Vec::new(&env);
    for _ in 0..10u32 {
        nodes.push_back(Address::generate(&env));
    }

    let mut participants = Vec::new(&env);
    // Whale: 60% stake (6000 bps share).  Voting weight capped at 3334.
    participants.push_back(Participant {
        address: whale.clone(),
        role: ParticipantRole::Merchant,
        share_bps: 6000,
        weight_bps: 3334,
        approved: false,
        approved_at: None,
    });
    // 10 community nodes: 400 bps each (total = 4000; 6000 + 4000 = 10000 ✓).
    for i in 0..10u32 {
        participants.push_back(Participant {
            address: nodes.get(i).unwrap().clone(),
            role: ParticipantRole::Customer,
            share_bps: 400,
            weight_bps: 400,
            approved: false,
            approved_at: None,
        });
    }

    env.ledger().set_timestamp(500);
    let release_timestamp = 1000_u64;

    let escrow_id = client.create_multi_party_escrow(
        &customer,
        &participants,
        &10000,
        &token_id,
        &release_timestamp,
    );

    // Verify whale's effective weight is capped at 3334.
    let (wt_before, _) = client.get_approval_weight(&escrow_id);
    assert_eq!(wt_before, 0);

    // Whale approves alone.
    client.approve_release(&whale, &escrow_id);
    let (whale_wt, threshold) = client.get_approval_weight(&escrow_id);
    assert_eq!(whale_wt, 3334, "whale weight must be capped at MAX_PARTICIPANT_WEIGHT_BPS");

    // At a standard 51% threshold (5001 bps), the whale alone must NOT be
    // sufficient to release.
    client.update_approval_threshold_bps(&admin, &escrow_id, &5001);
    env.ledger().set_timestamp(1001);
    let release_solo = client.try_release_multi_party_escrow(&escrow_id);
    assert_eq!(
        release_solo,
        Err(Ok(Error::Action(ActionError::ApprovalsThresholdNotMet))),
        "whale alone must not meet a 51% threshold"
    );

    // One community node co-approves: 3334 + 400 = 3734 — still below 5001.
    let node0 = nodes.get(0).unwrap();
    client.approve_release(&node0, &escrow_id);
    let (combined_wt, _) = client.get_approval_weight(&escrow_id);
    assert_eq!(combined_wt, 3734);

    let still_blocked = client.try_release_multi_party_escrow(&escrow_id);
    assert_eq!(
        still_blocked,
        Err(Ok(Error::Action(ActionError::ApprovalsThresholdNotMet))),
        "whale + 1 node must still be below 51% threshold"
    );

    // Admin drops the threshold to whale + 1 node weight (3734).
    client.update_approval_threshold_bps(&admin, &escrow_id, &3734);
    let release_ok = client.try_release_multi_party_escrow(&escrow_id);
    assert!(release_ok.is_ok(), "whale + 1 community node approval must release at 3734 threshold");
}
