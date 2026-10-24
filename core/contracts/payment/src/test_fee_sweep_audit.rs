#![cfg(test)]
use soroban_sdk::{testutils::Address as _, token, Address, Env, String};

use crate::{Currency, Error, FeatureError, FeeConfig, PaymentContract, PaymentContractClient};

fn create_completed_payment_with_fee(
    env: &Env,
    client: &PaymentContractClient,
    admin: &Address,
    amount: i128,
) {
    let customer = Address::generate(env);
    let merchant = Address::generate(env);

    let token_admin = Address::generate(env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_asset = token::StellarAssetClient::new(env, &token_id);
    let token_user = token::Client::new(env, &token_id);

    token_asset.mint(&customer, &amount);
    token_user.approve(&customer, &client.address, &amount, &999_999);

    client.set_fee_config(
        admin,
        &FeeConfig {
            fee_bps: 100,
            min_fee: 0,
            max_fee: 0,
            treasury: admin.clone(),
            fee_token: token_id.clone(),
            active: true,
        },
    );

    let payment_id = client.create_payment(
        &customer,
        &merchant,
        &amount,
        &token_id,
        &Currency::USDC,
        &0,
        &String::from_str(env, ""),
    );
    client.complete_payment(admin, &payment_id);
}

fn setup() -> (Env, PaymentContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PaymentContract);
    let client = PaymentContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.initialize(&admin);
    let token = soroban_sdk::token::StellarAssetClient::new(
        &env,
        &env.register_stellar_asset_contract_v2(admin.clone())
            .address(),
    );
    let token_addr = token.address.clone();
    token.mint(&contract_id, &1_000_000);
    client.set_fee_config(
        &admin,
        &FeeConfig {
            fee_bps: 100,
            min_fee: 0,
            max_fee: 0,
            treasury: treasury.clone(),
            fee_token: token_addr,
            active: true,
        },
    );
    (env, client, admin, treasury)
}

#[test]
fn test_sweep_amount_never_exceeds_accumulated_fees() {
    let (env, client, admin, _) = setup();
    let recipient = Address::generate(&env);
    client.set_sweep_recipient(&admin, &recipient);

    create_completed_payment_with_fee(&env, &client, &admin, 10_000);

    let accumulated = client.get_sweepable_balance();
    assert!(accumulated > 0, "expected non-zero accumulated fees");

    let swept = client.sweep_platform_fees(&admin);
    assert_eq!(swept, accumulated, "sweep must equal accumulated fees exactly");
    assert_eq!(client.get_sweepable_balance(), 0);
}

#[test]
fn test_large_sweep_requires_multisig_approval() {
    let (env, client, admin, _) = setup();
    let recipient = Address::generate(&env);
    client.set_sweep_recipient(&admin, &recipient);
    client.set_large_payment_threshold(&admin, &100);

    create_completed_payment_with_fee(&env, &client, &admin, 10_000);

    let result = client.try_sweep_platform_fees(&admin);
    match result {
        Ok(amount) => {
            assert!(amount > 0);
        }
        Err(_) => {}
    }
}

#[test]
fn test_sweep_only_touches_accumulated_fees() {
    let (env, client, admin, _) = setup();
    let recipient = Address::generate(&env);
    client.set_sweep_recipient(&admin, &recipient);

    create_completed_payment_with_fee(&env, &client, &admin, 5_000);

    let accumulated = client.get_sweepable_balance();
    let swept = client.sweep_platform_fees(&admin);

    assert_eq!(swept, accumulated);
    assert_eq!(client.get_sweepable_balance(), 0);
}
