#![cfg(test)]

// Issue #389: two-step admin rotation for the refund contract.

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::Env;

#[test]
fn test_propose_and_accept_admin_rotates_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    client.initialize(&admin);

    assert_eq!(client.get_pending_admin(), None);

    client.propose_admin(&admin, &new_admin);
    assert_eq!(client.get_pending_admin(), Some(new_admin.clone()));

    assert_eq!(
        client.try_accept_admin(&new_admin),
        Err(Ok(Error::Ext(ExtError::AdminRotationDelayNotElapsed)))
    );
    env.ledger().set_timestamp(ADMIN_ROTATION_DELAY_SECS);

    client.accept_admin(&new_admin);
    assert_eq!(client.get_pending_admin(), None);

    // The rotated-in admin can now perform an admin-gated action.
    let target_version = client.get_schema_version() + 1;
    client.migrate_schema(&new_admin, &target_version);
    assert_eq!(client.get_schema_version(), target_version);
}

#[test]
fn test_propose_admin_rejects_non_admin_caller() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let not_admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    client.initialize(&admin);

    let result = client.try_propose_admin(&not_admin, &new_admin);
    assert_eq!(result, Err(Ok(Error::Core(CoreError::Unauthorized))));
}

#[test]
fn test_accept_admin_rejects_wrong_caller() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let imposter = Address::generate(&env);
    client.initialize(&admin);

    client.propose_admin(&admin, &new_admin);

    let result = client.try_accept_admin(&imposter);
    assert_eq!(result, Err(Ok(Error::Ext(ExtError::NotPendingAdmin))));

    // Old admin still retains control since rotation never completed.
    let target_version = client.get_schema_version() + 1;
    client.migrate_schema(&admin, &target_version);
    assert_eq!(client.get_schema_version(), target_version);
}

#[test]
fn test_accept_admin_without_proposal_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let stranger = Address::generate(&env);
    client.initialize(&admin);

    let result = client.try_accept_admin(&stranger);
    assert_eq!(result, Err(Ok(Error::Ext(ExtError::NoPendingAdmin))));
}

#[test]
fn test_compromised_admin_key_cannot_be_used_after_rotation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    let compromised_admin = Address::generate(&env);
    let safe_admin = Address::generate(&env);
    client.initialize(&compromised_admin);

    client.propose_admin(&compromised_admin, &safe_admin);
    env.ledger().set_timestamp(ADMIN_ROTATION_DELAY_SECS);
    client.accept_admin(&safe_admin);

    // The old (compromised) admin address can no longer perform admin actions.
    let result = client.try_migrate_schema(&compromised_admin, &2);
    assert_eq!(result, Err(Ok(Error::Core(CoreError::Unauthorized))));
}

fn setup_rotation(env: &Env) -> (RefundContractClient<'_>, Address) {
    env.mock_all_auths();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

#[test]
fn test_pending_admin_has_no_power_until_accepted() {
    let env = Env::default();
    let (client, admin) = setup_rotation(&env);
    let proposed = Address::generate(&env);

    client.propose_admin(&admin, &proposed);
    assert_eq!(client.get_pending_admin(), Some(proposed.clone()));

    // Proposing alone transfers nothing: the old admin still rules...
    client.set_payment_rejection_cooldown(&admin, &3_600);
    // ...and the proposed address is not an admin yet.
    assert_eq!(
        client.try_set_payment_rejection_cooldown(&proposed, &1),
        Err(Ok(Error::Core(CoreError::Unauthorized)))
    );

    env.ledger().set_timestamp(ADMIN_ROTATION_DELAY_SECS);
    client.accept_admin(&proposed);
    client.set_payment_rejection_cooldown(&proposed, &7_200);
    assert_eq!(
        client.try_set_payment_rejection_cooldown(&admin, &1),
        Err(Ok(Error::Core(CoreError::Unauthorized)))
    );
}

#[test]
fn test_cancel_admin_proposal() {
    let env = Env::default();
    let (client, admin) = setup_rotation(&env);
    let typo = Address::generate(&env);

    client.propose_admin(&admin, &typo);
    client.cancel_admin_proposal(&admin);
    assert_eq!(client.get_pending_admin(), None);

    // The cancelled address can no longer claim the contract...
    assert_eq!(
        client.try_accept_admin(&typo),
        Err(Ok(Error::Ext(ExtError::NoPendingAdmin)))
    );
    // ...and the original admin keeps control.
    client.set_payment_rejection_cooldown(&admin, &3_600);

    // Nothing left to cancel.
    assert_eq!(
        client.try_cancel_admin_proposal(&admin),
        Err(Ok(Error::Ext(ExtError::NoPendingAdmin)))
    );
}

#[test]
fn test_cancel_admin_proposal_requires_current_admin() {
    let env = Env::default();
    let (client, admin) = setup_rotation(&env);
    let proposed = Address::generate(&env);
    client.propose_admin(&admin, &proposed);

    // Neither a stranger nor the proposed admin can cancel the proposal.
    let stranger = Address::generate(&env);
    assert_eq!(
        client.try_cancel_admin_proposal(&stranger),
        Err(Ok(Error::Core(CoreError::Unauthorized)))
    );
    assert_eq!(
        client.try_cancel_admin_proposal(&proposed),
        Err(Ok(Error::Core(CoreError::Unauthorized)))
    );
    assert_eq!(client.get_pending_admin(), Some(proposed));
}

#[test]
fn test_reproposing_replaces_mistyped_admin() {
    let env = Env::default();
    let (client, admin) = setup_rotation(&env);
    let typo = Address::generate(&env);
    let intended = Address::generate(&env);

    client.propose_admin(&admin, &typo);
    client.propose_admin(&admin, &intended);
    assert_eq!(client.get_pending_admin(), Some(intended.clone()));

    assert_eq!(
        client.try_accept_admin(&typo),
        Err(Ok(Error::Ext(ExtError::NotPendingAdmin)))
    );
    env.ledger().set_timestamp(ADMIN_ROTATION_DELAY_SECS);
    client.accept_admin(&intended);
    client.set_payment_rejection_cooldown(&intended, &60);
}
