#![cfg(test)]

use super::*;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn setup() -> (Env, PaymentContractClient<'static>, Address) {
    let env = Env::default();
    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);
    (env, client, admin)
}

macro_rules! make_payment {
    ($client:expr, $env:expr, $customer:expr, $merchant:expr, $token:expr, $amount:expr) => {
        $client.try_create_payment(
            $customer,
            $merchant,
            &$amount,
            $token,
            &Currency::XLM,
            &0u64,
            &String::from_str($env, ""),
        )
    };
}

/// Issue #23: A merchant's 24-hour aggregate transaction volume must be capped
/// at their verification tier's `volume_limit`, even when each individual
/// payment stays within any per-transaction or hourly rate limit.
#[test]
fn test_daily_tier_volume_throttling() {
    let (env, client, admin) = setup();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = Address::generate(&env);

    env.ledger().set_timestamp(1000);

    client.set_merchant_verification_level(&admin, &merchant, &MerchantVerificationLevel::Basic);
    client.set_verification_tier_limits(
        &admin,
        &VerificationTierLimits {
            level: MerchantVerificationLevel::Basic,
            tx_per_period: 0,
            volume_limit: 1000,
        },
    );

    // First payment of 600 stays within the 1000 daily cap.
    let result = make_payment!(client, &env, &customer, &merchant, &token, 600i128);
    assert!(result.is_ok());

    // Second payment of 500 would push the aggregate to 1100 > 1000 → rejected.
    let result = make_payment!(client, &env, &customer, &merchant, &token, 500i128);
    assert_eq!(
        result,
        Err(Ok(Error::Basic(BasicError::DailyTierLimitExceeded)))
    );

    // A new calendar day resets the aggregate, so the same payment now succeeds.
    env.ledger().set_timestamp(1000 + 86400);
    let result = make_payment!(client, &env, &customer, &merchant, &token, 500i128);
    assert!(result.is_ok());
}
