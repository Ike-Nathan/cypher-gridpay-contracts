#![cfg(test)]

use super::*;
use soroban_sdk::{contract, contractimpl, testutils::Address as _, token, Address, Env};

#[contract]
pub struct MockSwapOracle;

#[contractimpl]
impl MockSwapOracle {
    pub fn get_rate(env: Env) -> i128 {
        env.storage()
            .instance()
            .get::<u32, i128>(&0u32)
            .unwrap_or(10_000_000) // Default 1.0 rate (1e7)
    }

    pub fn set_rate(env: Env, rate: i128) {
        env.storage().instance().set(&0u32, &rate);
    }

    pub fn get_updated_at(env: Env) -> u64 {
        env.storage()
            .instance()
            .get::<u32, u64>(&1u32)
            .unwrap_or_else(|| env.ledger().timestamp())
    }

    pub fn set_updated_at(env: Env, timestamp: u64) {
        env.storage().instance().set(&1u32, &timestamp);
    }
}

#[test]
fn test_escrow_swap_successful() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = env.register_stellar_asset_contract(admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token);
    token_admin_client.mint(&customer, &1_000_000);
    let target_token = Address::generate(&env);
    let oracle = env.register(MockSwapOracle, ());
    let oracle_client = MockSwapOracleClient::new(&env, &oracle);

    client.initialize(&admin);

    // Create escrow (8 arguments as defined in lib.rs)
    let escrow_id = client.create_escrow(
        &customer, &merchant, &1000_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );

    // Set rate to 1.5 (15_000_000)
    oracle_client.set_rate(&15_000_000_i128);

    // Configure swap by merchant (min_output = 1400)
    client.configure_escrow_swap(&merchant, &escrow_id, &target_token, &1400_i128, &oracle);

    // Verify config is retrieved correctly
    let config = client.get_swap_config(&escrow_id).unwrap();
    assert_eq!(config.escrow_id, escrow_id);
    assert_eq!(config.source_token, token);
    assert_eq!(config.target_token, target_token);
    assert_eq!(config.min_output_amount, 1400_i128);
    assert_eq!(config.oracle, oracle);
    assert_eq!(config.executed, false);

    // Execute swap
    let output = client.execute_escrow_swap(&merchant, &escrow_id);
    assert_eq!(output, 1500_i128); // 1000 * 15_000_000 / 10_000_000 = 1500

    // Verify updated escrow
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.token, target_token);
    assert_eq!(escrow.amount, 1500_i128);

    // Verify config executed
    let config = client.get_swap_config(&escrow_id).unwrap();
    assert_eq!(config.executed, true);
}

#[test]
fn test_escrow_swap_successful_by_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = env.register_stellar_asset_contract(admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token);
    token_admin_client.mint(&customer, &1_000_000);
    let target_token = Address::generate(&env);
    let oracle = env.register(MockSwapOracle, ());
    let oracle_client = MockSwapOracleClient::new(&env, &oracle);

    client.initialize(&admin);

    let escrow_id = client.create_escrow(
        &customer, &merchant, &1000_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );

    oracle_client.set_rate(&15_000_000_i128);

    // Configure swap by admin
    client.configure_escrow_swap(&admin, &escrow_id, &target_token, &1400_i128, &oracle);

    // Execute swap by admin
    let output = client.execute_escrow_swap(&admin, &escrow_id);
    assert_eq!(output, 1500_i128);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.token, target_token);
    assert_eq!(escrow.amount, 1500_i128);
}

#[test]
fn test_escrow_swap_below_minimum_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = env.register_stellar_asset_contract(admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token);
    token_admin_client.mint(&customer, &1_000_000);
    let target_token = Address::generate(&env);
    let oracle = env.register(MockSwapOracle, ());
    let oracle_client = MockSwapOracleClient::new(&env, &oracle);

    client.initialize(&admin);

    let escrow_id = client.create_escrow(
        &customer, &merchant, &1000_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );

    // Set rate to 1.2 (12_000_000) -> output = 1200
    oracle_client.set_rate(&12_000_000_i128);

    // Configure swap (min_output = 1300)
    client.configure_escrow_swap(&merchant, &escrow_id, &target_token, &1300_i128, &oracle);

    // Executing should fail with SwapOutputBelowMinimum
    let res = client.try_execute_escrow_swap(&merchant, &escrow_id);
    assert_eq!(
        res,
        Err(Ok(Error::Action(ActionError::SwapOutputBelowMinimum)))
    );
}

#[test]
fn test_escrow_swap_double_execution_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = env.register_stellar_asset_contract(admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token);
    token_admin_client.mint(&customer, &1_000_000);
    let target_token = Address::generate(&env);
    let oracle = env.register(MockSwapOracle, ());
    let oracle_client = MockSwapOracleClient::new(&env, &oracle);

    client.initialize(&admin);

    let escrow_id = client.create_escrow(
        &customer, &merchant, &1000_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );

    oracle_client.set_rate(&15_000_000_i128);

    client.configure_escrow_swap(&merchant, &escrow_id, &target_token, &1400_i128, &oracle);

    // Execute first time: success
    let output = client.execute_escrow_swap(&merchant, &escrow_id);
    assert_eq!(output, 1500_i128);

    // Execute second time: fails with SwapAlreadyExecuted
    let res = client.try_execute_escrow_swap(&merchant, &escrow_id);
    assert_eq!(
        res,
        Err(Ok(Error::Action(ActionError::SwapAlreadyExecuted)))
    );
}

#[test]
fn test_escrow_swap_unauthorized_config_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let unauthorized_caller = Address::generate(&env);
    let token = env.register_stellar_asset_contract(admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token);
    token_admin_client.mint(&customer, &1_000_000);
    let target_token = Address::generate(&env);
    let oracle = env.register(MockSwapOracle, ());

    client.initialize(&admin);

    let escrow_id = client.create_escrow(
        &customer, &merchant, &1000_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );

    // Non-merchant, non-admin tries to configure swap: fails with Unauthorized
    let res = client.try_configure_escrow_swap(
        &unauthorized_caller,
        &escrow_id,
        &target_token,
        &1400_i128,
        &oracle,
    );
    assert_eq!(res, Err(Ok(Error::Basic(BasicError::Unauthorized))));
}

#[test]
fn test_escrow_swap_unauthorized_execute_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let unauthorized_caller = Address::generate(&env);
    let token = env.register_stellar_asset_contract(admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token);
    token_admin_client.mint(&customer, &1_000_000);
    let target_token = Address::generate(&env);
    let oracle = env.register(MockSwapOracle, ());

    client.initialize(&admin);

    let escrow_id = client.create_escrow(
        &customer, &merchant, &1000_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );

    client.configure_escrow_swap(&merchant, &escrow_id, &target_token, &1400_i128, &oracle);

    // Non-merchant, non-admin tries to execute swap: fails with Unauthorized
    let res = client.try_execute_escrow_swap(&unauthorized_caller, &escrow_id);
    assert_eq!(res, Err(Ok(Error::Basic(BasicError::Unauthorized))));
}

#[test]
fn test_escrow_swap_config_not_found_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = env.register_stellar_asset_contract(admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token);
    token_admin_client.mint(&customer, &1_000_000);

    client.initialize(&admin);

    let escrow_id = client.create_escrow(
        &customer, &merchant, &1000_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );

    // Execute swap directly without configuring first: fails with SwapConfigNotFound
    let res = client.try_execute_escrow_swap(&merchant, &escrow_id);
    assert_eq!(res, Err(Ok(Error::Action(ActionError::SwapConfigNotFound))));
}

#[test]
fn swap_rolls_back_when_second_leg_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = env.register_stellar_asset_contract(admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token);
    token_admin_client.mint(&customer, &1_000_000);
    let target_token = Address::generate(&env);
    let oracle = env.register(MockSwapOracle, ());
    let oracle_client = MockSwapOracleClient::new(&env, &oracle);

    client.initialize(&admin);

    let escrow_id = client.create_escrow(
        &customer, &merchant, &1000_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );

    // Set rate to 1.2 (12_000_000) -> output = 1200
    oracle_client.set_rate(&12_000_000_i128);

    // Configure swap (min_output = 1300)
    client.configure_escrow_swap(&merchant, &escrow_id, &target_token, &1300_i128, &oracle);

    // Get initial escrow state
    let escrow_before = client.get_escrow(&escrow_id);
    assert_eq!(escrow_before.token, token);
    assert_eq!(escrow_before.amount, 1000_i128);

    // Executing should fail with SwapOutputBelowMinimum
    let res = client.try_execute_escrow_swap(&merchant, &escrow_id);
    assert_eq!(
        res,
        Err(Ok(Error::Action(ActionError::SwapOutputBelowMinimum)))
    );

    // Verify escrow state is unchanged (rollback)
    let escrow_after = client.get_escrow(&escrow_id);
    assert_eq!(escrow_after.token, token);
    assert_eq!(escrow_after.amount, 1000_i128);

    // Verify swap config is not marked as executed
    let config = client.get_swap_config(&escrow_id).unwrap();
    assert_eq!(config.executed, false);
}

#[test]
fn both_legs_succeed_funds_exchanged_correctly() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = env.register_stellar_asset_contract(admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token);
    token_admin_client.mint(&customer, &1_000_000);
    let target_token = Address::generate(&env);
    let oracle = env.register(MockSwapOracle, ());
    let oracle_client = MockSwapOracleClient::new(&env, &oracle);

    client.initialize(&admin);

    let escrow_id = client.create_escrow(
        &customer, &merchant, &1000_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );

    // Set rate to 1.5 (15_000_000)
    oracle_client.set_rate(&15_000_000_i128);

    // Configure swap by merchant (min_output = 1400)
    client.configure_escrow_swap(&merchant, &escrow_id, &target_token, &1400_i128, &oracle);

    // Execute swap
    let output = client.execute_escrow_swap(&merchant, &escrow_id);
    assert_eq!(output, 1500_i128); // 1000 * 15_000_000 / 10_000_000 = 1500

    // Verify updated escrow - funds exchanged correctly
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.token, target_token);
    assert_eq!(escrow.amount, 1500_i128);

    // Verify config executed
    let config = client.get_swap_config(&escrow_id).unwrap();
    assert_eq!(config.executed, true);
}

#[test]
fn swap_cannot_be_claimed_by_non_participant() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let non_participant = Address::generate(&env);
    let token = env.register_stellar_asset_contract(admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token);
    token_admin_client.mint(&customer, &1_000_000);
    let target_token = Address::generate(&env);
    let oracle = env.register(MockSwapOracle, ());

    client.initialize(&admin);

    let escrow_id = client.create_escrow(
        &customer, &merchant, &1000_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );

    client.configure_escrow_swap(&merchant, &escrow_id, &target_token, &1400_i128, &oracle);

    // Non-participant tries to execute swap: fails with Unauthorized
    let res = client.try_execute_escrow_swap(&non_participant, &escrow_id);
    assert_eq!(res, Err(Ok(Error::Basic(BasicError::Unauthorized))));
}

#[test]
fn test_escrow_swap_rejects_stale_oracle_price() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 1_000_000);
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = env.register_stellar_asset_contract(admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token);
    token_admin_client.mint(&customer, &1_000_000);
    let target_token = Address::generate(&env);
    let oracle = env.register(MockSwapOracle, ());
    let oracle_client = MockSwapOracleClient::new(&env, &oracle);

    client.initialize(&admin);

    let escrow_id = client.create_escrow(
        &customer, &merchant, &1000_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );

    oracle_client.set_rate(&15_000_000_i128);
    // Oracle price was last updated more than 300 seconds ago.
    oracle_client.set_updated_at(&(1_000_000u64 - 301));

    client.configure_escrow_swap(&merchant, &escrow_id, &target_token, &1400_i128, &oracle);

    let res = client.try_execute_escrow_swap(&merchant, &escrow_id);
    assert_eq!(
        res,
        Err(Ok(Error::Action(ActionError::StaleOraclePrice)))
    );
}

/// MEV sandwich-attack protection: configure_escrow_swap now locks the oracle
/// rate at configuration time. If the rate moves adversely between configure
/// and execute (simulating a sandwich), the locked rate is used for the
/// slippage check so the attacker cannot widen the effective slippage window.
///
/// Scenario:
///   1. Merchant configures swap when rate = 1.5 → locked_rate = 15_000_000,
///      min_output = 1400 (6.7% tolerance at 1.5 rate).
///   2. Attacker front-runs and moves the oracle rate to 1.41, which is still
///      above min_output (1410 > 1400) and would pass if the *current* oracle
///      rate were used — a classic sandwich.
///   3. execute_escrow_swap uses the locked rate (15_000_000), so the computed
///      output is 1500, and the trade succeeds at the expected price rather
///      than the manipulated one.
#[test]
fn test_mev_sandwich_protection_via_rate_lock() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 1_000_000);

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = env.register_stellar_asset_contract(admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token);
    token_admin_client.mint(&customer, &1_000_000);
    let target_token = Address::generate(&env);
    let oracle = env.register(MockSwapOracle, ());
    let oracle_client = MockSwapOracleClient::new(&env, &oracle);

    client.initialize(&admin);

    let escrow_id = client.create_escrow(
        &customer, &merchant, &1000_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );

    // Step 1: set a fresh oracle price of 1.5 and configure the swap.
    oracle_client.set_rate(&15_000_000_i128);
    oracle_client.set_updated_at(&999_999_u64); // 1 second old — fresh
    client.configure_escrow_swap(&merchant, &escrow_id, &target_token, &1400_i128, &oracle);

    // Verify that the locked_rate captured the rate at configure time.
    let config = client.get_swap_config(&escrow_id).unwrap();
    assert_eq!(config.locked_rate, 15_000_000_i128,
        "configure_escrow_swap must lock the oracle rate at call time");

    // Step 2: simulate a sandwich — attacker moves oracle rate to 1.41 (just
    // above the 1.4 × 1000 = 1400 min_output threshold) between configure and
    // execute. The oracle timestamp is still recent so staleness does not fire.
    oracle_client.set_rate(&14_100_000_i128);
    oracle_client.set_updated_at(&999_999_u64);

    // Step 3: execute must use the locked rate (1.5), not the manipulated rate.
    let output = client.execute_escrow_swap(&merchant, &escrow_id);
    assert_eq!(output, 1500_i128,
        "execute_escrow_swap must use the locked rate, not the sandwiched oracle rate");

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.token, target_token);
    assert_eq!(escrow.amount, 1500_i128);
}

/// Ensures that if the oracle rate has dropped BELOW min_output even at the
/// locked rate, the transaction is correctly rejected (not silently passed).
#[test]
fn test_mev_rate_lock_still_enforces_min_output() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 1_000_000);

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = env.register_stellar_asset_contract(admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token);
    token_admin_client.mint(&customer, &1_000_000);
    let target_token = Address::generate(&env);
    let oracle = env.register(MockSwapOracle, ());
    let oracle_client = MockSwapOracleClient::new(&env, &oracle);

    client.initialize(&admin);

    let escrow_id = client.create_escrow(
        &customer, &merchant, &1000_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );

    // Configure when rate = 1.2 but min_output = 1300 (higher than 1200).
    oracle_client.set_rate(&12_000_000_i128);
    oracle_client.set_updated_at(&999_999_u64);
    client.configure_escrow_swap(&merchant, &escrow_id, &target_token, &1300_i128, &oracle);

    let config = client.get_swap_config(&escrow_id).unwrap();
    assert_eq!(config.locked_rate, 12_000_000_i128);

    // Even if the oracle rate improves after configure, the locked rate is 1.2
    // → output = 1200 < min_output = 1300 → must fail.
    oracle_client.set_rate(&15_000_000_i128);
    oracle_client.set_updated_at(&999_999_u64);

    let res = client.try_execute_escrow_swap(&merchant, &escrow_id);
    assert_eq!(res, Err(Ok(Error::Action(ActionError::SwapOutputBelowMinimum))));
}
