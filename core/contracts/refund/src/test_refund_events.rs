#![cfg(test)]

// Issue #72: the refund request, approval, denial and appeal flows publish
// structured Soroban contract events carrying the customer, merchant, refund
// id and the canonical reason code, so customer/merchant dashboards can follow
// the refund lifecycle in real time without re-reading contract state.

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    IntoVal as _, Map, TryFromVal as _, Val,
};

/// The first topic of a `#[contractevent]` is the snake_case event name.
fn is_named(env: &Env, topics: &soroban_sdk::Vec<Val>, name: &Symbol) -> bool {
    Symbol::try_from_val(env, &topics.first().unwrap()).unwrap() == *name
}

/// The `data` payload of a `#[contractevent]` is a map keyed by field name.
fn data_map(env: &Env, data: &Val) -> Map<Val, Val> {
    Map::try_from_val(env, data).unwrap()
}

/// Decodes a single field of an event payload.
fn field<T: TryFromVal<Env, Val>>(env: &Env, data: &Val, name: &str) -> T {
    let value = data_map(env, data)
        .get(Symbol::new(env, name).into_val(env))
        .unwrap_or_else(|| panic!("event payload has no {} field", name));
    T::try_from_val(env, &value).unwrap()
}

/// Number of fields carried by an event payload.
fn field_count(env: &Env, data: &Val) -> u32 {
    data_map(env, data).len()
}

/// Number of events published by `contract_id` under the event name `name`.
fn count_events(env: &Env, contract_id: &Address, name: &str) -> u32 {
    let name = Symbol::new(env, name);
    let mut count = 0;
    let all = env.events().all();
    for i in 0..all.len() {
        let (source, topics, _data) = all.get(i).unwrap();
        if source == *contract_id && is_named(env, &topics, &name) {
            count += 1;
        }
    }
    count
}

/// Total number of events published by `contract_id`.
fn count_all_events(env: &Env, contract_id: &Address) -> u32 {
    let mut count = 0;
    let all = env.events().all();
    for i in 0..all.len() {
        if all.get(i).unwrap().0 == *contract_id {
            count += 1;
        }
    }
    count
}

/// `data` of the last event published by `contract_id` under `name`.
fn last_event(env: &Env, contract_id: &Address, name: &str) -> Val {
    let symbol = Symbol::new(env, name);
    let all = env.events().all();
    let mut found: Option<Val> = None;
    for i in 0..all.len() {
        let (source, topics, data) = all.get(i).unwrap();
        if source == *contract_id && is_named(env, &topics, &symbol) {
            found = Some(data);
        }
    }
    found.unwrap_or_else(|| panic!("no {:?} event was published", symbol))
}

struct Fixture {
    env: Env,
    contract_id: Address,
    client: RefundContractClient<'static>,
    admin: Address,
    merchant: Address,
    customer: Address,
    token: Address,
    payment_id: u64,
    amount: i128,
    reason: String,
    reason_code: RefundReasonCode,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    Fixture {
        admin: Address::generate(&env),
        merchant: Address::generate(&env),
        customer: Address::generate(&env),
        token: Address::generate(&env),
        payment_id: 1,
        amount: 1_000i128,
        reason: String::from_str(&env, "Damaged on arrival"),
        reason_code: RefundReasonCode::ProductDefect,
        env,
        contract_id,
        client,
    }
}

impl Fixture {
    fn request(&self) -> u64 {
        self.client.request_refund(
            &self.merchant,
            &self.payment_id,
            &self.customer,
            &self.amount,
            &self.amount,
            &self.token,
            &self.reason,
            &self.reason_code,
            &self.env.ledger().timestamp(),
        )
    }

    fn advance(&self, seconds: u64) {
        self.env.ledger().with_mut(|li| li.timestamp += seconds);
    }
}

#[test]
fn test_request_refund_emits_refund_requested_event() {
    let f = setup();
    let requested_at = f.env.ledger().timestamp();

    let refund_id = f.request();

    // A single request publishes exactly one event.
    assert_eq!(count_all_events(&f.env, &f.contract_id), 1);

    let data = last_event(&f.env, &f.contract_id, "refund_requested_event");
    assert_eq!(field::<u64>(&f.env, &data, "refund_id"), refund_id);
    assert_eq!(field::<u64>(&f.env, &data, "payment_id"), f.payment_id);
    assert_eq!(field::<Address>(&f.env, &data, "customer"), f.customer);
    assert_eq!(field::<Address>(&f.env, &data, "merchant"), f.merchant);
    assert_eq!(field::<i128>(&f.env, &data, "amount"), f.amount);
    assert_eq!(field::<Address>(&f.env, &data, "token"), f.token);
    assert_eq!(
        field::<RefundReasonCode>(&f.env, &data, "reason_code"),
        f.reason_code
    );
    assert_eq!(field::<u64>(&f.env, &data, "requested_at"), requested_at);
    assert_eq!(field_count(&f.env, &data), 8);
}

#[test]
fn test_approve_refund_emits_refund_approved_event() {
    let f = setup();
    let refund_id = f.request();

    f.advance(60);
    let approved_at = f.env.ledger().timestamp();
    f.client.approve_refund(&f.admin, &refund_id);

    assert_eq!(
        count_events(&f.env, &f.contract_id, "refund_approved_event"),
        1
    );

    let data = last_event(&f.env, &f.contract_id, "refund_approved_event");
    assert_eq!(field::<u64>(&f.env, &data, "refund_id"), refund_id);
    assert_eq!(field::<u64>(&f.env, &data, "payment_id"), f.payment_id);
    assert_eq!(field::<Address>(&f.env, &data, "customer"), f.customer);
    assert_eq!(field::<Address>(&f.env, &data, "merchant"), f.merchant);
    assert_eq!(field::<i128>(&f.env, &data, "amount"), f.amount);
    assert_eq!(
        field::<RefundReasonCode>(&f.env, &data, "reason_code"),
        f.reason_code
    );
    assert_eq!(field::<Address>(&f.env, &data, "approved_by"), f.admin);
    assert_eq!(field::<u64>(&f.env, &data, "approved_at"), approved_at);
    assert_eq!(field_count(&f.env, &data), 8);
}

#[test]
fn test_reject_refund_emits_refund_denied_event() {
    let f = setup();
    let refund_id = f.request();
    let denial_reason = String::from_str(&f.env, "Insufficient evidence");

    f.advance(120);
    let rejected_at = f.env.ledger().timestamp();
    f.client
        .reject_refund(&f.admin, &refund_id, &denial_reason);

    let data = last_event(&f.env, &f.contract_id, "refund_denied_event");
    assert_eq!(field::<u64>(&f.env, &data, "refund_id"), refund_id);
    assert_eq!(field::<Address>(&f.env, &data, "customer"), f.customer);
    assert_eq!(field::<Address>(&f.env, &data, "merchant"), f.merchant);
    assert_eq!(field::<i128>(&f.env, &data, "amount"), f.amount);
    assert_eq!(
        field::<RefundReasonCode>(&f.env, &data, "reason_code"),
        f.reason_code
    );
    assert_eq!(field::<Address>(&f.env, &data, "rejected_by"), f.admin);
    assert_eq!(field::<u64>(&f.env, &data, "rejected_at"), rejected_at);
    assert_eq!(
        field::<String>(&f.env, &data, "rejection_reason"),
        denial_reason
    );
    assert_eq!(field_count(&f.env, &data), 8);
}

#[test]
fn test_file_appeal_emits_appeal_filed_event() {
    let f = setup();
    let refund_id = f.request();
    f.client
        .reject_refund(&f.admin, &refund_id, &String::from_str(&f.env, "Denied"));

    f.advance(60);
    let filed_at = f.env.ledger().timestamp();
    let appeal_id = f.client.file_appeal(
        &f.customer,
        &refund_id,
        &String::from_str(&f.env, "Item was indeed damaged"),
    );

    let data = last_event(&f.env, &f.contract_id, "appeal_filed_event");
    assert_eq!(field::<u64>(&f.env, &data, "appeal_id"), appeal_id);
    assert_eq!(field::<u64>(&f.env, &data, "refund_id"), refund_id);
    assert_eq!(field::<Address>(&f.env, &data, "customer"), f.customer);
    assert_eq!(field::<Address>(&f.env, &data, "merchant"), f.merchant);
    assert_eq!(field::<Address>(&f.env, &data, "appellant"), f.customer);
    assert_eq!(
        field::<RefundReasonCode>(&f.env, &data, "reason_code"),
        f.reason_code
    );
    assert_eq!(field::<u64>(&f.env, &data, "filed_at"), filed_at);
    assert_eq!(field_count(&f.env, &data), 7);
}

#[test]
fn test_finalize_denial_emits_refund_denied_event() {
    let f = setup();
    let refund_id = f.request();
    f.client
        .reject_refund(&f.admin, &refund_id, &String::from_str(&f.env, "Denied"));

    // Past the appeal window (7 days by default), so the denial becomes final.
    f.advance(604_800 + 3_600);
    let rejected_at = f.env.ledger().timestamp();
    f.client.finalize_denial(&refund_id);

    let data = last_event(&f.env, &f.contract_id, "refund_denied_event");
    assert_eq!(field::<u64>(&f.env, &data, "refund_id"), refund_id);
    assert_eq!(field::<Address>(&f.env, &data, "customer"), f.customer);
    assert_eq!(field::<Address>(&f.env, &data, "merchant"), f.merchant);
    assert_eq!(
        field::<Address>(&f.env, &data, "rejected_by"),
        f.admin,
        "the denial keeps the reviewer recorded on the refund"
    );
    assert_eq!(field::<u64>(&f.env, &data, "rejected_at"), rejected_at);
    assert_eq!(
        field::<String>(&f.env, &data, "rejection_reason"),
        String::from_str(&f.env, "appeal window expired")
    );
    assert_eq!(
        field::<RefundReasonCode>(&f.env, &data, "reason_code"),
        f.reason_code
    );
}

/// Every step of the request -> deny -> appeal lifecycle reports the same
/// customer, merchant, refund id and reason code, so a dashboard can key its
/// refund view off the event stream alone.
///
/// Note: the test environment only exposes the events of the most recent
/// contract call, so each step is inspected right after it runs.
#[test]
fn test_lifecycle_events_share_customer_merchant_refund_id_and_reason_code() {
    let f = setup();
    let refund_id = f.request();

    let requested = last_event(&f.env, &f.contract_id, "refund_requested_event");
    f.client
        .reject_refund(&f.admin, &refund_id, &String::from_str(&f.env, "Denied"));
    let denied = last_event(&f.env, &f.contract_id, "refund_denied_event");
    let appeal_id = f.client.file_appeal(
        &f.customer,
        &refund_id,
        &String::from_str(&f.env, "Item was indeed damaged"),
    );
    let appeal = last_event(&f.env, &f.contract_id, "appeal_filed_event");

    assert_eq!(refund_id, 1);
    assert_eq!(field::<u64>(&f.env, &appeal, "appeal_id"), appeal_id);

    for (name, data) in [
        ("refund_requested_event", &requested),
        ("refund_denied_event", &denied),
        ("appeal_filed_event", &appeal),
    ] {
        assert_eq!(field::<u64>(&f.env, data, "refund_id"), refund_id, "{}", name);
        assert_eq!(field::<Address>(&f.env, data, "customer"), f.customer, "{}", name);
        assert_eq!(field::<Address>(&f.env, data, "merchant"), f.merchant, "{}", name);
        assert_eq!(
            field::<RefundReasonCode>(&f.env, data, "reason_code"),
            f.reason_code,
            "{}",
            name
        );
    }
}

/// A denied request publishes nothing, so a failed flow can never leave a
/// dangling dashboard entry.
#[test]
fn test_failed_flow_publishes_no_event() {
    let f = setup();
    let result = f.client.try_request_refund(
        &f.merchant,
        &f.payment_id,
        &f.customer,
        &0i128,
        &f.amount,
        &f.token,
        &f.reason,
        &f.reason_code,
        &f.env.ledger().timestamp(),
    );
    assert!(result.is_err());
    assert_eq!(count_all_events(&f.env, &f.contract_id), 0);
}
