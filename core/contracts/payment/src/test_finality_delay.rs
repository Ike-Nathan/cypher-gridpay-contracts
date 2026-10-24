#![cfg(test)]
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

use crate::{
    Currency, Error, FeatureError, FinalityConfig, PaymentContract, PaymentContractClient,
};

fn setup() -> (Env, PaymentContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PaymentContract);
    let client = PaymentContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token = soroban_sdk::token::StellarAssetClient::new(&env, &token_addr);
    token.mint(&contract_id, &10_000_000);
    client.initialize(&admin);
    (env, client, admin, token_addr)
}

#[test]
fn test_configure_finality_delay() {
    let (_, client, admin, _) = setup();
    client.configure_finality_delay(
        &admin,
        &FinalityConfig {
            delay_seconds: 86400,
            min_amount_threshold: 100,
            active: true,
        },
    );
    let cfg = client.get_finality_config().unwrap();
    assert_eq!(cfg.delay_seconds, 86400);
    assert!(cfg.active);
}

#[test]
fn test_complete_payment_creates_pending_settlement() {
    let (env, client, admin, token_addr) = setup();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = soroban_sdk::token::StellarAssetClient::new(&env, &token_addr);
    token.mint(&customer, &10_000);

    client.configure_finality_delay(
        &admin,
        &FinalityConfig {
            delay_seconds: 3600,
            min_amount_threshold: 100,
            active: true,
        },
    );

    let payment_id = client.create_payment(
        &customer,
        &merchant,
        &500,
        &token_addr,
        &Currency::USDC,
        &0,
        &String::from_str(&env, ""),
    );
    client.complete_payment(&admin, &payment_id);

    let settlements = client.get_pending_settlements(&merchant, &10, &0);
    assert_eq!(settlements.len(), 1);
    assert_eq!(settlements.get(0).unwrap().payment_id, payment_id);
}

#[test]
fn test_finalize_before_release_at_fails() {
    let (env, client, admin, token_addr) = setup();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = soroban_sdk::token::StellarAssetClient::new(&env, &token_addr);
    token.mint(&customer, &10_000);

    client.configure_finality_delay(
        &admin,
        &FinalityConfig {
            delay_seconds: 3600,
            min_amount_threshold: 100,
            active: true,
        },
    );

    let payment_id = client.create_payment(
        &customer,
        &merchant,
        &500,
        &token_addr,
        &Currency::USDC,
        &0,
        &String::from_str(&env, ""),
    );
    client.complete_payment(&admin, &payment_id);

    let result = client.try_finalize_pending_settlement(&payment_id);
    assert_eq!(
        result,
        Err(Ok(Error::Feature(FeatureError::SettlementNotReady)))
    );
}

#[test]
fn test_finalize_after_delay_succeeds() {
    let (env, client, admin, token_addr) = setup();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = soroban_sdk::token::StellarAssetClient::new(&env, &token_addr);
    token.mint(&customer, &10_000);

    client.configure_finality_delay(
        &admin,
        &FinalityConfig {
            delay_seconds: 3600,
            min_amount_threshold: 100,
            active: true,
        },
    );

    let payment_id = client.create_payment(
        &customer,
        &merchant,
        &500,
        &token_addr,
        &Currency::USDC,
        &0,
        &String::from_str(&env, ""),
    );
    client.complete_payment(&admin, &payment_id);

    // Advance time past the delay
    env.ledger().with_mut(|l| l.timestamp += 7200);

    client.finalize_pending_settlement(&payment_id);

    // Double finalization should fail
    let result = client.try_finalize_pending_settlement(&payment_id);
    assert_eq!(
        result,
        Err(Ok(Error::Feature(FeatureError::SettlementAlreadyFinalized)))
    );
}

#[test]
fn test_threshold_bypass_settles_immediately() {
    let (env, client, admin, token_addr) = setup();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = soroban_sdk::token::StellarAssetClient::new(&env, &token_addr);
    token.mint(&customer, &10_000);
    soroban_sdk::token::Client::new(&env, &token_addr).approve(
        &customer,
        &client.address,
        &50,
        &10_000,
    );

    // min_amount_threshold = 1000, payment = 50 → bypass
    client.configure_finality_delay(
        &admin,
        &FinalityConfig {
            delay_seconds: 3600,
            min_amount_threshold: 1000,
            active: true,
        },
    );

    let payment_id = client.create_payment(
        &customer,
        &merchant,
        &50,
        &token_addr,
        &Currency::USDC,
        &0,
        &String::from_str(&env, ""),
    );
    client.complete_payment(&admin, &payment_id);

    // No pending settlement created for below-threshold payment
    let settlements = client.get_pending_settlements(&merchant, &10, &0);
    assert_eq!(settlements.len(), 0);
}

// ── Risk-tier scaling and fraud challenge (#196) ─────────────────────────────

mod risk_tier {
    use super::*;
    use crate::{FinalityRiskConfig, MerchantVerificationLevel, PaymentError};
    use soroban_sdk::{symbol_short, Symbol};

    fn risk_cfg(oracle: Option<Address>) -> FinalityRiskConfig {
        FinalityRiskConfig {
            base_delay_secs: 1000,
            high_value_threshold: 5_000,
            high_value_multiplier_bps: 30_000,
            verified_merchant_discount_bps: 9_000,
            min_delay_floor_secs: 400,
            risk_oracle: oracle,
            emergency_pauser: None,
        }
    }

    fn pay(
        env: &Env,
        client: &PaymentContractClient,
        token_addr: &Address,
        customer: &Address,
        merchant: &Address,
        amount: i128,
    ) -> u64 {
        client.create_payment(
            customer,
            merchant,
            &amount,
            token_addr,
            &Currency::USDC,
            &0,
            &String::from_str(env, ""),
        )
    }

    fn setup_risk(
        oracle: Option<Address>,
    ) -> (Env, PaymentContractClient<'static>, Address, Address, Address, Address) {
        let (env, client, admin, token_addr) = setup();
        let customer = Address::generate(&env);
        let merchant = Address::generate(&env);
        soroban_sdk::token::StellarAssetClient::new(&env, &token_addr).mint(&customer, &100_000);
        client.configure_finality_delay(
            &admin,
            &FinalityConfig {
                delay_seconds: 100,
                min_amount_threshold: 100,
                active: true,
            },
        );
        client.configure_finality_risk(&admin, &risk_cfg(oracle));
        (env, client, admin, token_addr, customer, merchant)
    }

    #[test]
    fn test_delay_scales_with_amount() {
        let (env, client, _a, tok, c, m) = setup_risk(None);
        let small = pay(&env, &client, &tok, &c, &m, 1_000);
        let big = pay(&env, &client, &tok, &c, &m, 6_000);
        assert_eq!(client.get_required_finality_delay(&small), 1000);
        assert_eq!(client.get_required_finality_delay(&big), 3000);
    }

    #[test]
    fn test_verified_discount_and_floor() {
        let (env, client, admin, tok, c, m) = setup_risk(None);
        client.set_merchant_verification_level(&admin, &m, &MerchantVerificationLevel::Premium);
        let small = pay(&env, &client, &tok, &c, &m, 1_000);
        // 1000 * (1 - 0.9) = 100, raised to the 400 floor.
        assert_eq!(client.get_required_finality_delay(&small), 400);
    }

    #[test]
    fn test_flag_freezes_finalization_and_clear_releases() {
        let oracle = None;
        let (env, client, admin, tok, c, m) = setup_risk(oracle);
        let id = pay(&env, &client, &tok, &c, &m, 1_000);
        client.complete_payment(&admin, &id);
        client.flag_payment_during_finality(&id, &c, &symbol_short!("fraud"));
        assert!(client.get_finality_flag(&id).is_some());

        env.ledger().with_mut(|l| l.timestamp += 10_000);
        assert_eq!(
            client.try_finalize_pending_settlement(&id),
            Err(Ok(Error::Payment(PaymentError::PaymentFlagged)))
        );
        client.resolve_finality_flag(&admin, &id, &false);
        client.finalize_pending_settlement(&id);
    }

    #[test]
    fn test_flag_refund_cancels_settlement() {
        let (env, client, admin, tok, c, m) = setup_risk(None);
        let id = pay(&env, &client, &tok, &c, &m, 1_000);
        client.complete_payment(&admin, &id);
        client.flag_payment_during_finality(&id, &c, &symbol_short!("fraud"));
        client.resolve_finality_flag(&admin, &id, &true);
        assert_eq!(
            client.try_finalize_pending_settlement(&id),
            Err(Ok(Error::Feature(FeatureError::SettlementAlreadyFinalized)))
        );
    }

    #[test]
    fn test_flag_rejects_stranger_and_closed_window() {
        let (env, client, admin, tok, c, m) = setup_risk(None);
        let id = pay(&env, &client, &tok, &c, &m, 1_000);
        client.complete_payment(&admin, &id);
        let stranger = Address::generate(&env);
        let reason: Symbol = symbol_short!("fraud");
        assert_eq!(
            client.try_flag_payment_during_finality(&id, &stranger, &reason),
            Err(Ok(Error::Payment(PaymentError::NotAuthorizedChallenger)))
        );
        env.ledger().with_mut(|l| l.timestamp += 10_000);
        assert_eq!(
            client.try_flag_payment_during_finality(&id, &c, &reason),
            Err(Ok(Error::Payment(PaymentError::FinalityWindowClosed)))
        );
    }
}
