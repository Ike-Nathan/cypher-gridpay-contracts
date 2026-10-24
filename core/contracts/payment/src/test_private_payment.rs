#![cfg(test)]

use soroban_sdk::{testutils::Address as _, token, xdr::ToXdr, Address, BytesN, Env, String};

use crate::{Currency, PaymentContract, PaymentContractClient};

#[test]
fn private_payment_hides_vault_until_commitment_reveal() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let vault = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let secret = BytesN::<32>::from_array(&env, &[7u8; 32]);
    let mut preimage = vault.to_xdr(&env);
    preimage.append(&secret.to_xdr(&env));
    let commitment = env.crypto().sha256(&preimage);
    let payment_id = client.create_private_payment(
        &customer,
        &commitment,
        &500i128,
        &token_id,
        &Currency::USDC,
        &0u64,
        &String::from_str(&env, ""),
    );

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.merchant, contract_id);

    let wrong_secret = BytesN::<32>::from_array(&env, &[8u8; 32]);
    assert!(client
        .try_complete_private_payment(&vault, &wrong_secret, &payment_id)
        .is_err());
    assert_eq!(client.get_payment(&payment_id).merchant, contract_id);

    token::StellarAssetClient::new(&env, &token_id).mint(&customer, &500i128);
    token::Client::new(&env, &token_id).approve(&customer, &contract_id, &500i128, &100_000);
    client.complete_private_payment(&vault, &secret, &payment_id);

    assert_eq!(token::Client::new(&env, &token_id).balance(&vault), 500i128);
    assert_eq!(client.get_payment(&payment_id).merchant, vault);
}
