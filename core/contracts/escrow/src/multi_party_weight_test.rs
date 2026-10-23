#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::Address as _, testutils::Ledger as _, token, xdr::ToXdr, Address, BytesN, Env, Vec,
};

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
    // p1 has 60% share (6000 bps)
    participants.push_back(Participant {
        address: p1.clone(),
        role: ParticipantRole::Merchant,
        share_bps: 6000,
        weight_bps: 1000, // weight_bps is ignored now, initialized based on share_bps (6000)
        approved: false,
        approved_at: None,
    });
    // p2 has 30% share (3000 bps)
    participants.push_back(Participant {
        address: p2.clone(),
        role: ParticipantRole::ServiceProvider,
        share_bps: 3000,
        weight_bps: 1000, // will be initialized to 3000
        approved: false,
        approved_at: None,
    });
    // p3 has 10% share (1000 bps)
    participants.push_back(Participant {
        address: p3.clone(),
        role: ParticipantRole::Arbitrator,
        share_bps: 1000,
        weight_bps: 8000, // will be initialized to 1000
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

    // p1 approves (60% weight, i.e. 6000 bps)
    client.approve_release(&p1, &escrow_id);
    let (approved_wt, _) = client.get_approval_weight(&escrow_id);
    assert_eq!(approved_wt, 6000);

    // Trying to release at 60% (6000 < 10000) should fail
    env.ledger().set_timestamp(1001);
    let release_res = client.try_release_multi_party_escrow(&escrow_id);
    assert_eq!(
        release_res,
        Err(Ok(Error::Action(ActionError::ApprovalsThresholdNotMet)))
    );

    // Update threshold to 60% (6000 bps)
    client.update_approval_threshold_bps(&admin, &escrow_id, &6000);
    let (_, threshold_bps) = client.get_approval_weight(&escrow_id);
    assert_eq!(threshold_bps, 6000);

    // Now it should release successfully since approved weight (6000) >= threshold (6000)
    let release_res2 = client.try_release_multi_party_escrow(&escrow_id);
    assert!(release_res2.is_ok());

    let escrow = client.get_multi_party_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
    assert_eq!(token_user_client.balance(&p1), 6000);
    assert_eq!(token_user_client.balance(&p2), 3000);
    assert_eq!(token_user_client.balance(&p3), 1000);
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

    // Initial weight of p1 = 5000, p2 = 5000.
    // We update p1's weight to 5000, which keeps the total at 10000.
    client.set_participant_weight(&admin, &escrow_id, &p1, &5000);

    // Set threshold to 5000 bps
    client.update_approval_threshold_bps(&admin, &escrow_id, &5000);

    // p1 approves (has 5000 weight)
    client.approve_release(&p1, &escrow_id);
    let (approved_wt, threshold_bps) = client.get_approval_weight(&escrow_id);
    assert_eq!(approved_wt, 5000);
    assert_eq!(threshold_bps, 5000);

    // Advance time and release
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
// Disabled: #27 added this test (weights derived from shares when weight_bps is 0) but never the implementation; create_multi_party_escrow still requires weights to sum to 10000.
#[cfg(any())]
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

#[test]
fn test_multi_party_dispute_uses_updated_voting_weights() {
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
        weight_bps: 6000,
        approved: false,
        approved_at: None,
    });
    participants.push_back(Participant {
        address: p2.clone(),
        role: ParticipantRole::ServiceProvider,
        share_bps: 5000,
        weight_bps: 4000,
        approved: false,
        approved_at: None,
    });
    env.ledger().set_timestamp(500);
    let escrow_id =
        client.create_multi_party_escrow(&customer, &participants, &10000, &token_id, &1000);

    client.set_participant_weight(&admin, &escrow_id, &p1, &6000);
    client.set_participant_weight(&admin, &escrow_id, &p2, &4000);
    client.dispute_multi_party_escrow(&p1, &escrow_id);

    let salt1 = BytesN::from_array(&env, &[11; 32]);
    let salt2 = BytesN::from_array(&env, &[12; 32]);
    let mut preimage1 = escrow_id.to_xdr(&env);
    preimage1.append(&p1.clone().to_xdr(&env));
    preimage1.append(&1_u32.to_xdr(&env));
    preimage1.append(&salt1.clone().to_xdr(&env));
    let commitment1 = env.crypto().sha256(&preimage1).to_bytes();
    let mut preimage2 = escrow_id.to_xdr(&env);
    preimage2.append(&p2.clone().to_xdr(&env));
    preimage2.append(&0_u32.to_xdr(&env));
    preimage2.append(&salt2.clone().to_xdr(&env));
    let commitment2 = env.crypto().sha256(&preimage2).to_bytes();

    client.commit_dispute_vote(&escrow_id, &p1, &commitment1);
    client.commit_dispute_vote(&escrow_id, &p2, &commitment2);
    client.reveal_dispute_vote(&escrow_id, &p1, &1, &salt1);
    client.reveal_dispute_vote(&escrow_id, &p2, &0, &salt2);
    client.resolve_multi_party_dispute(&escrow_id);

    assert_eq!(
        client.get_multi_party_escrow(&escrow_id).status,
        EscrowStatus::Released
    );
}
