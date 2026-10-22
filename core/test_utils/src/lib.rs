//! Shared test fixtures for the core contracts (#97): a mocked environment,
//! a set of party addresses, and a funded Stellar asset token.
#![no_std]

use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env};

/// A default test environment with every `require_auth` mocked.
pub fn setup_test_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

/// Three distinct generated addresses: `(payer, payee, arbiter)`.
pub fn generate_parties(env: &Env) -> (Address, Address, Address) {
    (Address::generate(env), Address::generate(env), Address::generate(env))
}

/// Registers a Stellar asset contract and mints `amount` to `holder`.
/// Returns the token contract's address.
pub fn create_mock_token(env: &Env, holder: &Address, amount: i128) -> Address {
    let issuer = Address::generate(env);
    let token = env.register_stellar_asset_contract_v2(issuer).address();
    StellarAssetClient::new(env, &token).mint(holder, &amount);
    token
}
