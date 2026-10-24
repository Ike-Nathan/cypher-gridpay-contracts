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

fn setup_test_env() -> (
    Env,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
    token::Client<'static>,
) {
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
    token_admin.mint(&merchant, &10000);
    token_admin.mint(&customer, &10000);

    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    // Register arbitrators
    client.register_arbitrator(&admin, &arbitrator1);
    client.register_arbitrator(&admin, &arbitrator2);
    client.register_arbitrator(&admin, &arbitrator3);

    (
        env,
        admin,
        merchant,
        customer,
        arbitrator1,
        arbitrator2,
        arbitrator3,
        treasury,
        token_client,
    )
}

#[test]
fn test_set_arbitration_fee_config_valid() {
    let (env, admin, _, _, _, _, _, treasury, token_client) = setup_test_env();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let config = ArbitrationFeeConfig {
        arbitrator_share_bps: 7000, // 70%
        treasury_share_bps: 3000,   // 30%
        treasury_address: treasury.clone(),
        fee_token: token_client.address.clone(),
        fee_per_case: 1000,
    };

    client.set_arbitration_fee_config(&admin, &config);

    let retrieved_config = client.get_arbitration_fee_config();
    assert!(retrieved_config.is_some());
    let retrieved = retrieved_config.unwrap();
    assert_eq!(retrieved.arbitrator_share_bps, 7000);
    assert_eq!(retrieved.treasury_share_bps, 3000);
    assert_eq!(retrieved.treasury_address, treasury);
}

#[test]
#[should_panic(expected = "Error(Contract, #30)")]
fn test_set_arbitration_fee_config_invalid_sum() {
    let (env, admin, _, _, _, _, _, treasury, token_client) = setup_test_env();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let config = ArbitrationFeeConfig {
        arbitrator_share_bps: 7000,
        treasury_share_bps: 2000, // Sum is 9000, not 10000
        treasury_address: treasury.clone(),
        fee_token: token_client.address.clone(),
        fee_per_case: 1000,
    };

    client.set_arbitration_fee_config(&admin, &config);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_set_arbitration_fee_config_unauthorized() {
    let (env, admin, _, customer, _, _, _, treasury, token_client) = setup_test_env();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let config = ArbitrationFeeConfig {
        arbitrator_share_bps: 7000,
        treasury_share_bps: 3000,
        treasury_address: treasury.clone(),
        fee_token: token_client.address.clone(),
        fee_per_case: 1000,
    };

    // Try to set config with non-admin address
    client.set_arbitration_fee_config(&customer, &config);
}

#[test]
fn test_fee_distribution_equal_split() {
    let (
        env,
        admin,
        merchant,
        customer,
        arbitrator1,
        arbitrator2,
        arbitrator3,
        treasury,
        token_client,
    ) = setup_test_env();

    // Set fee configuration: 50/50 split
    let config = ArbitrationFeeConfig {
        arbitrator_share_bps: 5000, // 50%
        treasury_share_bps: 5000,   // 50%
        treasury_address: treasury.clone(),
        fee_token: token_client.address.clone(),
        fee_per_case: 1000,
    };

    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    // Register arbitrators
    client.register_arbitrator(&admin, &arbitrator1);
    client.register_arbitrator(&admin, &arbitrator2);
    client.register_arbitrator(&admin, &arbitrator3);

    client.set_arbitration_fee_config(&admin, &config);

    // Create a refund request
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

    // Reject it first so we can escalate
    client.reject_refund(
        &admin,
        &refund_id,
        &String::from_str(&env, "Rejected for testing"),
    );

    // Escalate to arbitration with fee pool of 1000
    let fee_pool = 1000i128;
    token_client.transfer(&merchant, &contract_id, &fee_pool);

    let case_id =
        client.escalate_to_arbitration(&merchant, &refund_id, &token_client.address, &fee_pool);

    // Cast votes - all three arbitrators vote for refund (majority)
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    client.cast_arbitration_vote(&arbitrator1, &case_id, &true, &hash);
    client.cast_arbitration_vote(&arbitrator2, &case_id, &true, &hash);
    client.cast_arbitration_vote(&arbitrator3, &case_id, &true, &hash);

    // Get initial balances
    let arb1_initial = token_client.balance(&arbitrator1);
    let arb2_initial = token_client.balance(&arbitrator2);
    let arb3_initial = token_client.balance(&arbitrator3);
    let treasury_initial = token_client.balance(&treasury);

    // Close the case
    client.close_arbitration_case(&case_id);

    // Check balances after distribution
    let arb1_final = token_client.balance(&arbitrator1);
    let arb2_final = token_client.balance(&arbitrator2);
    let arb3_final = token_client.balance(&arbitrator3);
    let treasury_final = token_client.balance(&treasury);

    // Each arbitrator should get 500 / 3 = 166 (with rounding)
    let expected_per_arbitrator = 500 / 3;
    assert_eq!(arb1_final - arb1_initial, expected_per_arbitrator);
    assert_eq!(arb2_final - arb2_initial, expected_per_arbitrator);
    assert_eq!(arb3_final - arb3_initial, expected_per_arbitrator);

    // Treasury should get 500
    assert_eq!(treasury_final - treasury_initial, 500);

    // Check accumulated fees
    assert_eq!(client.get_accumulated_arbitration_fees(), 500);
}

#[test]
fn test_fee_distribution_majority_only() {
    let (
        env,
        admin,
        merchant,
        customer,
        arbitrator1,
        arbitrator2,
        arbitrator3,
        treasury,
        token_client,
    ) = setup_test_env();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    // Register arbitrators
    client.register_arbitrator(&admin, &arbitrator1);
    client.register_arbitrator(&admin, &arbitrator2);
    client.register_arbitrator(&admin, &arbitrator3);

    // Set fee configuration: 80/20 split
    let config = ArbitrationFeeConfig {
        arbitrator_share_bps: 8000, // 80%
        treasury_share_bps: 2000,   // 20%
        treasury_address: treasury.clone(),
        fee_token: token_client.address.clone(),
        fee_per_case: 1000,
    };
    client.set_arbitration_fee_config(&admin, &config);

    // Create a refund request
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

    // Reject it first so we can escalate
    client.reject_refund(
        &admin,
        &refund_id,
        &String::from_str(&env, "Rejected for testing"),
    );

    // Escalate to arbitration
    let fee_pool = 1000i128;
    token_client.transfer(&merchant, &contract_id, &fee_pool);

    let case_id =
        client.escalate_to_arbitration(&merchant, &refund_id, &token_client.address, &fee_pool);

    // Cast votes - 2 for refund (majority), 1 against (minority)
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    client.cast_arbitration_vote(&arbitrator1, &case_id, &true, &hash);
    client.cast_arbitration_vote(&arbitrator2, &case_id, &true, &hash);
    client.cast_arbitration_vote(&arbitrator3, &case_id, &false, &hash); // minority

    // Get initial balances
    let arb1_initial = token_client.balance(&arbitrator1);
    let arb2_initial = token_client.balance(&arbitrator2);
    let arb3_initial = token_client.balance(&arbitrator3);
    let treasury_initial = token_client.balance(&treasury);

    // Close the case
    client.close_arbitration_case(&case_id);

    // Check balances after distribution
    let arb1_final = token_client.balance(&arbitrator1);
    let arb2_final = token_client.balance(&arbitrator2);
    let arb3_final = token_client.balance(&arbitrator3);
    let treasury_final = token_client.balance(&treasury);

    // Only majority voters (arb1 and arb2) should get fees
    // 800 / 2 = 400 each
    assert_eq!(arb1_final - arb1_initial, 400);
    assert_eq!(arb2_final - arb2_initial, 400);

    // Minority voter (arb3) should get nothing
    assert_eq!(arb3_final - arb3_initial, 0);

    // Treasury should get 200
    assert_eq!(treasury_final - treasury_initial, 200);
}

#[test]
fn test_withdraw_treasury_fees() {
    let (
        env,
        admin,
        merchant,
        customer,
        arbitrator1,
        arbitrator2,
        arbitrator3,
        treasury,
        token_client,
    ) = setup_test_env();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    // Register arbitrators
    client.register_arbitrator(&admin, &arbitrator1);
    client.register_arbitrator(&admin, &arbitrator2);
    client.register_arbitrator(&admin, &arbitrator3);

    // Set fee configuration
    let config = ArbitrationFeeConfig {
        arbitrator_share_bps: 7000, // 70%
        treasury_share_bps: 3000,   // 30%
        treasury_address: treasury.clone(),
        fee_token: token_client.address.clone(),
        fee_per_case: 1000,
    };
    client.set_arbitration_fee_config(&admin, &config);

    // Create and close an arbitration case
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

    // Reject it first so we can escalate
    client.reject_refund(
        &admin,
        &refund_id,
        &String::from_str(&env, "Rejected for testing"),
    );

    let fee_pool = 1000i128;
    token_client.transfer(&merchant, &contract_id, &fee_pool);

    let case_id =
        client.escalate_to_arbitration(&merchant, &refund_id, &token_client.address, &fee_pool);

    let hash = BytesN::from_array(&env, &[0u8; 32]);
    client.cast_arbitration_vote(&arbitrator1, &case_id, &true, &hash);
    client.cast_arbitration_vote(&arbitrator2, &case_id, &true, &hash);
    client.cast_arbitration_vote(&arbitrator3, &case_id, &true, &hash);

    client.close_arbitration_case(&case_id);

    // Check accumulated fees (should be 300)
    let accumulated = client.get_accumulated_arbitration_fees();
    assert_eq!(accumulated, 300);

    // Withdraw fees
    let withdrawn = client.withdraw_treasury_fees(&admin);
    assert_eq!(withdrawn, 300);

    // Check that accumulated fees are now 0
    assert_eq!(client.get_accumulated_arbitration_fees(), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #31)")]
fn test_withdraw_treasury_fees_insufficient() {
    let (env, admin, _, _, _, _, _, _, _) = setup_test_env();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    // Try to withdraw when there are no accumulated fees
    client.withdraw_treasury_fees(&admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_withdraw_treasury_fees_unauthorized() {
    let (env, admin, _, customer, _, _, _, _, _) = setup_test_env();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    // Try to withdraw with non-admin address
    client.withdraw_treasury_fees(&customer);
}

#[test]
fn test_fee_distribution_without_config() {
    let (env, admin, merchant, customer, arbitrator1, arbitrator2, arbitrator3, _, token_client) =
        setup_test_env();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    // Register arbitrators
    client.register_arbitrator(&admin, &arbitrator1);
    client.register_arbitrator(&admin, &arbitrator2);
    client.register_arbitrator(&admin, &arbitrator3);

    // Don't set any fee configuration - should default to 100% arbitrators

    // Create a refund request
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

    // Reject it first so we can escalate
    client.reject_refund(
        &admin,
        &refund_id,
        &String::from_str(&env, "Rejected for testing"),
    );

    // Escalate to arbitration
    let fee_pool = 1000i128;
    token_client.transfer(&merchant, &contract_id, &fee_pool);

    let case_id =
        client.escalate_to_arbitration(&merchant, &refund_id, &token_client.address, &fee_pool);

    // Cast votes
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    client.cast_arbitration_vote(&arbitrator1, &case_id, &true, &hash);
    client.cast_arbitration_vote(&arbitrator2, &case_id, &true, &hash);
    client.cast_arbitration_vote(&arbitrator3, &case_id, &true, &hash);

    // Get initial balances
    let arb1_initial = token_client.balance(&arbitrator1);
    let arb2_initial = token_client.balance(&arbitrator2);
    let arb3_initial = token_client.balance(&arbitrator3);

    // Close the case
    client.close_arbitration_case(&case_id);

    // Check balances - all fees should go to arbitrators
    let arb1_final = token_client.balance(&arbitrator1);
    let arb2_final = token_client.balance(&arbitrator2);
    let arb3_final = token_client.balance(&arbitrator3);

    let expected_per_arbitrator = 1000 / 3;
    assert_eq!(arb1_final - arb1_initial, expected_per_arbitrator);
    assert_eq!(arb2_final - arb2_initial, expected_per_arbitrator);
    assert_eq!(arb3_final - arb3_initial, expected_per_arbitrator);

    // No treasury fees should be accumulated
    assert_eq!(client.get_accumulated_arbitration_fees(), 0);
}

#[test]
fn test_fee_distribution_100_percent_treasury() {
    let (
        env,
        admin,
        merchant,
        customer,
        arbitrator1,
        arbitrator2,
        arbitrator3,
        treasury,
        token_client,
    ) = setup_test_env();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    // Register arbitrators
    client.register_arbitrator(&admin, &arbitrator1);
    client.register_arbitrator(&admin, &arbitrator2);
    client.register_arbitrator(&admin, &arbitrator3);

    // Set fee configuration: 0% arbitrators, 100% treasury
    let config = ArbitrationFeeConfig {
        arbitrator_share_bps: 0,   // 0%
        treasury_share_bps: 10000, // 100%
        treasury_address: treasury.clone(),
        fee_token: token_client.address.clone(),
        fee_per_case: 1000,
    };
    client.set_arbitration_fee_config(&admin, &config);

    // Create a refund request
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

    // Reject it first so we can escalate
    client.reject_refund(
        &admin,
        &refund_id,
        &String::from_str(&env, "Rejected for testing"),
    );

    // Escalate to arbitration
    let fee_pool = 1000i128;
    token_client.transfer(&merchant, &contract_id, &fee_pool);

    let case_id =
        client.escalate_to_arbitration(&merchant, &refund_id, &token_client.address, &fee_pool);

    // Cast votes
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    client.cast_arbitration_vote(&arbitrator1, &case_id, &true, &hash);
    client.cast_arbitration_vote(&arbitrator2, &case_id, &true, &hash);
    client.cast_arbitration_vote(&arbitrator3, &case_id, &true, &hash);

    // Get initial balances
    let arb1_initial = token_client.balance(&arbitrator1);
    let arb2_initial = token_client.balance(&arbitrator2);
    let arb3_initial = token_client.balance(&arbitrator3);
    let treasury_initial = token_client.balance(&treasury);

    // Close the case
    client.close_arbitration_case(&case_id);

    // Check balances - arbitrators should get nothing
    let arb1_final = token_client.balance(&arbitrator1);
    let arb2_final = token_client.balance(&arbitrator2);
    let arb3_final = token_client.balance(&arbitrator3);
    let treasury_final = token_client.balance(&treasury);

    assert_eq!(arb1_final - arb1_initial, 0);
    assert_eq!(arb2_final - arb2_initial, 0);
    assert_eq!(arb3_final - arb3_initial, 0);

    // Treasury should get all 1000
    assert_eq!(treasury_final - treasury_initial, 1000);
    assert_eq!(client.get_accumulated_arbitration_fees(), 1000);
}

/// Runs a refund of `amount` through rejection and a unanimous arbitration
/// ruling in the customer's favour. Returns (client, refund_id).
fn arbitrate_refund_in_customer_favour<'a>(
    env: &'a Env,
    admin: &Address,
    merchant: &Address,
    customer: &Address,
    arbitrators: [&Address; 3],
    token_client: &token::Client,
    amount: i128,
) -> (RefundContractClient<'a>, u64) {
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(env, &contract_id);
    client.initialize(admin);
    for arbitrator in arbitrators {
        client.register_arbitrator(admin, arbitrator);
    }
    // Fund the contract so the award can be paid out.
    token_client.transfer(merchant, &contract_id, &5_000);

    let refund_id = client.request_refund(
        merchant,
        &1u64,
        customer,
        &amount,
        &10_000i128,
        &token_client.address,
        &String::from_str(env, "Test refund"),
        &RefundReasonCode::Other,
        &1000u64,
    );
    client.reject_refund(admin, &refund_id, &String::from_str(env, "Rejected"));
    let case_id = client.escalate_to_arbitration(customer, &refund_id, &token_client.address, &300);

    let hash = BytesN::from_array(env, &[0u8; 32]);
    for arbitrator in arbitrators {
        client.cast_arbitration_vote(arbitrator, &case_id, &true, &hash);
    }
    client.close_arbitration_case(&case_id);
    assert_eq!(client.get_refund(&refund_id).status, RefundStatus::Approved);
    (client, refund_id)
}

#[test]
fn test_protocol_fee_deducted_from_arbitration_award() {
    let (env, admin, merchant, customer, arb1, arb2, arb3, treasury, token_client) =
        setup_test_env();
    let (client, refund_id) = arbitrate_refund_in_customer_favour(
        &env,
        &admin,
        &merchant,
        &customer,
        [&arb1, &arb2, &arb3],
        &token_client,
        1_000,
    );
    assert_eq!(client.get_arbitration_protocol_fee(), 200); // default 2%

    let customer_before = token_client.balance(&customer);
    let contract_before = token_client.balance(&client.address);
    client.process_refund(&admin, &refund_id);

    // Winning customer receives the award net of the 2% protocol fee...
    assert_eq!(token_client.balance(&customer) - customer_before, 980);
    // ...and the fee is credited to ConfigKey::AccumulatedFees and kept in
    // the contract.
    assert_eq!(
        client.get_accumulated_protocol_fees(&token_client.address),
        20
    );
    assert_eq!(contract_before - token_client.balance(&client.address), 980);

    // The fee can be withdrawn by the admin.
    assert_eq!(
        client.withdraw_protocol_fees(&admin, &token_client.address, &treasury),
        20
    );
    assert_eq!(token_client.balance(&treasury), 20);
    assert_eq!(
        client.get_accumulated_protocol_fees(&token_client.address),
        0
    );
}

#[test]
fn test_protocol_fee_is_configurable() {
    let (env, admin, merchant, customer, arb1, arb2, arb3, _, token_client) = setup_test_env();
    let (client, refund_id) = arbitrate_refund_in_customer_favour(
        &env,
        &admin,
        &merchant,
        &customer,
        [&arb1, &arb2, &arb3],
        &token_client,
        1_000,
    );
    client.set_arbitration_protocol_fee(&admin, &500); // 5%

    let customer_before = token_client.balance(&customer);
    client.process_refund(&admin, &refund_id);
    assert_eq!(token_client.balance(&customer) - customer_before, 950);
    assert_eq!(
        client.get_accumulated_protocol_fees(&token_client.address),
        50
    );

    assert_eq!(
        client.try_set_arbitration_protocol_fee(&admin, &(MAX_ARBITRATION_PROTOCOL_FEE_BPS + 1)),
        Err(Ok(Error::Core(CoreError::InvalidFeeConfig)))
    );
    assert_eq!(
        client.try_set_arbitration_protocol_fee(&customer, &100),
        Err(Ok(Error::Core(CoreError::Unauthorized)))
    );
}

#[test]
fn test_protocol_fee_not_charged_on_non_arbitrated_refund() {
    let (env, admin, merchant, customer, _, _, _, _, token_client) = setup_test_env();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);
    token_client.transfer(&merchant, &contract_id, &5_000);

    let refund_id = client.request_refund(
        &merchant,
        &1u64,
        &customer,
        &1_000i128,
        &10_000i128,
        &token_client.address,
        &String::from_str(&env, "Direct refund"),
        &RefundReasonCode::Other,
        &1000u64,
    );
    client.approve_refund(&admin, &refund_id);
    let customer_before = token_client.balance(&customer);
    client.process_refund(&admin, &refund_id);

    assert_eq!(token_client.balance(&customer) - customer_before, 1_000);
    assert_eq!(
        client.get_accumulated_protocol_fees(&token_client.address),
        0
    );
}
