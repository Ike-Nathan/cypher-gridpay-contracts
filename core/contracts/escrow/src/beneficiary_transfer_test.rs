#![cfg(test)]

use super::*;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{testutils::Address as _, token, Address, BytesN, Env};

fn setup() -> (
    Env,
    EscrowContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);
    env.ledger().set_timestamp(1000);
    client.create_escrow(&customer, &merchant, &1000_i128, &token, &2000_u64, &0_u64, &0_u64, &false);

    (env, client, admin, customer, merchant, token)
}

#[test]
fn test_authorized_transfer_by_merchant() {
    let (env, client, _admin, _customer, merchant, _token) = setup();
    let new_merchant = Address::generate(&env);

    client.transfer_escrow_beneficiary(&merchant, &1u64, &new_merchant);

    let escrow = EscrowContract::get_escrow(&env, 1);
    assert_eq!(escrow.merchant, new_merchant);
}

#[test]
fn test_authorized_transfer_by_admin() {
    let (env, client, admin, _customer, _merchant, _token) = setup();
    let new_merchant = Address::generate(&env);

    client.transfer_escrow_beneficiary(&admin, &1u64, &new_merchant);

    let escrow = EscrowContract::get_escrow(&env, 1);
    assert_eq!(escrow.merchant, new_merchant);
}

#[test]
fn test_unauthorized_transfer_rejected() {
    let (env, client, _admin, customer, _merchant, _token) = setup();
    let new_merchant = Address::generate(&env);

    let result = client.try_transfer_escrow_beneficiary(&customer, &1u64, &new_merchant);
    assert_eq!(result, Err(Ok(Error::Basic(BasicError::Unauthorized))));
}

#[test]
fn test_transfer_blocked_on_disputed_escrow() {
    let (env, client, _admin, customer, merchant, _token) = setup();
    let new_merchant = Address::generate(&env);

    client.dispute_escrow(&customer, &1u64);

    let result = client.try_transfer_escrow_beneficiary(&merchant, &1u64, &new_merchant);
    assert_eq!(result, Err(Ok(Error::Action(ActionError::TransferNotAllowed))));
}

#[test]
fn test_transfer_blocked_on_resolved_escrow() {
    let (env, client, admin, customer, merchant, _token) = setup();
    let new_merchant = Address::generate(&env);

    client.dispute_escrow(&customer, &1u64);
    client.resolve_dispute(&admin, &1u64, &false);

    let result = client.try_transfer_escrow_beneficiary(&merchant, &1u64, &new_merchant);
    assert_eq!(result, Err(Ok(Error::Action(ActionError::TransferNotAllowed))));
}

#[test]
fn test_same_beneficiary_rejected() {
    let (_env, client, _admin, _customer, merchant, _token) = setup();

    let result = client.try_transfer_escrow_beneficiary(&merchant, &1u64, &merchant);
    assert_eq!(result, Err(Ok(Error::Action(ActionError::SameBeneficiary))));
}

#[test]
fn test_transfer_history_is_append_only() {
    let (env, client, _admin, _customer, merchant, _token) = setup();
    let new_merchant1 = Address::generate(&env);
    let new_merchant2 = Address::generate(&env);

    client.transfer_escrow_beneficiary(&merchant, &1u64, &new_merchant1);
    client.transfer_escrow_beneficiary(&new_merchant1, &1u64, &new_merchant2);

    let history = client.get_transfer_history(&1u64);
    assert_eq!(history.len(), 2);
    assert_eq!(history.get(0).unwrap().from, merchant);
    assert_eq!(history.get(0).unwrap().to, new_merchant1);
    assert_eq!(history.get(1).unwrap().from, new_merchant1);
    assert_eq!(history.get(1).unwrap().to, new_merchant2);
}

#[test]
fn test_transfer_history_empty_before_any_transfer() {
    let (_env, client, _admin, _customer, _merchant, _token) = setup();
    let history = client.get_transfer_history(&1u64);
    assert_eq!(history.len(), 0);
}

// ── Beneficiary transfer seasoning window ────────────────────────────────────

#[test]
fn test_transfer_records_seasoning_timestamp() {
    let (env, client, _admin, _customer, merchant, _token) = setup();
    let new_merchant = Address::generate(&env);

    // Advance clock so we have a non-zero timestamp.
    env.ledger().set_timestamp(5_000);
    client.transfer_escrow_beneficiary(&merchant, &1u64, &new_merchant);

    let escrow = EscrowContract::get_escrow(&env, 1);
    // beneficiary_transferred_at must be set to the current ledger time.
    assert_eq!(escrow.beneficiary_transferred_at, 5_000);
}

#[test]
fn test_second_transfer_updates_seasoning_timestamp() {
    let (env, client, _admin, _customer, merchant, _token) = setup();
    let new_merchant1 = Address::generate(&env);
    let new_merchant2 = Address::generate(&env);

    env.ledger().set_timestamp(5_000);
    client.transfer_escrow_beneficiary(&merchant, &1u64, &new_merchant1);

    env.ledger().set_timestamp(10_000);
    client.transfer_escrow_beneficiary(&new_merchant1, &1u64, &new_merchant2);

    let escrow = EscrowContract::get_escrow(&env, 1);
    // Seasoning timestamp must be refreshed on each transfer.
    assert_eq!(escrow.beneficiary_transferred_at, 10_000);
}

#[test]
fn test_partial_clawback_allowed_during_seasoning_window() {
    let env = Env::default();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token_admin_addr = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin_addr.clone())
        .address();
    let token_client = token::StellarAssetClient::new(&env, &token_id);

    env.mock_all_auths();
    client.initialize(&admin);
    token_client.mint(&customer, &2_000);

    env.ledger().set_timestamp(1_000);
    let escrow_id = client.create_escrow(
        &customer, &merchant, &1_000_i128, &token_id,
        &5_000_u64, &0_u64, &0_u64, &false,
    );

    // Transfer beneficiary — starts the seasoning window.
    let new_merchant = Address::generate(&env);
    env.ledger().set_timestamp(2_000);
    client.transfer_escrow_beneficiary(&merchant, &escrow_id, &new_merchant);

    // Partial clawback is still allowed during the seasoning window.
    let evidence = BytesN::from_array(&env, &[9_u8; 32]);
    env.ledger().set_timestamp(3_000); // well inside 24 h window
    let clawed = client.execute_partial_clawback(&escrow_id, &admin, &100_i128, &evidence);
    assert_eq!(clawed, 100_i128);

    let escrow = client.get_escrow(&escrow_id);
    // Eligible balance reduced by the clawed amount.
    assert_eq!(escrow.clawback_eligible_balance, 900_i128);
    // Escrow still Locked (not fully clawed).
    assert_eq!(escrow.status, EscrowStatus::Locked);
}

#[test]
fn test_partial_clawback_exceeding_eligible_fails() {
    let env = Env::default();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);
    env.ledger().set_timestamp(1_000);
    let escrow_id = client.create_escrow(
        &customer, &merchant, &500_i128, &token,
        &2_000_u64, &0_u64, &0_u64, &false,
    );

    let evidence = BytesN::from_array(&env, &[7_u8; 32]);
    // Attempt to claw back more than the eligible balance.
    let result = client.try_execute_partial_clawback(
        &escrow_id, &admin, &600_i128, &evidence,
    );
    assert_eq!(
        result,
        Err(Ok(Error::Escrow(EscrowError::InsufficientClawbackBalance)))
    );
}

#[test]
fn test_no_seasoning_before_transfer() {
    let (env, client, _admin, _customer, _merchant, _token) = setup();

    let escrow = EscrowContract::get_escrow(&env, 1);
    // Fresh escrow has zero beneficiary_transferred_at.
    assert_eq!(escrow.beneficiary_transferred_at, 0);
    assert_eq!(escrow.beneficiary_seasoning_secs, 0);
}
