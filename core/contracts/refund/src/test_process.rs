#![cfg(test)]
use super::{
    NetRefund, RefundContract, RefundContractClient, RefundReasonCode, RefundStatus,
    DEFAULT_NETWORK_FEE_SHARE_BPS,
};
use soroban_sdk::{testutils::Address as _, token, Address, Env, String};

#[test]
fn test_initialize() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);
}

#[test]
#[should_panic(expected = "Already initialized")]
fn test_initialize_twice_should_fail() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);
    client.initialize(&admin);
}

#[test]
fn test_approve_refund() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = Address::generate(&env);
    let payment_id = 1u64;
    let amount = 1000i128;
    let reason = String::from_str(&env, "reason");

    env.mock_all_auths();
    let refund_id = client.request_refund(
        &merchant,
        &payment_id,
        &customer,
        &amount,
        &1000,
        &token,
        &reason,
        &RefundReasonCode::Other,
        &0_u64,
    );

    // Approve
    client.approve_refund(&admin, &refund_id);

    let refund = client.get_refund(&refund_id);
    assert_eq!(refund.status, RefundStatus::Approved);
}

// ── Issue #71: net refund fee deduction math ────────────────────────────────

/// Invariant asserted by every calculation test: the breakdown must reconcile
/// exactly, with no unaccounted rounding remainder.
fn assert_reconciles(net: &NetRefund) {
    assert_eq!(
        net.net_amount + net.total_fee,
        net.gross_amount,
        "net + total_fee must equal gross"
    );
    assert_eq!(
        net.processing_fee + net.network_fee,
        net.total_fee,
        "processing + network must equal total_fee"
    );
    assert!(net.total_fee >= 0, "fee must never be negative");
    assert!(net.net_amount >= 0, "net payout must never be negative");
    assert!(net.net_amount <= net.gross_amount);
}

#[test]
fn test_calculate_net_refund_zero_fee() {
    let env = Env::default();
    let client = RefundContractClient::new(&env, &env.register(RefundContract, ()));

    let net = client.calculate_net_refund(&10_000i128, &0u32);
    assert_eq!(net.gross_amount, 10_000);
    assert_eq!(net.total_fee, 0);
    assert_eq!(net.processing_fee, 0);
    assert_eq!(net.network_fee, 0);
    assert_eq!(net.net_amount, 10_000);
    assert_reconciles(&net);
}

#[test]
fn test_calculate_net_refund_one_percent() {
    let env = Env::default();
    let client = RefundContractClient::new(&env, &env.register(RefundContract, ()));

    // 100 bps == 1%
    let net = client.calculate_net_refund(&10_000i128, &100u32);
    assert_eq!(net.fee_bps, 100);
    assert_eq!(net.total_fee, 100);
    assert_eq!(net.net_amount, 9_900);
    assert_reconciles(&net);

    // Default split: 25% of the fee is network cost, 75% is processing.
    assert_eq!(net.network_fee_share_bps, DEFAULT_NETWORK_FEE_SHARE_BPS);
    assert_eq!(net.network_fee, 25);
    assert_eq!(net.processing_fee, 75);
}

#[test]
fn test_calculate_net_refund_250_bps() {
    let env = Env::default();
    let client = RefundContractClient::new(&env, &env.register(RefundContract, ()));

    // 250 bps == 2.5% of 40_000 == 1_000
    let net = client.calculate_net_refund(&40_000i128, &250u32);
    assert_eq!(net.total_fee, 1_000);
    assert_eq!(net.net_amount, 39_000);
    assert_eq!(net.network_fee, 250);
    assert_eq!(net.processing_fee, 750);
    assert_reconciles(&net);
}

#[test]
fn test_calculate_net_refund_full_fee() {
    let env = Env::default();
    let client = RefundContractClient::new(&env, &env.register(RefundContract, ()));

    // 10_000 bps == 100%: the customer receives nothing.
    let net = client.calculate_net_refund(&5_000i128, &10_000u32);
    assert_eq!(net.total_fee, 5_000);
    assert_eq!(net.net_amount, 0);
    assert_eq!(net.processing_fee, 3_750);
    assert_eq!(net.network_fee, 1_250);
    assert_reconciles(&net);
}

#[test]
fn test_calculate_net_refund_clamps_oversized_bps() {
    let env = Env::default();
    let client = RefundContractClient::new(&env, &env.register(RefundContract, ()));

    // A misconfigured rate above 10_000 bps is capped rather than producing a
    // negative payout.
    let net = client.calculate_net_refund(&1_000i128, &50_000u32);
    assert_eq!(net.fee_bps, 10_000);
    assert_eq!(net.total_fee, 1_000);
    assert_eq!(net.net_amount, 0);
    assert_reconciles(&net);
}

#[test]
fn test_calculate_net_refund_zero_and_negative_gross() {
    let env = Env::default();
    let client = RefundContractClient::new(&env, &env.register(RefundContract, ()));

    for gross in [0i128, -1i128, -100_000i128] {
        let net = client.calculate_net_refund(&gross, &250u32);
        assert_eq!(net.gross_amount, gross);
        assert_eq!(net.total_fee, 0);
        assert_eq!(net.net_amount, 0);
        assert_eq!(net.processing_fee, 0);
        assert_eq!(net.network_fee, 0);
    }
}

#[test]
fn test_calculate_net_refund_rounding_never_loses_a_unit() {
    let env = Env::default();
    let client = RefundContractClient::new(&env, &env.register(RefundContract, ()));

    // Exercise a wide range of amounts and rates; every combination must
    // reconcile exactly, which is what the remainder-based split buys us.
    for gross in 1i128..200 {
        for bps in [1u32, 7, 33, 100, 250, 999, 2_500, 10_000] {
            let net = client.calculate_net_refund(&gross, &bps);
            assert_reconciles(&net);
            assert_eq!(net.net_amount, gross - net.total_fee);
        }
    }
}

#[test]
fn test_calculate_net_refund_is_monotonic() {
    let env = Env::default();
    let client = RefundContractClient::new(&env, &env.register(RefundContract, ()));

    // A higher fee rate can never pay the customer more.
    let mut previous = client.calculate_net_refund(&100_000i128, &0u32).net_amount;
    for bps in [1u32, 10, 100, 500, 1_000, 5_000, 10_000] {
        let net = client.calculate_net_refund(&100_000i128, &bps);
        assert!(net.net_amount <= previous);
        previous = net.net_amount;
    }
}

#[test]
fn test_calculate_net_refund_tiny_amount_deducts_nothing() {
    let env = Env::default();
    let client = RefundContractClient::new(&env, &env.register(RefundContract, ()));

    // 1 stroop at 100 bps truncates to a zero fee rather than rounding up and
    // producing a negative payout.
    let net = client.calculate_net_refund(&1i128, &100u32);
    assert_eq!(net.total_fee, 0);
    assert_eq!(net.net_amount, 1);
    assert_reconciles(&net);
}

#[test]
fn test_preview_net_refund_without_config_is_gross() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);

    assert!(client.get_refund_fee_config().is_none());
    let net = client.preview_net_refund(&12_345i128);
    assert_eq!(net.total_fee, 0);
    assert_eq!(net.net_amount, 12_345);
    assert_reconciles(&net);
}

#[test]
fn test_preview_net_refund_honours_configured_bps_and_network_share() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);

    client.set_refund_fee_config(
        &admin,
        &super::RefundFeeConfig {
            fee_bps: 300,
            min_fee: 0,
            max_fee: 0,
            treasury: treasury.clone(),
            fee_token: token.clone(),
            active: true,
            network_fee_share_bps: 1_000,
        },
    );

    let net = client.preview_net_refund(&20_000i128);
    // 300 bps of 20_000 == 600 total, 10% of which is the network share.
    assert_eq!(net.total_fee, 600);
    assert_eq!(net.network_fee, 60);
    assert_eq!(net.processing_fee, 540);
    assert_eq!(net.net_amount, 19_400);
    assert_reconciles(&net);
}

#[test]
fn test_preview_net_refund_inactive_config_is_gross() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);

    client.set_refund_fee_config(
        &admin,
        &super::RefundFeeConfig {
            fee_bps: 500,
            min_fee: 0,
            max_fee: 0,
            treasury: Address::generate(&env),
            fee_token: Address::generate(&env),
            active: false,
            network_fee_share_bps: 2_500,
        },
    );

    let net = client.preview_net_refund(&10_000i128);
    assert_eq!(net.total_fee, 0);
    assert_eq!(net.net_amount, 10_000);
    assert_reconciles(&net);
}

#[test]
fn test_preview_net_refund_min_fee_raises_total_and_keeps_split_reconciled() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);

    client.set_refund_fee_config(
        &admin,
        &super::RefundFeeConfig {
            // 1% of 1_000 is only 10, below the 200 minimum.
            fee_bps: 100,
            min_fee: 200,
            max_fee: 0,
            treasury: Address::generate(&env),
            fee_token: Address::generate(&env),
            active: true,
            network_fee_share_bps: 5_000,
        },
    );

    let net = client.preview_net_refund(&1_000i128);
    assert_eq!(net.total_fee, 200);
    assert_eq!(net.network_fee, 100);
    assert_eq!(net.processing_fee, 100);
    assert_eq!(net.net_amount, 800);
    assert_reconciles(&net);
}

#[test]
fn test_preview_net_refund_max_fee_caps_total() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);

    client.set_refund_fee_config(
        &admin,
        &super::RefundFeeConfig {
            fee_bps: 1_000, // 10% of 10_000 == 1_000, capped to 250 below
            min_fee: 0,
            max_fee: 250,
            treasury: Address::generate(&env),
            fee_token: Address::generate(&env),
            active: true,
            network_fee_share_bps: 0,
        },
    );

    let net = client.preview_net_refund(&10_000i128);
    assert_eq!(net.total_fee, 250);
    assert_eq!(net.network_fee, 0);
    assert_eq!(net.processing_fee, 250);
    assert_eq!(net.net_amount, 9_750);
    assert_reconciles(&net);
}

#[test]
fn test_set_refund_fee_config_rejects_invalid_values() {
    let env = Env::default();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let stranger = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);

    let base = super::RefundFeeConfig {
        fee_bps: 100,
        min_fee: 0,
        max_fee: 0,
        treasury: Address::generate(&env),
        fee_token: Address::generate(&env),
        active: true,
        network_fee_share_bps: 2_500,
    };

    // Non-admin.
    let mut bad = base.clone();
    bad.fee_bps = 10_001;
    assert_eq!(
        client.try_set_refund_fee_config(&stranger, &bad),
        Err(Ok(super::Error::Core(super::CoreError::Unauthorized)))
    );

    // fee_bps above 10_000.
    let mut bad = base.clone();
    bad.fee_bps = 10_001;
    assert_eq!(
        client.try_set_refund_fee_config(&admin, &bad),
        Err(Ok(super::Error::Core(super::CoreError::InvalidFeeConfig)))
    );

    // network share above 10_000.
    let mut bad = base.clone();
    bad.network_fee_share_bps = 10_001;
    assert_eq!(
        client.try_set_refund_fee_config(&admin, &bad),
        Err(Ok(super::Error::Core(super::CoreError::InvalidFeeConfig)))
    );

    // min_fee above max_fee.
    let mut bad = base.clone();
    bad.min_fee = 500;
    bad.max_fee = 100;
    assert_eq!(
        client.try_set_refund_fee_config(&admin, &bad),
        Err(Ok(super::Error::Core(super::CoreError::InvalidFeeConfig)))
    );

    // Nothing was persisted by any rejected call.
    assert!(client.get_refund_fee_config().is_none());
}

#[test]
fn test_process_refund_transfers_net_amount_and_fee_to_treasury() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let treasury = Address::generate(&env);

    let asset = env.register_stellar_asset_contract_v2(admin.clone());
    let token_addr = asset.address();
    let asset_client = token::StellarAssetClient::new(&env, &token_addr);
    asset_client.mint(&merchant, &100_000);

    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    // 5% fee, 20% of which is the network share.
    client.set_refund_fee_config(
        &admin,
        &super::RefundFeeConfig {
            fee_bps: 500,
            min_fee: 0,
            max_fee: 0,
            treasury: treasury.clone(),
            fee_token: token_addr.clone(),
            active: true,
            network_fee_share_bps: 2_000,
        },
    );

    // Fund the refund pool so it can pay the customer out.
    asset_client.mint(&contract_id, &100_000);

    let reason = String::from_str(&env, "defective item");
    let refund_id = client.request_refund(
        &merchant,
        &1u64,
        &customer,
        &10_000i128,
        &10_000i128,
        &token_addr,
        &reason,
        &RefundReasonCode::ProductDefect,
        &0u64,
    );
    client.approve_refund(&admin, &refund_id);
    client.process_refund(&admin, &refund_id);

    let token_client = token::Client::new(&env, &token_addr);
    // 5% of 10_000 == 500 deducted; 20% of 500 == 100 network, 400 processing.
    assert_eq!(token_client.balance(&customer), 9_500);
    assert_eq!(token_client.balance(&treasury), 500);

    let net = client.preview_net_refund(&10_000i128);
    assert_eq!(net.total_fee, 500);
    assert_eq!(net.network_fee, 100);
    assert_eq!(net.processing_fee, 400);
    assert_eq!(net.net_amount, 9_500);
    assert_reconciles(&net);

    let refund = client.get_refund(&refund_id);
    assert_eq!(refund.status, RefundStatus::Processed);
}

#[test]
fn test_process_refund_without_fee_config_pays_full_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    let asset = env.register_stellar_asset_contract_v2(admin.clone());
    let token_addr = asset.address();
    let asset_client = token::StellarAssetClient::new(&env, &token_addr);

    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.initialize(&admin);
    asset_client.mint(&contract_id, &100_000);

    let reason = String::from_str(&env, "customer request");
    let refund_id = client.request_refund(
        &merchant,
        &2u64,
        &customer,
        &4_200i128,
        &4_200i128,
        &token_addr,
        &reason,
        &RefundReasonCode::CustomerRequest,
        &0u64,
    );
    client.approve_refund(&admin, &refund_id);
    client.process_refund(&admin, &refund_id);

    let token_client = token::Client::new(&env, &token_addr);
    assert_eq!(token_client.balance(&customer), 4_200);
}
