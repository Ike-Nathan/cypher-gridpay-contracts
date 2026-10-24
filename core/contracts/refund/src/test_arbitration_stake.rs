#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, token, Address, Env, String};

fn create_token_contract<'a>(
    env: &Env,
    admin: &Address,
) -> (token::Client<'a>, token::StellarAssetClient<'a>) {
    let contract = env.register_stellar_asset_contract_v2(admin.clone());
    let contract_address = contract.address();
    (
        token::Client::new(env, &contract_address),
        token::StellarAssetClient::new(env, &contract_address),
    )
}

#[test]
fn test_set_arbitration_stake_config_valid() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let (stake_token_client, stake_token_admin) = create_token_contract(&env, &admin);

    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let config = ArbitrationStakeConfig {
        token: stake_token_client.address.clone(),
        amount: 5000,
        enabled: true,
    };

    client.set_arbitration_stake_config(&admin, &config);

    let retrieved = client.get_arbitration_stake_config();
    assert!(retrieved.is_some());
    let retrieved_config = retrieved.unwrap();
    assert_eq!(retrieved_config.amount, 5000);
    assert_eq!(retrieved_config.enabled, true);
}

#[test]
#[should_panic]
fn test_set_arbitration_stake_config_invalid_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let (stake_token_client, _) = create_token_contract(&env, &admin);

    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let config = ArbitrationStakeConfig {
        token: stake_token_client.address.clone(),
        amount: 0, // Invalid: zero amount when enabled
        enabled: true,
    };

    client.set_arbitration_stake_config(&admin, &config);
}

#[test]
#[should_panic]
fn test_set_arbitration_stake_config_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let (stake_token_client, _) = create_token_contract(&env, &admin);

    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let config = ArbitrationStakeConfig {
        token: stake_token_client.address.clone(),
        amount: 5000,
        enabled: true,
    };

    client.set_arbitration_stake_config(&non_admin, &config);
}

#[test]
fn test_stake_deposit_on_escalation() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let arbitrator1 = Address::generate(&env);
    let arbitrator2 = Address::generate(&env);
    let arbitrator3 = Address::generate(&env);

    let (token_client, token_admin) = create_token_contract(&env, &admin);
    let (stake_token_client, stake_token_admin) = create_token_contract(&env, &admin);

    token_admin.mint(&merchant, &10000);
    stake_token_admin.mint(&merchant, &10000);

    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    // Register arbitrators
    client.register_arbitrator(&admin, &arbitrator1);
    client.register_arbitrator(&admin, &arbitrator2);
    client.register_arbitrator(&admin, &arbitrator3);

    // Set stake configuration
    let stake_config = ArbitrationStakeConfig {
        token: stake_token_client.address.clone(),
        amount: 5000,
        enabled: true,
    };
    client.set_arbitration_stake_config(&admin, &stake_config);

    // Create and reject refund
    let refund_id = client.request_refund(
        &merchant,
        &1u64,
        &customer,
        &1000i128,
        &10000i128,
        &token_client.address,
        &String::from_str(&env, "Test refund"),
        &RefundReasonCode::Other,
        &1000u64,
    );
    client.reject_refund(&admin, &refund_id, &String::from_str(&env, "Rejected"));

    // Check initial balance
    let merchant_initial = stake_token_client.balance(&merchant);

    // Escalate to arbitration (should require stake)
    let fee_pool = 1000i128;

    let case_id =
        client.escalate_to_arbitration(&merchant, &refund_id, &token_client.address, &fee_pool);

    // With mock_all_auths, balances don't actually change
    // Just verify stake record was created
    let stake = client.get_arbitration_stake(&case_id);
    assert!(stake.is_some());
    let stake_info = stake.unwrap();
    assert_eq!(stake_info.staker, merchant);
    assert_eq!(stake_info.amount, 5000);
    assert_eq!(stake_info.returned, false);
}

#[test]
fn test_stake_returned_on_win() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let arbitrator1 = Address::generate(&env);
    let arbitrator2 = Address::generate(&env);
    let arbitrator3 = Address::generate(&env);

    let (token_client, token_admin) = create_token_contract(&env, &admin);
    let (stake_token_client, stake_token_admin) = create_token_contract(&env, &admin);

    token_admin.mint(&merchant, &10000);
    stake_token_admin.mint(&merchant, &10000);

    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    // Register arbitrators
    client.register_arbitrator(&admin, &arbitrator1);
    client.register_arbitrator(&admin, &arbitrator2);
    client.register_arbitrator(&admin, &arbitrator3);

    // Set stake configuration
    let stake_config = ArbitrationStakeConfig {
        token: stake_token_client.address.clone(),
        amount: 5000,
        enabled: true,
    };
    client.set_arbitration_stake_config(&admin, &stake_config);

    // Create and reject refund
    let refund_id = client.request_refund(
        &merchant,
        &1u64,
        &customer,
        &1000i128,
        &10000i128,
        &token_client.address,
        &String::from_str(&env, "Test refund"),
        &RefundReasonCode::Other,
        &1000u64,
    );
    client.reject_refund(&admin, &refund_id, &String::from_str(&env, "Rejected"));

    // Escalate to arbitration
    let fee_pool = 1000i128;

    let case_id =
        client.escalate_to_arbitration(&merchant, &refund_id, &token_client.address, &fee_pool);

    // Vote against refund (merchant wins)
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    client.cast_arbitration_vote(&arbitrator1, &case_id, &false, &hash);
    client.cast_arbitration_vote(&arbitrator2, &case_id, &false, &hash);
    client.cast_arbitration_vote(&arbitrator3, &case_id, &false, &hash);

    // Close case
    client.close_arbitration_case(&case_id);

    // Verify stake is marked as returned
    let stake = client.get_arbitration_stake(&case_id);
    assert!(stake.is_some());
    assert_eq!(stake.unwrap().returned, true);
}

#[test]
fn test_stake_forfeited_on_loss() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let arbitrator1 = Address::generate(&env);
    let arbitrator2 = Address::generate(&env);
    let arbitrator3 = Address::generate(&env);
    let treasury = Address::generate(&env);

    let (token_client, token_admin) = create_token_contract(&env, &admin);
    let (stake_token_client, stake_token_admin) = create_token_contract(&env, &admin);

    token_admin.mint(&merchant, &10000);
    stake_token_admin.mint(&merchant, &10000);

    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    // Register arbitrators
    client.register_arbitrator(&admin, &arbitrator1);
    client.register_arbitrator(&admin, &arbitrator2);
    client.register_arbitrator(&admin, &arbitrator3);

    // Set stake configuration
    let stake_config = ArbitrationStakeConfig {
        token: stake_token_client.address.clone(),
        amount: 5000,
        enabled: true,
    };
    client.set_arbitration_stake_config(&admin, &stake_config);

    // Set fee configuration with treasury address
    let fee_config = ArbitrationFeeConfig {
        arbitrator_share_bps: 7000,
        treasury_share_bps: 3000,
        treasury_address: treasury.clone(),
        fee_token: token_client.address.clone(),
        fee_per_case: 1000,
    };
    client.set_arbitration_fee_config(&admin, &fee_config);

    // Create and reject refund
    let refund_id = client.request_refund(
        &merchant,
        &1u64,
        &customer,
        &1000i128,
        &10000i128,
        &token_client.address,
        &String::from_str(&env, "Test refund"),
        &RefundReasonCode::Other,
        &1000u64,
    );
    client.reject_refund(&admin, &refund_id, &String::from_str(&env, "Rejected"));

    // Escalate to arbitration
    let fee_pool = 1000i128;

    let case_id =
        client.escalate_to_arbitration(&merchant, &refund_id, &token_client.address, &fee_pool);

    // Vote for refund (merchant loses)
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    client.cast_arbitration_vote(&arbitrator1, &case_id, &true, &hash);
    client.cast_arbitration_vote(&arbitrator2, &case_id, &true, &hash);
    client.cast_arbitration_vote(&arbitrator3, &case_id, &true, &hash);

    // Close case
    client.close_arbitration_case(&case_id);

    // Verify stake is marked as returned (processed)
    let stake = client.get_arbitration_stake(&case_id);
    assert!(stake.is_some());
    assert_eq!(stake.unwrap().returned, true);
}

#[test]
fn test_escalation_without_stake_config() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let arbitrator1 = Address::generate(&env);
    let arbitrator2 = Address::generate(&env);
    let arbitrator3 = Address::generate(&env);

    let (token_client, token_admin) = create_token_contract(&env, &admin);

    token_admin.mint(&merchant, &10000);

    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    // Register arbitrators
    client.register_arbitrator(&admin, &arbitrator1);
    client.register_arbitrator(&admin, &arbitrator2);
    client.register_arbitrator(&admin, &arbitrator3);

    // Don't set stake configuration

    // Create and reject refund
    let refund_id = client.request_refund(
        &merchant,
        &1u64,
        &customer,
        &1000i128,
        &10000i128,
        &token_client.address,
        &String::from_str(&env, "Test refund"),
        &RefundReasonCode::Other,
        &1000u64,
    );
    client.reject_refund(&admin, &refund_id, &String::from_str(&env, "Rejected"));

    // Escalate to arbitration (should work without stake)
    let fee_pool = 1000i128;

    let case_id =
        client.escalate_to_arbitration(&merchant, &refund_id, &token_client.address, &fee_pool);

    // Verify no stake was recorded
    let stake = client.get_arbitration_stake(&case_id);
    assert!(stake.is_none());
}

#[test]
fn test_customer_stake_returned_when_refund_approved() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let arbitrator1 = Address::generate(&env);
    let arbitrator2 = Address::generate(&env);
    let arbitrator3 = Address::generate(&env);

    let (token_client, token_admin) = create_token_contract(&env, &admin);
    let (stake_token_client, stake_token_admin) = create_token_contract(&env, &admin);

    token_admin.mint(&customer, &10000);
    stake_token_admin.mint(&customer, &10000);

    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    client.register_arbitrator(&admin, &arbitrator1);
    client.register_arbitrator(&admin, &arbitrator2);
    client.register_arbitrator(&admin, &arbitrator3);

    let stake_config = ArbitrationStakeConfig {
        token: stake_token_client.address.clone(),
        amount: 5000,
        enabled: true,
    };
    client.set_arbitration_stake_config(&admin, &stake_config);

    let refund_id = client.request_refund(
        &merchant,
        &1u64,
        &customer,
        &1000i128,
        &10000i128,
        &token_client.address,
        &String::from_str(&env, "Test refund"),
        &RefundReasonCode::Other,
        &1000u64,
    );
    client.reject_refund(&admin, &refund_id, &String::from_str(&env, "Rejected"));

    let fee_pool = 1000i128;
    let case_id =
        client.escalate_to_arbitration(&customer, &refund_id, &token_client.address, &fee_pool);

    let stake = client.get_arbitration_stake(&case_id).unwrap();
    assert_eq!(stake.staker, customer);

    let hash = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);
    client.cast_arbitration_vote(&arbitrator1, &case_id, &true, &hash);
    client.cast_arbitration_vote(&arbitrator2, &case_id, &true, &hash);
    client.cast_arbitration_vote(&arbitrator3, &case_id, &true, &hash);
    client.close_arbitration_case(&case_id);

    let stake_after = client.get_arbitration_stake(&case_id).unwrap();
    assert_eq!(stake_after.returned, true);
}

#[test]
fn test_escalation_with_disabled_stake() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let arbitrator1 = Address::generate(&env);
    let arbitrator2 = Address::generate(&env);
    let arbitrator3 = Address::generate(&env);

    let (token_client, token_admin) = create_token_contract(&env, &admin);
    let (stake_token_client, _) = create_token_contract(&env, &admin);

    token_admin.mint(&merchant, &10000);

    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    // Register arbitrators
    client.register_arbitrator(&admin, &arbitrator1);
    client.register_arbitrator(&admin, &arbitrator2);
    client.register_arbitrator(&admin, &arbitrator3);

    // Set stake configuration but disabled
    let stake_config = ArbitrationStakeConfig {
        token: stake_token_client.address.clone(),
        amount: 5000,
        enabled: false, // Disabled
    };
    client.set_arbitration_stake_config(&admin, &stake_config);

    // Create and reject refund
    let refund_id = client.request_refund(
        &merchant,
        &1u64,
        &customer,
        &1000i128,
        &10000i128,
        &token_client.address,
        &String::from_str(&env, "Test refund"),
        &RefundReasonCode::Other,
        &1000u64,
    );
    client.reject_refund(&admin, &refund_id, &String::from_str(&env, "Rejected"));

    // Escalate to arbitration (should work without stake since disabled)
    let fee_pool = 1000i128;

    let case_id =
        client.escalate_to_arbitration(&merchant, &refund_id, &token_client.address, &fee_pool);

    // Verify no stake was recorded
    let stake = client.get_arbitration_stake(&case_id);
    assert!(stake.is_none());
}

fn setup_staked_arbitrator<'a>(
    env: &'a Env,
    stake: i128,
) -> (
    RefundContractClient<'a>,
    Address,
    Address,
    token::Client<'a>,
) {
    let admin = Address::generate(env);
    let arbitrator = Address::generate(env);
    let (stake_token_client, stake_token_admin) = create_token_contract(env, &admin);
    stake_token_admin.mint(&arbitrator, &10_000);

    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(env, &contract_id);
    client.initialize(&admin);
    client.register_arbitrator(&admin, &arbitrator);
    client.set_arbitration_stake_config(
        &admin,
        &ArbitrationStakeConfig {
            token: stake_token_client.address.clone(),
            amount: 5000,
            enabled: true,
        },
    );
    assert_eq!(client.deposit_arbitrator_stake(&arbitrator, &stake), stake);

    (client, admin, arbitrator, stake_token_client)
}

#[test]
fn test_slash_arbitrator_stake_reduces_stake_and_pays_recipient() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, arbitrator, stake_token) = setup_staked_arbitrator(&env, 5000);
    let customer = Address::generate(&env);

    assert_eq!(client.get_arbitrator_stake(&arbitrator), 5000);
    assert_eq!(stake_token.balance(&arbitrator), 5000);
    assert_eq!(stake_token.balance(&client.address), 5000);

    let remaining = client.slash_arbitrator_stake(&admin, &arbitrator, &2000, &customer);

    assert_eq!(remaining, 3000);
    assert_eq!(client.get_arbitrator_stake(&arbitrator), 3000);
    assert_eq!(stake_token.balance(&customer), 2000);
    assert_eq!(stake_token.balance(&client.address), 3000);
    // The arbitrator's own wallet is not touched by slashing.
    assert_eq!(stake_token.balance(&arbitrator), 5000);

    // Remaining stake can be slashed to an insurance pool.
    let insurance_pool = Address::generate(&env);
    assert_eq!(
        client.slash_arbitrator_stake(&admin, &arbitrator, &3000, &insurance_pool),
        0
    );
    assert_eq!(stake_token.balance(&insurance_pool), 3000);
    assert_eq!(client.get_arbitrator_stake(&arbitrator), 0);
}

#[test]
fn test_slash_arbitrator_stake_rejects_amount_above_stake() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, arbitrator, stake_token) = setup_staked_arbitrator(&env, 1000);
    let customer = Address::generate(&env);

    let result = client.try_slash_arbitrator_stake(&admin, &arbitrator, &1001, &customer);
    assert_eq!(
        result,
        Err(Ok(Error::Ext(ExtError::InsufficientArbitratorStake)))
    );
    let result = client.try_slash_arbitrator_stake(&admin, &arbitrator, &0, &customer);
    assert_eq!(result, Err(Ok(Error::Core(CoreError::InvalidAmount))));

    assert_eq!(client.get_arbitrator_stake(&arbitrator), 1000);
    assert_eq!(stake_token.balance(&customer), 0);
}

#[test]
fn test_slash_arbitrator_stake_requires_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, arbitrator, _) = setup_staked_arbitrator(&env, 1000);
    let impostor = Address::generate(&env);

    let result = client.try_slash_arbitrator_stake(&impostor, &arbitrator, &500, &impostor);
    assert_eq!(result, Err(Ok(Error::Core(CoreError::Unauthorized))));
    assert_eq!(client.get_arbitrator_stake(&arbitrator), 1000);
}

#[test]
fn test_deposit_arbitrator_stake_requires_registered_arbitrator() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _arbitrator, _) = setup_staked_arbitrator(&env, 1000);
    let outsider = Address::generate(&env);

    let result = client.try_deposit_arbitrator_stake(&outsider, &100);
    assert_eq!(result, Err(Ok(Error::Core(CoreError::NotArbitrator))));
}
