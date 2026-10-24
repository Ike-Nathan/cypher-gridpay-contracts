#![cfg(test)]

use soroban_sdk::{testutils::Address as _, token, Address, Env, String};

use crate::{Currency, FeeConfig, PaymentContract, PaymentContractClient};

#[test]
fn completion_relayers_accrue_and_claim_a_share_of_fees() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(&env, &contract_id);
    client.initialize(&admin);
    client.set_fee_config(
        &admin,
        &FeeConfig {
            fee_bps: 100,
            min_fee: 0,
            max_fee: 0,
            treasury: admin.clone(),
            fee_token: token_id.clone(),
            active: true,
        },
    );
    client.set_relayer_stipend_bps(&admin, &2_500);

    let payment_id = client.create_payment(
        &customer,
        &merchant,
        &1_000i128,
        &token_id,
        &Currency::USDC,
        &0u64,
        &String::from_str(&env, ""),
    );
    token::StellarAssetClient::new(&env, &token_id).mint(&customer, &1_000i128);
    token::Client::new(&env, &token_id).approve(&customer, &contract_id, &1_000i128, &100_000);
    client.complete_payment(&admin, &payment_id);

    assert_eq!(client.get_relayer_stipend(&admin, &token_id), 2);
    assert_eq!(client.get_accumulated_fees(), 8);
    assert_eq!(client.claim_relayer_stipend(&admin, &token_id), 2);
    assert_eq!(token::Client::new(&env, &token_id).balance(&admin), 2);
    assert_eq!(client.get_relayer_stipend(&admin, &token_id), 0);
}
