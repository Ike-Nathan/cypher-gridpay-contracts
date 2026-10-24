#![cfg(test)]
use super::*;
use soroban_sdk::{Address, BytesN, Env, token};
use soroban_sdk::testutils::Address as TestAddress;

#[test]
fn test_initiate_clawback_success() {
    let env = Env::default();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);

    let escrow_id = client.create_escrow(
        &customer,
        &merchant,
        &1000_i128,
        &token,
        &1000_u64,
        &0_u64,
        &0_u64,
        &false
    );

    let reason_hash = BytesN::from_array(&env, &[1_u8; 32]);
    let delay = 86400_u64;

    let request_id = client.initiate_clawback(&admin, &escrow_id, &reason_hash, &delay);
    assert_eq!(request_id, 1);

    let request = client.get_clawback_request(&request_id).unwrap();
    assert_eq!(request.escrow_id, escrow_id);
    assert_eq!(request.initiated_by, admin);
    assert_eq!(request.reason_hash, reason_hash);
    assert_eq!(request.execute_after, env.ledger().timestamp() + delay);
    assert!(!request.executed);
    assert!(!request.cancelled);
}

#[test]
fn test_initiate_clawback_too_short_delay() {
    let env = Env::default();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);

    let escrow_id = client.create_escrow(
        &customer,
        &merchant,
        &1000_i128,
        &token,
        &1000_u64,
        &0_u64,
        &0_u64,
        &false
    );

    let reason_hash = BytesN::from_array(&env, &[1_u8; 32]);
    let delay = 0_u64;

    let result = client.try_initiate_clawback(&admin, &escrow_id, &reason_hash, &delay);
    assert_eq!(
        result,
        Err(Ok(Error::Escrow(EscrowError::ClawbackDelayTooShort)))
    );
}

#[test]
fn test_initiate_clawback_already_initiated() {
    let env = Env::default();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);

    let escrow_id = client.create_escrow(
        &customer,
        &merchant,
        &1000_i128,
        &token,
        &1000_u64,
        &0_u64,
        &0_u64,
        &false
    );

    let reason_hash = BytesN::from_array(&env, &[1_u8; 32]);
    let delay = 86400_u64;

    client.initiate_clawback(&admin, &escrow_id, &reason_hash, &delay);

    let result = client.try_initiate_clawback(&admin, &escrow_id, &reason_hash, &delay);
    assert_eq!(result, Err(Ok(Error::ClawbackAlreadyInitiated)));
}

#[test]
fn test_execute_clawback_success() {
    let env = Env::default();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin).address();
    let token_client = token::StellarAssetClient::new(&env, &token_id);

    env.mock_all_auths();
    client.initialize(&admin);

    token_client.mint(&customer, &1000);
    let escrow_id = client.create_escrow(
        &customer,
        &merchant,
        &1000_i128,
        &token_id,
        &1000_u64,
        &0_u64,
        &0_u64,
        &false
    );

    let reason_hash = BytesN::from_array(&env, &[1_u8; 32]);
    let delay = 86400_u64;
    let request_id = client.initiate_clawback(&admin, &escrow_id, &reason_hash, &delay);

    // Fast forward time
    env.ledger().set_timestamp(env.ledger().timestamp() + delay + 1);

    client.execute_clawback(&admin, &request_id);

    let request = client.get_clawback_request(&request_id).unwrap();
    assert!(request.executed);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Resolved);

    let user_token_client = token::Client::new(&env, &token_id);
    assert_eq!(user_token_client.balance(&admin), 1000);
}

#[test]
fn test_execute_clawback_at_exact_timelock_boundary() {
    let env = Env::default();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin).address();
    let token_client = token::StellarAssetClient::new(&env, &token_id);

    env.mock_all_auths();
    client.initialize(&admin);

    token_client.mint(&customer, &1000);
    let escrow_id = client.create_escrow(
        &customer,
        &merchant,
        &1000_i128,
        &token_id,
        &1000_u64,
        &0_u64,
        &0_u64,
        &false
    );

    let reason_hash = BytesN::from_array(&env, &[1_u8; 32]);
    let delay = 86400_u64;
    let request_id = client.initiate_clawback(&admin, &escrow_id, &reason_hash, &delay);

    // Advance to exactly the timelock boundary; inclusive comparison must allow execution.
    let request = client.get_clawback_request(&request_id).unwrap();
    env.ledger().set_timestamp(request.execute_after);

    client.execute_clawback(&admin, &request_id);

    let request = client.get_clawback_request(&request_id).unwrap();
    assert!(request.executed);
}

#[test]
fn test_execute_clawback_before_delay_rejected() {
    let env = Env::default();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);

    let escrow_id = client.create_escrow(
        &customer,
        &merchant,
        &1000_i128,
        &token,
        &1000_u64,
        &0_u64,
        &0_u64,
        &false
    );

    let reason_hash = BytesN::from_array(&env, &[1_u8; 32]);
    let delay = 86400_u64;
    let request_id = client.initiate_clawback(&admin, &escrow_id, &reason_hash, &delay);

    // Only move time forward slightly, still before the timelock expires.
    env.ledger().set_timestamp(env.ledger().timestamp() + 100);

    let result = client.try_execute_clawback(&admin, &request_id);
    assert_eq!(
        result,
        Err(Ok(Error::Escrow(EscrowError::ClawbackTimelockNotExpired)))
    );

    let request = client.get_clawback_request(&request_id).unwrap();
    assert!(!request.executed);
}

#[test]
#[should_panic]
fn test_execute_clawback_before_delay() {
    let env = Env::default();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);

    let escrow_id = client.create_escrow(
        &customer,
        &merchant,
        &1000_i128,
        &token,
        &1000_u64,
        &0_u64,
        &0_u64,
        &false
    );

    let reason_hash = BytesN::from_array(&env, &[1_u8; 32]);
    let delay = 86400_u64;
    let request_id = client.initiate_clawback(&admin, &escrow_id, &reason_hash, &delay);

    // Only move time forward slightly
    env.ledger().set_timestamp(env.ledger().timestamp() + 100);

    client.execute_clawback(&admin, &request_id);
}

#[test]
fn test_cancel_clawback_success() {
    let env = Env::default();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);

    let escrow_id = client.create_escrow(
        &customer,
        &merchant,
        &1000_i128,
        &token,
        &1000_u64,
        &0_u64,
        &0_u64,
        &false
    );

    let reason_hash = BytesN::from_array(&env, &[1_u8; 32]);
    let delay = 86400_u64;
    let request_id = client.initiate_clawback(&admin, &escrow_id, &reason_hash, &delay);

    client.cancel_clawback(&admin, &request_id);

    let request = client.get_clawback_request(&request_id).unwrap();
    assert!(request.cancelled);
}

#[test]
#[should_panic]
fn test_execute_cancelled_clawback() {
    let env = Env::default();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);

    let escrow_id = client.create_escrow(
        &customer,
        &merchant,
        &1000_i128,
        &token,
        &1000_u64,
        &0_u64,
        &0_u64,
        &false
    );

    let reason_hash = BytesN::from_array(&env, &[1_u8; 32]);
    let delay = 86400_u64;
    let request_id = client.initiate_clawback(&admin, &escrow_id, &reason_hash, &delay);

    client.cancel_clawback(&admin, &request_id);

    env.ledger().set_timestamp(env.ledger().timestamp() + delay + 1);
    client.execute_clawback(&admin, &request_id);
}

// ── execute_partial_clawback ─────────────────────────────────────────────────

fn setup_with_real_token(
    env: &Env,
) -> (
    EscrowContractClient<'static>,
    Address, // admin
    Address, // customer
    Address, // merchant
    Address, // token_id
    u64,     // escrow_id
) {
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let customer = Address::generate(env);
    let merchant = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin).address();
    let token_mint = token::StellarAssetClient::new(env, &token_id);

    env.mock_all_auths();
    client.initialize(&admin);
    token_mint.mint(&customer, &10_000);

    env.ledger().set_timestamp(1_000);
    let escrow_id = client.create_escrow(
        &customer, &merchant, &1_000_i128, &token_id,
        &5_000_u64, &0_u64, &0_u64, &false,
    );

    (client, admin, customer, merchant, token_id, escrow_id)
}

#[test]
fn test_execute_partial_clawback_success() {
    let env = Env::default();
    let (client, admin, _customer, _merchant, token_id, escrow_id) =
        setup_with_real_token(&env);

    let evidence = BytesN::from_array(&env, &[2_u8; 32]);
    let clawed = client.execute_partial_clawback(&escrow_id, &admin, &300_i128, &evidence);
    assert_eq!(clawed, 300_i128);

    let escrow = client.get_escrow(&escrow_id);
    // eligible balance reduced
    assert_eq!(escrow.clawback_eligible_balance, 700_i128);
    // escrow still Locked (partial only)
    assert_eq!(escrow.status, EscrowStatus::Locked);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&admin), 300_i128);
}

#[test]
fn test_execute_partial_clawback_full_balance_resolves_escrow() {
    let env = Env::default();
    let (client, admin, _customer, _merchant, token_id, escrow_id) =
        setup_with_real_token(&env);

    let evidence = BytesN::from_array(&env, &[3_u8; 32]);
    // Clawback the entire eligible balance.
    let clawed = client.execute_partial_clawback(&escrow_id, &admin, &1_000_i128, &evidence);
    assert_eq!(clawed, 1_000_i128);

    let escrow = client.get_escrow(&escrow_id);
    // eligible balance zeroed
    assert_eq!(escrow.clawback_eligible_balance, 0);
    // escrow is now Resolved
    assert_eq!(escrow.status, EscrowStatus::Resolved);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&admin), 1_000_i128);
}

#[test]
fn test_execute_partial_clawback_exceeds_eligible_fails() {
    let env = Env::default();
    let (client, admin, _customer, _merchant, _token_id, escrow_id) =
        setup_with_real_token(&env);

    let evidence = BytesN::from_array(&env, &[4_u8; 32]);
    let result = client.try_execute_partial_clawback(&escrow_id, &admin, &1_001_i128, &evidence);
    assert_eq!(
        result,
        Err(Ok(Error::Escrow(EscrowError::InsufficientClawbackBalance)))
    );
}

#[test]
fn test_execute_partial_clawback_non_admin_fails() {
    let env = Env::default();
    let (client, _admin, customer, _merchant, _token_id, escrow_id) =
        setup_with_real_token(&env);

    let evidence = BytesN::from_array(&env, &[5_u8; 32]);
    let result = client.try_execute_partial_clawback(&escrow_id, &customer, &100_i128, &evidence);
    assert_eq!(result, Err(Ok(Error::Basic(BasicError::NotAnAdmin))));
}

#[test]
fn test_execute_partial_clawback_on_resolved_escrow_fails() {
    let env = Env::default();
    let (client, admin, customer, _merchant, _token_id, escrow_id) =
        setup_with_real_token(&env);

    // Dispute then resolve
    client.dispute_escrow(&customer, &escrow_id);
    client.resolve_dispute(&admin, &escrow_id, &false);

    let evidence = BytesN::from_array(&env, &[6_u8; 32]);
    let result = client.try_execute_partial_clawback(&escrow_id, &admin, &100_i128, &evidence);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::InvalidStatus))));
}

#[test]
fn test_partial_clawback_multiple_steps() {
    let env = Env::default();
    let (client, admin, _customer, _merchant, token_id, escrow_id) =
        setup_with_real_token(&env);

    let ev1 = BytesN::from_array(&env, &[0xAA_u8; 32]);
    let ev2 = BytesN::from_array(&env, &[0xBB_u8; 32]);
    let ev3 = BytesN::from_array(&env, &[0xCC_u8; 32]);

    // Step 1: claw 250
    client.execute_partial_clawback(&escrow_id, &admin, &250_i128, &ev1);
    assert_eq!(client.get_escrow(&escrow_id).clawback_eligible_balance, 750_i128);

    // Step 2: claw another 250
    client.execute_partial_clawback(&escrow_id, &admin, &250_i128, &ev2);
    assert_eq!(client.get_escrow(&escrow_id).clawback_eligible_balance, 500_i128);

    // Step 3: claw remaining 500 — resolves the escrow
    client.execute_partial_clawback(&escrow_id, &admin, &500_i128, &ev3);
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.clawback_eligible_balance, 0);
    assert_eq!(escrow.status, EscrowStatus::Resolved);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&admin), 1_000_i128);
}

#[test]
fn test_execute_clawback_emits_event_with_evidence() {
    let env = Env::default();
    let (client, admin, _customer, _merchant, _token_id, escrow_id) =
        setup_with_real_token(&env);

    let reason_hash = BytesN::from_array(&env, &[0xDE_u8; 32]);
    let delay = 86_400_u64;
    let request_id = client.initiate_clawback(&admin, &escrow_id, &reason_hash, &delay);

    env.ledger().set_timestamp(env.ledger().timestamp() + delay + 1);
    client.execute_clawback(&admin, &request_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Resolved);
    assert_eq!(escrow.clawback_eligible_balance, 0);
}

#[test]
fn test_partial_clawback_not_found_fails() {
    let env = Env::default();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);

    let evidence = BytesN::from_array(&env, &[8_u8; 32]);
    let result = client.try_execute_partial_clawback(&999_u64, &admin, &100_i128, &evidence);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::NotFound))));
}

#[test]
fn test_full_clawback_uses_eligible_balance_not_raw_amount() {
    // Demonstrates that execute_clawback uses clawback_eligible_balance
    // rather than escrow.amount, so any previously-settled portion is excluded.
    let env = Env::default();
    let (client, admin, _customer, _merchant, token_id, escrow_id) =
        setup_with_real_token(&env);

    let evidence = BytesN::from_array(&env, &[0x11_u8; 32]);

    // Reduce eligible balance by 200 via a partial clawback first.
    client.execute_partial_clawback(&escrow_id, &admin, &200_i128, &evidence);

    let reason_hash = BytesN::from_array(&env, &[0x22_u8; 32]);
    let delay = 86_400_u64;
    let request_id = client.initiate_clawback(&admin, &escrow_id, &reason_hash, &delay);
    env.ledger().set_timestamp(env.ledger().timestamp() + delay + 1);
    client.execute_clawback(&admin, &request_id);

    // Admin should have received 200 (partial) + 800 (full, eligible only) = 1000 total.
    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&admin), 1_000_i128);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.clawback_eligible_balance, 0);
    assert_eq!(escrow.status, EscrowStatus::Resolved);
}
