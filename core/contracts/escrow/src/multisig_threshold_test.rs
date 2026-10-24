#![cfg(test)]

use crate::*;
use soroban_sdk::{testutils::Address as _, Address, Bytes, Env};

fn setup() -> (Env, EscrowContractClient<'static>, Address) {
    let env = Env::default();
    let id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);
    (env, client, admin)
}

#[test]
fn test_direct_update_required_signatures_rejected() {
    let (env, client, admin) = setup();
    let _ = env;
    let result = client.try_update_required_signatures(&admin, &1u32);
    assert_eq!(result, Err(Ok(Error::Basic(BasicError::Unauthorized))));
}

#[test]
fn test_threshold_change_via_multisig() {
    let (env, client, admin) = setup();

    let admin2 = Address::generate(&env);
    client.add_admin(&admin, &admin2);

    // With threshold=1 (default), one approval from proposer is enough to execute.
    let proposal_id = client.propose_action(
        &admin,
        &ActionType::UpdateThreshold(2u32),
        &admin2,
        &Bytes::new(&env),
    );
    client.execute_action(&proposal_id);

    let config = client.get_multisig_config();
    assert_eq!(config.required_signatures, 2);
}

#[test]
fn test_threshold_zero_rejected() {
    let (env, client, admin) = setup();

    let proposal_id = client.propose_action(
        &admin,
        &ActionType::UpdateThreshold(0u32),
        &admin,
        &Bytes::new(&env),
    );

    let result = client.try_execute_action(&proposal_id);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::InvalidThreshold))));
}

#[test]
fn test_threshold_exceeds_admin_count_rejected() {
    let (env, client, admin) = setup();

    // Only 1 admin exists; requesting threshold=2 should fail.
    let proposal_id = client.propose_action(
        &admin,
        &ActionType::UpdateThreshold(2u32),
        &admin,
        &Bytes::new(&env),
    );

    let result = client.try_execute_action(&proposal_id);
    assert_eq!(
        result,
        Err(Ok(Error::Basic(BasicError::InsufficientAdmins)))
    );
}

#[test]
fn test_threshold_increase_invalidates_pending_proposal_approvals() {
    let (env, client, admin) = setup();

    let admin2 = Address::generate(&env);
    client.add_admin(&admin, &admin2);

    // A pending action that collected its approvals under the old threshold.
    let pending = client.propose_action(
        &admin,
        &ActionType::UpdateThreshold(1u32),
        &admin,
        &Bytes::new(&env),
    );

    // Raise the required signatures to 2 through a separate proposal.
    let raise = client.propose_action(
        &admin,
        &ActionType::UpdateThreshold(2u32),
        &admin,
        &Bytes::new(&env),
    );
    client.execute_action(&raise);
    assert_eq!(client.get_multisig_config().required_signatures, 2);

    // The stale proposal cannot ride on approvals gathered under the old rule.
    let stale = client.try_execute_action(&pending);
    assert_eq!(
        stale,
        Err(Ok(Error::Escrow(EscrowError::InvalidStatus)))
    );

    // Re-approving restarts collection under the new threshold: one admin is
    // still not enough, two are.
    client.approve_action(&admin, &pending);
    let still_short = client.try_execute_action(&pending);
    assert_eq!(
        still_short,
        Err(Ok(Error::Escrow(EscrowError::InvalidStatus)))
    );

    client.approve_action(&admin2, &pending);
    client.execute_action(&pending);
    assert_eq!(client.get_multisig_config().required_signatures, 1);
}

// ── Issue #219: Timelocked participant key rotation & recovery guardian tests ─

use soroban_sdk::{token, testutils::Ledger, Vec as SorobanVec};

/// Creates a minimal two-participant multi-party escrow and returns its ID.
/// Mints `total_amount` tokens to the contract address so later transfers succeed.
fn make_multi_party_escrow(
    env: &Env,
    client: &EscrowContractClient,
    customer: &Address,
    p1: &Address,
    p2: &Address,
    token_addr: &Address,
    total_amount: i128,
) -> u64 {
    // Build participants: p1 and p2 split 50/50 for both share and weight.
    let mut participants = SorobanVec::new(env);
    participants.push_back(Participant {
        address: p1.clone(),
        role: ParticipantRole::Customer,
        share_bps: 5_000u32,
        weight_bps: 5_000u32,
        approved: false,
        approved_at: None,
    });
    participants.push_back(Participant {
        address: p2.clone(),
        role: ParticipantRole::Merchant,
        share_bps: 5_000u32,
        weight_bps: 5_000u32,
        approved: false,
        approved_at: None,
    });

    // Fund customer so the deposit transfer inside create_multi_party_escrow works.
    let token_admin = token::StellarAssetClient::new(env, token_addr);
    token_admin.mint(customer, &total_amount);

    client.create_multi_party_escrow(customer, &participants, &total_amount, token_addr, &0u64)
}

/// Shared setup: env + contract + admin + token.
fn setup_with_token() -> (Env, EscrowContractClient<'static>, Address, Address) {
    let env = Env::default();
    let id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);
    let token_addr = env.register_stellar_asset_contract(admin.clone());
    (env, client, admin, token_addr)
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. Happy-path: owner proposes, timelock elapses, rotation executes
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_owner_key_rotation_succeeds_after_timelock() {
    let (env, client, _admin, token_addr) = setup_with_token();
    let customer = Address::generate(&env);
    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    let p1_new = Address::generate(&env);

    env.ledger().set_timestamp(1_000);
    let escrow_id =
        make_multi_party_escrow(&env, &client, &customer, &p1, &p2, &token_addr, 2_000);

    // p1 (owner) proposes their own rotation with a 300-second timelock.
    env.ledger().set_timestamp(2_000);
    client.propose_participant_key_rotation(&p1, &escrow_id, &p1, &p1_new, &300u64);

    // Cannot execute before the timelock expires.
    env.ledger().set_timestamp(2_200);
    let too_early = client.try_execute_participant_key_rotation(&escrow_id, &p1);
    assert_eq!(
        too_early,
        Err(Ok(Error::Escrow(EscrowError::TimeoutNotReached)))
    );

    // After timelock: execute succeeds.
    env.ledger().set_timestamp(2_000 + 301);
    client.execute_participant_key_rotation(&escrow_id, &p1);

    let escrow = client.get_multi_party_escrow(&escrow_id);
    // p1 must be gone; p1_new must be present.
    let mut found_old = false;
    let mut found_new = false;
    for p in escrow.participants.iter() {
        if p.address == p1 { found_old = true; }
        if p.address == p1_new { found_new = true; }
    }
    assert!(!found_old);
    assert!(found_new);
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Guardian-initiated rotation succeeds
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_guardian_can_propose_rotation() {
    let (env, client, _admin, token_addr) = setup_with_token();
    let customer = Address::generate(&env);
    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    let guardian = Address::generate(&env);
    let p1_new = Address::generate(&env);

    env.ledger().set_timestamp(1_000);
    let escrow_id =
        make_multi_party_escrow(&env, &client, &customer, &p1, &p2, &token_addr, 2_000);

    // p1 registers a guardian.
    client.register_recovery_guardian(&p1, &escrow_id, &guardian);

    // Guardian proposes rotation on behalf of p1.
    env.ledger().set_timestamp(2_000);
    client.propose_participant_key_rotation(&guardian, &escrow_id, &p1, &p1_new, &100u64);

    env.ledger().set_timestamp(2_000 + 101);
    client.execute_participant_key_rotation(&escrow_id, &p1);

    let escrow = client.get_multi_party_escrow(&escrow_id);
    let mut found_new = false;
    let mut found_old = false;
    for p in escrow.participants.iter() {
        if p.address == p1_new { found_new = true; }
        if p.address == p1 { found_old = true; }
    }
    assert!(found_new);
    assert!(!found_old);
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Unauthorised proposer rejected
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_unauthorised_proposer_rejected() {
    let (env, client, _admin, token_addr) = setup_with_token();
    let customer = Address::generate(&env);
    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    let rando = Address::generate(&env);
    let p1_new = Address::generate(&env);

    env.ledger().set_timestamp(1_000);
    let escrow_id =
        make_multi_party_escrow(&env, &client, &customer, &p1, &p2, &token_addr, 2_000);

    let result = client
        .try_propose_participant_key_rotation(&rando, &escrow_id, &p1, &p1_new, &300u64);
    assert_eq!(result, Err(Ok(Error::Basic(BasicError::Unauthorized))));
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. Owner (old participant) veto blocks execution
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_owner_veto_blocks_execution() {
    let (env, client, _admin, token_addr) = setup_with_token();
    let customer = Address::generate(&env);
    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    let guardian = Address::generate(&env);
    let p1_new = Address::generate(&env);

    env.ledger().set_timestamp(1_000);
    let escrow_id =
        make_multi_party_escrow(&env, &client, &customer, &p1, &p2, &token_addr, 2_000);

    // Register guardian and let guardian propose rotation.
    client.register_recovery_guardian(&p1, &escrow_id, &guardian);
    env.ledger().set_timestamp(2_000);
    client.propose_participant_key_rotation(&guardian, &escrow_id, &p1, &p1_new, &600u64);

    // p1 vetoes the guardian's proposal while timelock is still open.
    env.ledger().set_timestamp(2_100);
    client.veto_participant_key_rotation(&p1, &escrow_id, &p1);

    // After timelock, execution still fails because veto was filed.
    env.ledger().set_timestamp(2_000 + 601);
    let result = client.try_execute_participant_key_rotation(&escrow_id, &p1);
    assert_eq!(
        result,
        Err(Ok(Error::Escrow(EscrowError::RotationTimelockActive)))
    );

    // p1 should still be a participant.
    let escrow = client.get_multi_party_escrow(&escrow_id);
    let mut found_old = false;
    for p in escrow.participants.iter() {
        if p.address == p1 { found_old = true; }
    }
    assert!(found_old);
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. Counterparty (another participant) veto blocks execution
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_counterparty_veto_blocks_execution() {
    let (env, client, _admin, token_addr) = setup_with_token();
    let customer = Address::generate(&env);
    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    let p1_new = Address::generate(&env);

    env.ledger().set_timestamp(1_000);
    let escrow_id =
        make_multi_party_escrow(&env, &client, &customer, &p1, &p2, &token_addr, 2_000);

    env.ledger().set_timestamp(2_000);
    client.propose_participant_key_rotation(&p1, &escrow_id, &p1, &p1_new, &300u64);

    // p2 (counterparty) vetoes.
    env.ledger().set_timestamp(2_100);
    client.veto_participant_key_rotation(&p2, &escrow_id, &p1);

    env.ledger().set_timestamp(2_000 + 301);
    let result = client.try_execute_participant_key_rotation(&escrow_id, &p1);
    assert_eq!(
        result,
        Err(Ok(Error::Escrow(EscrowError::RotationTimelockActive)))
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 6. Duplicate new_participant rejected
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_duplicate_new_participant_rejected() {
    let (env, client, _admin, token_addr) = setup_with_token();
    let customer = Address::generate(&env);
    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);

    env.ledger().set_timestamp(1_000);
    let escrow_id =
        make_multi_party_escrow(&env, &client, &customer, &p1, &p2, &token_addr, 2_000);

    // Attempting to rotate p1 → p2 (p2 already exists) must fail.
    let result =
        client.try_propose_participant_key_rotation(&p1, &escrow_id, &p1, &p2, &300u64);
    assert_eq!(
        result,
        Err(Ok(Error::Escrow(EscrowError::DuplicateParticipant)))
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 7. Weight is migrated to new participant, old weight removed
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_vote_weight_migrated_after_rotation() {
    let (env, client, admin, token_addr) = setup_with_token();
    let customer = Address::generate(&env);
    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    let p1_new = Address::generate(&env);

    env.ledger().set_timestamp(1_000);
    let escrow_id =
        make_multi_party_escrow(&env, &client, &customer, &p1, &p2, &token_addr, 2_000);

    // Manually set p1's weight to 6_000 so we can confirm migration.
    // Adjust p2 to 4_000 to keep total at 10_000.
    client.set_participant_weight(&admin, &escrow_id, &p1, &6_000u32);
    client.set_participant_weight(&admin, &escrow_id, &p2, &4_000u32);

    env.ledger().set_timestamp(2_000);
    client.propose_participant_key_rotation(&p1, &escrow_id, &p1, &p1_new, &100u64);
    env.ledger().set_timestamp(2_000 + 101);
    client.execute_participant_key_rotation(&escrow_id, &p1);

    let escrow = client.get_multi_party_escrow(&escrow_id);
    // Confirm p1_new carries weight_bps == 6_000 in the embedded struct.
    let mut new_weight = 0u32;
    for p in escrow.participants.iter() {
        if p.address == p1_new {
            new_weight = p.weight_bps;
        }
    }
    assert_eq!(new_weight, 6_000u32, "weight must migrate to new address");
}

// ─────────────────────────────────────────────────────────────────────────────
// 8. Active approval by old participant is cleared on rotation
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_stale_approval_cleared_after_rotation() {
    let (env, client, _admin, token_addr) = setup_with_token();
    let customer = Address::generate(&env);
    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    let p1_new = Address::generate(&env);

    env.ledger().set_timestamp(1_000);
    let escrow_id =
        make_multi_party_escrow(&env, &client, &customer, &p1, &p2, &token_addr, 2_000);

    // p1 approves release before their key is rotated.
    client.approve_release(&p1, &escrow_id);

    env.ledger().set_timestamp(2_000);
    client.propose_participant_key_rotation(&p1, &escrow_id, &p1, &p1_new, &100u64);
    env.ledger().set_timestamp(2_000 + 101);
    client.execute_participant_key_rotation(&escrow_id, &p1);

    let escrow = client.get_multi_party_escrow(&escrow_id);
    // p1_new should show approved=false (stale approval cleared).
    for p in escrow.participants.iter() {
        if p.address == p1_new {
            assert!(!p.approved, "stale approval must be cleared after rotation");
        }
    }
    // p1 must not appear in the approvals dedup list.
    assert!(!escrow.approvals.contains(&p1));
}

// ─────────────────────────────────────────────────────────────────────────────
// 9. Duplicate pending rotation rejected
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_duplicate_pending_rotation_rejected() {
    let (env, client, _admin, token_addr) = setup_with_token();
    let customer = Address::generate(&env);
    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    let p1_new = Address::generate(&env);
    let p1_new2 = Address::generate(&env);

    env.ledger().set_timestamp(1_000);
    let escrow_id =
        make_multi_party_escrow(&env, &client, &customer, &p1, &p2, &token_addr, 2_000);

    env.ledger().set_timestamp(2_000);
    client.propose_participant_key_rotation(&p1, &escrow_id, &p1, &p1_new, &300u64);

    // A second proposal for the same old_participant while first is pending.
    let result =
        client.try_propose_participant_key_rotation(&p1, &escrow_id, &p1, &p1_new2, &300u64);
    assert_eq!(
        result,
        Err(Ok(Error::Escrow(EscrowError::RotationTimelockActive)))
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 10. Veto after timelock window is closed is itself rejected
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_veto_after_window_closes_rejected() {
    let (env, client, _admin, token_addr) = setup_with_token();
    let customer = Address::generate(&env);
    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    let p1_new = Address::generate(&env);

    env.ledger().set_timestamp(1_000);
    let escrow_id =
        make_multi_party_escrow(&env, &client, &customer, &p1, &p2, &token_addr, 2_000);

    env.ledger().set_timestamp(2_000);
    client.propose_participant_key_rotation(&p1, &escrow_id, &p1, &p1_new, &300u64);

    // Advance past the timelock window — veto is now too late.
    env.ledger().set_timestamp(2_000 + 301);
    let result = client.try_veto_participant_key_rotation(&p2, &escrow_id, &p1);
    assert_eq!(
        result,
        Err(Ok(Error::Escrow(EscrowError::RotationTimelockActive)))
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 11. Non-participant cannot veto
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_non_participant_cannot_veto() {
    let (env, client, _admin, token_addr) = setup_with_token();
    let customer = Address::generate(&env);
    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    let p1_new = Address::generate(&env);
    let outsider = Address::generate(&env);

    env.ledger().set_timestamp(1_000);
    let escrow_id =
        make_multi_party_escrow(&env, &client, &customer, &p1, &p2, &token_addr, 2_000);

    env.ledger().set_timestamp(2_000);
    client.propose_participant_key_rotation(&p1, &escrow_id, &p1, &p1_new, &300u64);

    let result = client.try_veto_participant_key_rotation(&outsider, &escrow_id, &p1);
    assert_eq!(result, Err(Ok(Error::Basic(BasicError::Unauthorized))));
}

// ─────────────────────────────────────────────────────────────────────────────
// 12. Guardian registration requires caller to be a participant
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_non_participant_cannot_register_guardian() {
    let (env, client, _admin, token_addr) = setup_with_token();
    let customer = Address::generate(&env);
    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    let outsider = Address::generate(&env);
    let guardian = Address::generate(&env);

    env.ledger().set_timestamp(1_000);
    let escrow_id =
        make_multi_party_escrow(&env, &client, &customer, &p1, &p2, &token_addr, 2_000);

    let result = client.try_register_recovery_guardian(&outsider, &escrow_id, &guardian);
    assert_eq!(result, Err(Ok(Error::Basic(BasicError::Unauthorized))));
}
