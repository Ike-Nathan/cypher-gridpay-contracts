#![cfg(test)]
extern crate alloc;
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token,
    xdr::ToXdr,
    Address, Bytes, BytesN, Env, Symbol,
};

use crate::{channels, PaymentContract, PaymentContractClient};

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

    // The domain-bound settle digest (channels.rs) is what gets signed.
    let msg = Bytes::from(
        PaymentContractClient::new(&env, &contract_id).channel_settle_digest(&channel_id_1, &merchant_amount, &nonce),
    );

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

    // The domain-bound settle digest (channels.rs) is what gets signed.
    let msg = Bytes::from(
        PaymentContractClient::new(&env, &contract_id).channel_settle_digest(&channel_id, &merchant_amount, &nonce),
    );

    let msg_vec: alloc::vec::Vec<u8> = msg.iter().collect();
    let signature = signing_key.sign(&msg_vec);
    let sig_bn = BytesN::<64>::from_array(&env, &signature.to_bytes());

    client.settle_channel(&channel_id, &merchant_amount, &nonce, &sig_bn);

    let stale_nonce: u64 = 1u64;
    // The domain-bound settle digest (channels.rs) is what gets signed.
    let msg2 = Bytes::from(
        PaymentContractClient::new(&env, &contract_id).channel_settle_digest(&channel_id, &merchant_amount, &stale_nonce),
    );

    let msg_vec2: alloc::vec::Vec<u8> = msg2.iter().collect();
    let signature2 = signing_key.sign(&msg_vec2);
    let sig_bn2 = BytesN::<64>::from_array(&env, &signature2.to_bytes());

    let result = client.try_settle_channel(&channel_id, &merchant_amount, &stale_nonce, &sig_bn2);
    assert!(result.is_err(), "Stale nonce replay must be rejected");
}

// ── Domain separator (channels.rs) ───────────────────────────────────────────

struct Channel<'a> {
    env: Env,
    contract_id: Address,
    client: PaymentContractClient<'a>,
    signing_key: SigningKey,
    channel_id: u64,
}

/// A payment contract with one open 5_000-token channel.
fn open_channel(env: &Env) -> Channel<'_> {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let merchant = Address::generate(env);
    let customer = Address::generate(env);
    let token_id = env
        .register_stellar_asset_contract_v2(Address::generate(env))
        .address();
    token::StellarAssetClient::new(env, &token_id).mint(&customer, &10_000i128);

    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(env, &contract_id);
    client.initialize(&admin);

    let signing_key = SigningKey::generate(&mut OsRng);
    let customer_pk = BytesN::<32>::from_array(env, &signing_key.verifying_key().to_bytes());
    let channel_id =
        client.open_channel(&customer, &merchant, &token_id, &5_000i128, &0u64, &customer_pk);
    Channel {
        env: env.clone(),
        contract_id,
        client,
        signing_key,
        channel_id,
    }
}

impl Channel<'_> {
    fn sign(&self, message: &[u8]) -> BytesN<64> {
        BytesN::from_array(&self.env, &self.signing_key.sign(message).to_bytes())
    }

    fn sign_digest(&self, digest: &BytesN<32>) -> BytesN<64> {
        self.sign(&digest.to_array())
    }
}

#[test]
fn test_signature_over_domain_digest_settles() {
    let env = Env::default();
    let c = open_channel(&env);
    let digest = c.client.channel_settle_digest(&c.channel_id, &100, &1);
    c.client
        .settle_channel(&c.channel_id, &100, &1, &c.sign_digest(&digest));
}

#[test]
fn test_cross_network_replay_rejected() {
    let env = Env::default();
    let c = open_channel(&env);
    // Signed on one network...
    env.ledger().set_network_id([7u8; 32]);
    let testnet_sig = c.sign_digest(&c.client.channel_settle_digest(&c.channel_id, &100, &1));

    // ...submitted on another: the network id is part of the domain.
    env.ledger().set_network_id([9u8; 32]);
    let result = c.client.try_settle_channel(&c.channel_id, &100, &1, &testnet_sig);
    assert!(result.is_err(), "a signature from another network must be rejected");

    // Back on the network it was signed for, it is valid.
    env.ledger().set_network_id([7u8; 32]);
    c.client.settle_channel(&c.channel_id, &100, &1, &testnet_sig);
}

#[test]
fn test_cross_contract_replay_rejected() {
    let env = Env::default();
    let a = open_channel(&env);
    // A redeployed instance with the same channel id and the same customer key.
    let b_id = env.register(PaymentContract, ());
    let b = PaymentContractClient::new(&env, &b_id);
    b.initialize(&Address::generate(&env));
    let token_id = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let customer = Address::generate(&env);
    token::StellarAssetClient::new(&env, &token_id).mint(&customer, &10_000i128);
    let pk = BytesN::<32>::from_array(&env, &a.signing_key.verifying_key().to_bytes());
    let b_channel = b.open_channel(&customer, &Address::generate(&env), &token_id, &5_000i128, &0u64, &pk);
    assert_eq!(b_channel, a.channel_id);

    let sig_for_a = a.sign_digest(&a.client.channel_settle_digest(&a.channel_id, &100, &1));
    assert_ne!(
        a.client.channel_settle_digest(&a.channel_id, &100, &1),
        b.channel_settle_digest(&b_channel, &100, &1),
        "digests differ per contract instance"
    );
    let result = b.try_settle_channel(&b_channel, &100, &1, &sig_for_a);
    assert!(result.is_err(), "a signature for another contract must be rejected");
}

#[test]
fn test_cross_action_replay_rejected() {
    let env = Env::default();
    let c = open_channel(&env);
    // Same struct hash, signed for a different entry point's action tag.
    let other_action_digest = env.as_contract(&c.contract_id, || {
        let struct_hash = channels::channel_settle_struct_hash(&env, c.channel_id, 100, 1);
        channels::signing_digest(&env, "channel_close", &struct_hash)
    });
    assert_ne!(other_action_digest, c.client.channel_settle_digest(&c.channel_id, &100, &1));
    let result = c
        .client
        .try_settle_channel(&c.channel_id, &100, &1, &c.sign_digest(&other_action_digest));
    assert!(result.is_err(), "a signature for another action must be rejected");
}

#[test]
fn test_pre_domain_message_format_rejected() {
    let env = Env::default();
    let c = open_channel(&env);
    // The old pre-image: raw (channel_id, amount, nonce, contract) XDR.
    let mut msg = Bytes::new(&env);
    msg.append(&c.channel_id.to_xdr(&env));
    msg.append(&100i128.to_xdr(&env));
    msg.append(&1u64.to_xdr(&env));
    msg.append(&c.contract_id.clone().to_xdr(&env));
    let raw: alloc::vec::Vec<u8> = msg.iter().collect();
    let result = c.client.try_settle_channel(&c.channel_id, &100, &1, &c.sign(&raw));
    assert!(result.is_err(), "an undomained signature must be rejected");
}

#[test]
fn test_digest_matches_documented_byte_layout() {
    let env = Env::default();
    env.ledger().set_network_id([3u8; 32]);
    let c = open_channel(&env);

    // Rebuild the digest exactly as SECURITY.md documents it.
    let contract_xdr = c.contract_id.clone().to_xdr(&env);
    assert_eq!(contract_xdr.len(), 40, "contract address XDR is 40 bytes");
    let mut domain = Bytes::from_slice(&env, b"CYPHER_GRIDPAY_DOMAIN_V1");
    domain.append(&Bytes::from_array(&env, &[3u8; 32]));
    domain.append(&contract_xdr);
    domain.append(&Bytes::from_slice(&env, b"channel_settle"));
    let domain_hash = env.crypto().sha256(&domain).to_bytes();

    let mut fields = Bytes::new(&env);
    fields.append(&c.channel_id.to_xdr(&env));
    fields.append(&250i128.to_xdr(&env));
    fields.append(&4u64.to_xdr(&env));
    let struct_hash = env.crypto().sha256(&fields).to_bytes();

    let mut pre = Bytes::from_array(&env, &[0x19, 0x01]);
    pre.append(&Bytes::from(domain_hash));
    pre.append(&Bytes::from(struct_hash));
    assert_eq!(pre.len(), 66);
    let expected = env.crypto().sha256(&pre).to_bytes();

    assert_eq!(c.client.channel_settle_digest(&c.channel_id, &250, &4), expected);
}

#[test]
fn test_get_domain_separator_reports_network_and_contract() {
    let env = Env::default();
    env.ledger().set_network_id([5u8; 32]);
    let c = open_channel(&env);
    let domain = c.client.get_domain_separator();
    assert_eq!(domain.network_id, BytesN::from_array(&env, &[5u8; 32]));
    assert_eq!(domain.contract_address, c.contract_id);
    assert_eq!(domain.version, 1);
    assert_eq!(domain.protocol_name, Symbol::new(&env, "CYPHER_GRIDPAY"));
}
