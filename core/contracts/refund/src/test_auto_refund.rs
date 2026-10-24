#![cfg(test)]

use super::*;
use soroban_sdk::{
    contract, contractimpl, testutils::Address as _, testutils::Ledger, Address, Bytes, BytesN,
    Env, String,
};

fn setup(env: &Env) -> (RefundContractClient, Address) {
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin);
    (client, admin)
}

fn install_mock_payment_contract(env: &Env, payment: ExternalPayment) -> Address {
    let contract_id = env.register(MockPaymentContract, ());
    let client = MockPaymentContractClient::new(env, &contract_id);
    client.set_payment(&payment);
    contract_id
}

#[contract]
struct MockPaymentContract;

#[contractimpl]
impl MockPaymentContract {
    pub fn set_payment(env: Env, payment: ExternalPayment) {
        env.storage().instance().set(&0u32, &payment);
    }

    pub fn get_payment(env: Env, payment_id: u64) -> ExternalPayment {
        let payment: ExternalPayment = env.storage().instance().get(&0u32).unwrap();
        assert_eq!(payment.id, payment_id);
        payment
    }

    pub fn check_payment_customer(env: Env, payment_id: u64, customer: Address) -> bool {
        let payment: ExternalPayment = env.storage().instance().get(&0u32).unwrap();
        payment.id == payment_id
            && payment.customer == customer
            && payment.status == ExternalPaymentStatus::Completed
    }
}

#[contract]
struct MockStateContract;

#[contractimpl]
impl MockStateContract {
    pub fn set_contract_state(env: Env, key: BytesN<32>, value: Bytes) {
        env.storage().instance().set(&key, &value);
    }

    pub fn get_contract_state(env: Env, key: BytesN<32>) -> Bytes {
        env.storage()
            .instance()
            .get(&key)
            .unwrap_or(Bytes::new(&env))
    }
}

fn sample_payment(
    env: &Env,
    merchant: &Address,
    customer: &Address,
    token: &Address,
) -> ExternalPayment {
    ExternalPayment {
        id: 7,
        customer: customer.clone(),
        merchant: merchant.clone(),
        amount: 10_000,
        token: token.clone(),
        currency: ExternalCurrency::USDC,
        status: ExternalPaymentStatus::Completed,
        created_at: 1_000,
        expires_at: 0,
        metadata: String::from_str(env, ""),
        notes: String::from_str(env, ""),
        refunded_amount: 0,
    }
}

#[test]
fn test_evaluate_auto_refund_triggers_on_timeout() {
    let env = Env::default();
    env.ledger().set_timestamp(10_000);
    let (client, admin) = setup(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = Address::generate(&env);
    let payment_contract =
        install_mock_payment_contract(&env, sample_payment(&env, &merchant, &customer, &token));
    client.set_payment_contract_address(&admin, &payment_contract);

    let trigger_id = client.register_auto_refund_trigger(
        &merchant,
        &7u64,
        &AutoRefundCondition::FulfillmentTimeout(FulfillmentTimeoutCondition {
            fulfillment_deadline: 9_000,
        }),
        &2_500u32,
    );

    assert!(client.evaluate_auto_refund(&trigger_id));

    let refund = client.get_refund(&1u64);
    assert_eq!(refund.status, RefundStatus::Processed);
    assert_eq!(refund.amount, 2_500i128);

    let trigger = client.get_auto_refund_trigger(&trigger_id);
    assert!(!trigger.active);
}

#[test]
fn test_evaluate_auto_refund_holds_before_timeout() {
    let env = Env::default();
    env.ledger().set_timestamp(5_000);
    let (client, admin) = setup(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = Address::generate(&env);
    let payment_contract =
        install_mock_payment_contract(&env, sample_payment(&env, &merchant, &customer, &token));
    client.set_payment_contract_address(&admin, &payment_contract);

    let trigger_id = client.register_auto_refund_trigger(
        &merchant,
        &7u64,
        &AutoRefundCondition::FulfillmentTimeout(FulfillmentTimeoutCondition {
            fulfillment_deadline: 9_000,
        }),
        &2_500u32,
    );

    assert!(!client.evaluate_auto_refund(&trigger_id));
    assert!(client.try_get_refund(&1u64).is_err());

    let trigger = client.get_auto_refund_trigger(&trigger_id);
    assert!(trigger.active);
}

#[test]
fn test_evaluate_auto_refund_triggers_on_contract_state_match() {
    let env = Env::default();
    env.ledger().set_timestamp(10_000);
    let (client, admin) = setup(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = Address::generate(&env);
    let payment_contract =
        install_mock_payment_contract(&env, sample_payment(&env, &merchant, &customer, &token));
    client.set_payment_contract_address(&admin, &payment_contract);

    let state_contract_id = env.register(MockStateContract, ());
    let state_client = MockStateContractClient::new(&env, &state_contract_id);
    let key = BytesN::from_array(&env, &[1; 32]);
    let expected = Bytes::from_slice(&env, b"fulfilled");
    state_client.set_contract_state(&key, &expected);

    let trigger_id = client.register_auto_refund_trigger(
        &merchant,
        &7u64,
        &AutoRefundCondition::ContractStateMatch(ContractStateMatchCondition {
            contract: state_contract_id,
            key: key.clone(),
            expected: expected.clone(),
        }),
        &5_000u32,
    );

    assert!(client.evaluate_auto_refund(&trigger_id));

    let refund = client.get_refund(&1u64);
    assert_eq!(refund.status, RefundStatus::Processed);
    assert_eq!(refund.amount, 5_000i128);
}

#[test]
fn test_evaluate_auto_refund_cannot_retrigger_after_success() {
    let env = Env::default();
    env.ledger().set_timestamp(10_000);
    let (client, admin) = setup(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = Address::generate(&env);
    let payment_contract =
        install_mock_payment_contract(&env, sample_payment(&env, &merchant, &customer, &token));
    client.set_payment_contract_address(&admin, &payment_contract);

    let trigger_id = client.register_auto_refund_trigger(
        &merchant,
        &7u64,
        &AutoRefundCondition::FulfillmentTimeout(FulfillmentTimeoutCondition {
            fulfillment_deadline: 9_000,
        }),
        &2_500u32,
    );

    assert!(client.evaluate_auto_refund(&trigger_id));
    assert!(!client.evaluate_auto_refund(&trigger_id));
    assert_eq!(client.get_refund(&1u64).status, RefundStatus::Processed);
    assert!(client.try_get_refund(&2u64).is_err());
}

fn timeout_condition(deadline: u64) -> AutoRefundCondition {
    AutoRefundCondition::FulfillmentTimeout(FulfillmentTimeoutCondition {
        fulfillment_deadline: deadline,
    })
}

fn state_condition(
    env: &Env,
    contract: &Address,
    seed: u8,
    expected: &Bytes,
) -> AutoRefundCondition {
    AutoRefundCondition::ContractStateMatch(ContractStateMatchCondition {
        contract: contract.clone(),
        key: BytesN::from_array(env, &[seed; 32]),
        expected: expected.clone(),
    })
}

fn all_of(env: &Env, conditions: &[AutoRefundCondition]) -> AutoRefundCondition {
    let mut inner = soroban_sdk::Vec::new(env);
    for condition in conditions {
        inner.push_back(condition.clone());
    }
    AutoRefundCondition::All(inner)
}

/// Registers a state contract whose keys `[1; 32]..=[count; 32]` all hold `value`.
fn install_state_contract(env: &Env, count: u8, value: &Bytes) -> Address {
    let state_contract_id = env.register(MockStateContract, ());
    let state_client = MockStateContractClient::new(env, &state_contract_id);
    for seed in 1..=count {
        state_client.set_contract_state(&BytesN::from_array(env, &[seed; 32]), value);
    }
    state_contract_id
}

#[test]
fn test_register_trigger_rejects_more_than_max_conditions() {
    let env = Env::default();
    env.ledger().set_timestamp(10_000);
    let (client, admin) = setup(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = Address::generate(&env);
    let payment_contract =
        install_mock_payment_contract(&env, sample_payment(&env, &merchant, &customer, &token));
    client.set_payment_contract_address(&admin, &payment_contract);

    let conditions: std::vec::Vec<AutoRefundCondition> = (0..(MAX_TRIGGER_CONDITIONS as u64 + 1))
        .map(|i| timeout_condition(9_000 + i))
        .collect();
    let result = client.try_register_auto_refund_trigger(
        &merchant,
        &7u64,
        &all_of(&env, &conditions),
        &2_500u32,
    );
    assert_eq!(
        result,
        Err(Ok(Error::Ext(ExtError::TooManyTriggerConditions)))
    );

    // Nested leaves count toward the same limit.
    let nested = all_of(
        &env,
        &[
            all_of(&env, &conditions[0..3]),
            all_of(&env, &conditions[3..6]),
        ],
    );
    let result = client.try_register_auto_refund_trigger(&merchant, &7u64, &nested, &2_500u32);
    assert_eq!(
        result,
        Err(Ok(Error::Ext(ExtError::TooManyTriggerConditions)))
    );

    // Nesting deeper than MAX_TRIGGER_CONDITION_DEPTH is rejected even with few leaves.
    let too_deep = all_of(
        &env,
        &[all_of(&env, &[all_of(&env, &[timeout_condition(9_000)])])],
    );
    let result = client.try_register_auto_refund_trigger(&merchant, &7u64, &too_deep, &2_500u32);
    assert_eq!(
        result,
        Err(Ok(Error::Ext(ExtError::TooManyTriggerConditions)))
    );

    // An empty composite is rejected rather than being vacuously true.
    let result =
        client.try_register_auto_refund_trigger(&merchant, &7u64, &all_of(&env, &[]), &2_500u32);
    assert_eq!(
        result,
        Err(Ok(Error::Ext(ExtError::TooManyTriggerConditions)))
    );

    // Exactly MAX_TRIGGER_CONDITIONS is accepted.
    client.register_auto_refund_trigger(
        &merchant,
        &7u64,
        &all_of(&env, &conditions[0..MAX_TRIGGER_CONDITIONS as usize]),
        &2_500u32,
    );
}

#[test]
fn test_composite_trigger_fires_only_when_all_conditions_met() {
    let env = Env::default();
    env.ledger().set_timestamp(10_000);
    let (client, admin) = setup(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    // A real asset funding the refund contract, so the payout can settle.
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&client.address, &10_000);
    let payment_contract =
        install_mock_payment_contract(&env, sample_payment(&env, &merchant, &customer, &token));
    client.set_payment_contract_address(&admin, &payment_contract);

    let fulfilled = Bytes::from_slice(&env, b"fulfilled");
    let state = install_state_contract(&env, 2, &fulfilled);

    let pending = all_of(
        &env,
        &[
            state_condition(&env, &state, 1, &fulfilled),
            timeout_condition(20_000),
        ],
    );
    let pending_id = client.register_auto_refund_trigger(&merchant, &7u64, &pending, &2_500u32);
    assert!(!client.evaluate_auto_refund(&pending_id));
    assert!(client.get_auto_refund_trigger(&pending_id).active);

    let ready = all_of(
        &env,
        &[
            state_condition(&env, &state, 1, &fulfilled),
            state_condition(&env, &state, 2, &fulfilled),
            timeout_condition(9_000),
        ],
    );
    let ready_id = client.register_auto_refund_trigger(&merchant, &7u64, &ready, &2_500u32);
    assert!(client.evaluate_auto_refund(&ready_id));
    assert_eq!(client.get_refund(&1u64).status, RefundStatus::Processed);
    assert!(!client.get_auto_refund_trigger(&ready_id).active);
}

/// Benchmark: resource consumption of evaluating a maximally complex trigger,
/// and the savings from short-circuiting on the first unmet condition.
#[test]
fn test_benchmark_complex_trigger_resource_consumption() {
    let env = Env::default();
    env.ledger().set_timestamp(10_000);
    let (client, admin) = setup(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token = Address::generate(&env);
    let payment_contract =
        install_mock_payment_contract(&env, sample_payment(&env, &merchant, &customer, &token));
    client.set_payment_contract_address(&admin, &payment_contract);

    let fulfilled = Bytes::from_slice(&env, b"fulfilled");
    let leaves = (MAX_TRIGGER_CONDITIONS - 1) as u8;
    let state = install_state_contract(&env, leaves, &fulfilled);
    let state_leaves: std::vec::Vec<AutoRefundCondition> = (1..=leaves)
        .map(|seed| state_condition(&env, &state, seed, &fulfilled))
        .collect();

    // Unmet condition last: every cross-contract check runs before failing.
    let mut late = state_leaves.clone();
    late.push(timeout_condition(20_000));
    // Unmet condition first: evaluation exits before any cross-contract call.
    let mut early = std::vec![timeout_condition(20_001)];
    early.extend(state_leaves.iter().cloned());

    let late_id =
        client.register_auto_refund_trigger(&merchant, &7u64, &all_of(&env, &late), &2_500u32);
    let early_id =
        client.register_auto_refund_trigger(&merchant, &7u64, &all_of(&env, &early), &2_500u32);

    let measure = |trigger_id: u64| -> (u64, u64) {
        env.cost_estimate().budget().reset_unlimited();
        assert!(!client.evaluate_auto_refund(&trigger_id));
        let budget = env.cost_estimate().budget();
        (budget.cpu_instruction_cost(), budget.memory_bytes_cost())
    };
    let (late_cpu, late_mem) = measure(late_id);
    let (early_cpu, early_mem) = measure(early_id);

    std::println!(
        "complex trigger ({} conditions): full eval cpu={} mem={}, early exit cpu={} mem={}",
        MAX_TRIGGER_CONDITIONS,
        late_cpu,
        late_mem,
        early_cpu,
        early_mem
    );

    // Soroban per-transaction limits: 100M CPU instructions, 40 MiB memory.
    // A max-size trigger must stay well within them.
    assert!(late_cpu < 100_000_000 / 4);
    assert!(late_mem < 40 * 1024 * 1024 / 4);
    // Short-circuiting skips the cross-contract calls entirely.
    assert!(early_cpu < late_cpu);
    assert!(early_mem < late_mem);
}
