#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

use crate::{PaymentContract, PaymentContractClient};

#[test]
fn streaming_claims_accrue_linearly_and_close_refunds_unearned_funds() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(&env, &contract_id);
    client.initialize(&admin);
    token::StellarAssetClient::new(&env, &token_id).mint(&customer, &1_000i128);

    env.ledger().set_timestamp(100);
    let channel_id = client.open_streaming_channel(
        &customer, &merchant, &token_id, &1_000i128, &10i128, &100u64, &200u64,
    );
    env.ledger().set_timestamp(130);
    assert_eq!(client.get_streaming_claimable(&channel_id), 300);
    assert_eq!(client.claim_streaming_channel(&merchant, &channel_id), 300);

    env.ledger().set_timestamp(150);
    assert_eq!(client.get_streaming_claimable(&channel_id), 200);
    assert_eq!(client.claim_streaming_channel(&merchant, &channel_id), 200);
    env.ledger().set_timestamp(160);
    assert_eq!(client.close_streaming_channel(&customer, &channel_id), 400);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&merchant), 600);
    assert_eq!(token_client.balance(&customer), 400);
}
