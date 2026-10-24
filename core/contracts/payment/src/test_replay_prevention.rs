#![cfg(test)]
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use soroban_sdk::{testutils::Address as _, token, Address, Bytes, BytesN, Env};

use crate::{Currency, Error, FeatureError, PaymentContract, PaymentContractClient};

#[test]
fn test_signature_replay_across_channels_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_asset = token::StellarAssetClient::new(&env, &token_id);

    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    token_asset.mint(&customer, &10_000i128);

    let mut rng = OsRng;
    let signing_key = SigningKey::generate(&mut rng);
    let pk_bytes = signing_key.verifying_key().to_bytes();
    let customer_pk = BytesN::<32>::from_array(&env, &pk_bytes);

    let channel_id_1 = client.open_channel(
        &customer,
        &merchant,
        &token_id,
        &5_000i128,
        &0u64,
        &customer_pk,
    );

    let channel_id_2 = client.open_channel(
        &customer,
        &merchant,
        &token_id,
        &5_000i128,
        &0u64,
        &customer_pk,
    );

    let merchant_amount: i128 = 100i128;
    let nonce: u64 = 1u64;

    let mut msg = Bytes::new(&env);
    msg.append(&channel_id_1.to_xdr(&env));
    msg.append(&merchant_amount.to_xdr(&env));
    msg.append(&nonce.to_xdr(&env));
    msg.append(&contract_id.to_xdr(&env));

    let msg_vec: alloc::vec::Vec<u8> = msg.iter().collect();
    let signature = signing_key.sign(&msg_vec);
    let sig_bn = BytesN::<64>::from_array(&env, &signature.to_bytes());

    client.settle_channel(&channel_id_1, &merchant_amount, &nonce, &sig_bn);

    let result = client.try_settle_channel(&channel_id_2, &merchant_amount, &nonce, &sig_bn);
    assert!(result.is_err(), "Cross-channel signature replay must be rejected");
}

#[test]
fn test_stale_nonce_replay_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_asset = token::StellarAssetClient::new(&env, &token_id);

    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    token_asset.mint(&customer, &10_000i128);

    let mut rng = OsRng;
    let signing_key = SigningKey::generate(&mut rng);
    let pk_bytes = signing_key.verifying_key().to_bytes();
    let customer_pk = BytesN::<32>::from_array(&env, &pk_bytes);

    let channel_id = client.open_channel(
        &customer,
        &merchant,
        &token_id,
        &5_000i128,
        &0u64,
        &customer_pk,
    );

    let merchant_amount: i128 = 100i128;
    let nonce: u64 = 5u64;

    let mut msg = Bytes::new(&env);
    msg.append(&channel_id.to_xdr(&env));
    msg.append(&merchant_amount.to_xdr(&env));
    msg.append(&nonce.to_xdr(&env));
    msg.append(&contract_id.to_xdr(&env));

    let msg_vec: alloc::vec::Vec<u8> = msg.iter().collect();
    let signature = signing_key.sign(&msg_vec);
    let sig_bn = BytesN::<64>::from_array(&env, &signature.to_bytes());

    client.settle_channel(&channel_id, &merchant_amount, &nonce, &sig_bn);

    let stale_nonce: u64 = 1u64;
    let mut msg2 = Bytes::new(&env);
    msg2.append(&channel_id.to_xdr(&env));
    msg2.append(&merchant_amount.to_xdr(&env));
    msg2.append(&stale_nonce.to_xdr(&env));
    msg2.append(&contract_id.to_xdr(&env));

    let msg_vec2: alloc::vec::Vec<u8> = msg2.iter().collect();
    let signature2 = signing_key.sign(&msg_vec2);
    let sig_bn2 = BytesN::<64>::from_array(&env, &signature2.to_bytes());

    let result = client.try_settle_channel(&channel_id, &merchant_amount, &stale_nonce, &sig_bn2);
    assert!(result.is_err(), "Stale nonce replay must be rejected");
}
