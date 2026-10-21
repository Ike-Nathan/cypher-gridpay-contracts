#![cfg(test)]

use super::*;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{testutils::Address as _, token, xdr::ToXdr, Address, BytesN, Env};

const COMMIT_PERIOD_SECONDS: u64 = 3 * 24 * 3600 + 12 * 3600;
const REVEAL_PERIOD_SECONDS: u64 = 7 * 24 * 3600;

fn make_3_party_escrow(
    env: &Env,
    client: &EscrowContractClient,
    token: &Address,
    p1: &Address,
    p2: &Address,
    p3: &Address,
) -> u64 {
    token::StellarAssetClient::new(env, token).mint(p1, &1000);
    let mut participants = soroban_sdk::Vec::new(env);
    participants.push_back(Participant {
        address: p1.clone(),
        role: ParticipantRole::Customer,
        share_bps: 4000,
        weight_bps: 4000,
        approved: false,
        approved_at: None,
    });
    participants.push_back(Participant {
        address: p2.clone(),
        role: ParticipantRole::Merchant,
        share_bps: 3000,
        weight_bps: 3000,
        approved: false,
        approved_at: None,
    });
    participants.push_back(Participant {
        address: p3.clone(),
        role: ParticipantRole::ServiceProvider,
        share_bps: 3000,
        weight_bps: 3000,
        approved: false,
        approved_at: None,
    });
    client.create_multi_party_escrow(p1, &participants, &1000_i128, token, &9999_u64)
}

fn setup() -> (Env, EscrowContractClient<'static>, Address) {
    let env = Env::default();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);
    env.ledger().set_timestamp(1000);
    (env, client, admin)
}

fn salt(env: &Env, byte: u8) -> BytesN<32> {
    BytesN::from_array(env, &[byte; 32])
}

fn vote_commitment(
    env: &Env,
    escrow_id: u64,
    voter: &Address,
    vote_choice: u32,
    salt: &BytesN<32>,
) -> BytesN<32> {
    let mut preimage = escrow_id.to_xdr(env);
    preimage.append(&(*voter).clone().to_xdr(env));
    preimage.append(&vote_choice.to_xdr(env));
    preimage.append(&(*salt).clone().to_xdr(env));
    env.crypto().sha256(&preimage).to_bytes()
}

fn commit_vote(
    env: &Env,
    client: &EscrowContractClient,
    escrow_id: u64,
    voter: &Address,
    vote_choice: u32,
    salt: &BytesN<32>,
) {
    let commitment = vote_commitment(env, escrow_id, voter, vote_choice, salt);
    client.commit_dispute_vote(&escrow_id, voter, &commitment);
}

fn create_disputed_escrow(
    env: &Env,
    client: &EscrowContractClient,
) -> (Address, Address, Address, u64) {
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(env))
        .address();
    let p1 = Address::generate(env);
    let p2 = Address::generate(env);
    let p3 = Address::generate(env);
    let escrow_id = make_3_party_escrow(env, client, &token, &p1, &p2, &p3);
    client.dispute_multi_party_escrow(&p1, &escrow_id);
    (p1, p2, p3, escrow_id)
}

#[test]
fn test_dispute_raised_by_participant() {
    let (env, client, _admin) = setup();
    let (p1, _p2, _p3, escrow_id) = create_disputed_escrow(&env, &client);

    let dispute = client.get_multi_party_dispute(&escrow_id);
    assert_eq!(dispute.escrow_id, escrow_id);
    assert!(!dispute.resolved);
    assert_eq!(dispute.quorum_required, 5001);
    assert_eq!(dispute.commit_deadline, 1000 + COMMIT_PERIOD_SECONDS);
    assert_eq!(dispute.reveal_deadline, 1000 + REVEAL_PERIOD_SECONDS);
    assert_eq!(dispute.phase, MultiPartyDisputePhase::CommitPhase);
    assert_eq!(dispute.committed_voters.len(), 0);
    assert_eq!(
        client
            .get_multi_party_escrow(&escrow_id)
            .participants
            .get(0)
            .unwrap()
            .address,
        p1
    );
}

#[test]
fn test_non_participant_cannot_dispute() {
    let (env, client, _admin) = setup();
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    let p3 = Address::generate(&env);
    let outsider = Address::generate(&env);

    let escrow_id = make_3_party_escrow(&env, &client, &token, &p1, &p2, &p3);
    let result = client.try_dispute_multi_party_escrow(&outsider, &escrow_id);
    assert_eq!(result, Err(Ok(Error::Basic(BasicError::Unauthorized))));
}

#[test]
fn test_commit_reveal_resolves_by_weighted_quorum() {
    let (env, client, _admin) = setup();
    let (p1, p2, p3, escrow_id) = create_disputed_escrow(&env, &client);
    let salt1 = salt(&env, 1);
    let salt2 = salt(&env, 2);
    let salt3 = salt(&env, 3);

    commit_vote(&env, &client, escrow_id, &p1, 0, &salt1);
    commit_vote(&env, &client, escrow_id, &p2, 1, &salt2);
    commit_vote(&env, &client, escrow_id, &p3, 1, &salt3);

    let dispute = client.get_multi_party_dispute(&escrow_id);
    assert_eq!(dispute.phase, MultiPartyDisputePhase::RevealPhase);
    assert_eq!(dispute.committed_voters.len(), 3);

    client.reveal_dispute_vote(&escrow_id, &p1, &0, &salt1);
    client.reveal_dispute_vote(&escrow_id, &p2, &1, &salt2);
    client.reveal_dispute_vote(&escrow_id, &p3, &1, &salt3);
    client.resolve_multi_party_dispute(&escrow_id);

    let dispute = client.get_multi_party_dispute(&escrow_id);
    assert!(dispute.resolved);
    assert_eq!(
        client.get_multi_party_escrow(&escrow_id).status,
        EscrowStatus::Released
    );
}

#[test]
fn test_reveal_rejects_invalid_salt() {
    let (env, client, _admin) = setup();
    let (p1, _p2, _p3, escrow_id) = create_disputed_escrow(&env, &client);
    let correct_salt = salt(&env, 4);
    let invalid_salt = salt(&env, 5);
    commit_vote(&env, &client, escrow_id, &p1, 1, &correct_salt);
    env.ledger().set_timestamp(1000 + COMMIT_PERIOD_SECONDS);

    let result = client.try_reveal_dispute_vote(&escrow_id, &p1, &1, &invalid_salt);
    assert_eq!(
        result,
        Err(Ok(Error::Action(ActionError::InvalidVoteCommitment)))
    );
}

#[test]
fn test_copycat_commitment_is_bound_to_voter() {
    let (env, client, _admin) = setup();
    let (p1, p2, _p3, escrow_id) = create_disputed_escrow(&env, &client);
    let vote_salt = salt(&env, 6);
    let p1_commitment = vote_commitment(&env, escrow_id, &p1, 1, &vote_salt);

    client.commit_dispute_vote(&escrow_id, &p1, &p1_commitment);
    client.commit_dispute_vote(&escrow_id, &p2, &p1_commitment);
    env.ledger().set_timestamp(1000 + COMMIT_PERIOD_SECONDS);

    let result = client.try_reveal_dispute_vote(&escrow_id, &p2, &1, &vote_salt);
    assert_eq!(
        result,
        Err(Ok(Error::Action(ActionError::InvalidVoteCommitment)))
    );
    client.reveal_dispute_vote(&escrow_id, &p1, &1, &vote_salt);
}

#[test]
fn test_commit_rejected_after_deadline_and_duplicate_commit_rejected() {
    let (env, client, _admin) = setup();
    let (p1, _p2, _p3, escrow_id) = create_disputed_escrow(&env, &client);
    let vote_salt = salt(&env, 7);
    commit_vote(&env, &client, escrow_id, &p1, 0, &vote_salt);

    let duplicate = vote_commitment(&env, escrow_id, &p1, 0, &vote_salt);
    let result = client.try_commit_dispute_vote(&escrow_id, &p1, &duplicate);
    assert_eq!(result, Err(Ok(Error::Basic(BasicError::DuplicateApproval))));

    env.ledger().set_timestamp(1000 + COMMIT_PERIOD_SECONDS);
    let late_voter = Address::generate(&env);
    let result = client.try_commit_dispute_vote(&escrow_id, &late_voter, &duplicate);
    assert_eq!(
        result,
        Err(Ok(Error::Action(ActionError::VotingPhaseClosed)))
    );
}

#[test]
fn test_no_quorum_waits_for_reveal_deadline_then_escalates() {
    let (env, client, _admin) = setup();
    let (p1, p2, _p3, escrow_id) = create_disputed_escrow(&env, &client);
    let vote_salt = salt(&env, 8);
    commit_vote(&env, &client, escrow_id, &p2, 1, &vote_salt);
    env.ledger().set_timestamp(1000 + COMMIT_PERIOD_SECONDS);
    client.reveal_dispute_vote(&escrow_id, &p2, &1, &vote_salt);

    let early = client.try_resolve_multi_party_dispute(&escrow_id);
    assert_eq!(
        early,
        Err(Ok(Error::Action(ActionError::ApprovalsThresholdNotMet)))
    );

    env.ledger().set_timestamp(1000 + REVEAL_PERIOD_SECONDS);
    client.resolve_multi_party_dispute(&escrow_id);
    let dispute = client.get_multi_party_dispute(&escrow_id);
    assert!(dispute.resolved);
    assert_eq!(
        client.get_multi_party_escrow(&escrow_id).status,
        EscrowStatus::Resolved
    );
    assert_eq!(client.get_reputation(&p2).score, 5000);
    assert_eq!(client.get_reputation(&p1).score, 5000);
}

#[test]
fn test_committed_but_unrevealed_voter_is_penalized() {
    let (env, client, _admin) = setup();
    let (_p1, p2, _p3, escrow_id) = create_disputed_escrow(&env, &client);
    let vote_salt = salt(&env, 9);
    commit_vote(&env, &client, escrow_id, &p2, 1, &vote_salt);

    env.ledger().set_timestamp(1000 + REVEAL_PERIOD_SECONDS);
    client.resolve_multi_party_dispute(&escrow_id);
    assert_eq!(client.get_reputation(&p2).score, 4800);
}
