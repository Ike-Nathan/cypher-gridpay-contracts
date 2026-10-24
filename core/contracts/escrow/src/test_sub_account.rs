#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, testutils::Ledger, token, Address, BytesN, Env};

fn setup(env: &Env) -> (EscrowContractClient<'_>, Address, Address, Address, Address) {
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let customer = Address::generate(env);
    let merchant = Address::generate(env);
    let token = Address::generate(env);
    env.mock_all_auths();
    (client, admin, customer, merchant, token)
}

fn make_escrow(
    client: &EscrowContractClient,
    customer: &Address,
    merchant: &Address,
    token: &Address,
    amount: i128,
) -> u64 {
    client.create_escrow(customer, merchant, &amount, token, &9999999_u64, &0_u64, &0_u64, &false)
}

fn label(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[1u8; 32])
}

#[test]
fn test_create_sub_account() {
    let env = Env::default();
    let (client, _, customer, merchant, token) = setup(&env);

    let escrow_id = make_escrow(&client, &customer, &merchant, &token, 1000);

    let sub_id = client.create_sub_account(&merchant, &escrow_id, &label(&env), &400, &None);
    assert_eq!(sub_id, 1);

    let sub = client.get_sub_account(&escrow_id, &sub_id).unwrap();
    assert_eq!(sub.amount, 400);
    assert!(!sub.released);
}

#[test]
fn test_funding_sub_account() {
    let env = Env::default();
    let (client, _, customer, merchant, token) = setup(&env);

    let escrow_id = make_escrow(&client, &customer, &merchant, &token, 1000);
    let sub_id = client.create_sub_account(&merchant, &escrow_id, &label(&env), &200, &None);

    client.fund_sub_account(&merchant, &escrow_id, &sub_id, &300);

    let sub = client.get_sub_account(&escrow_id, &sub_id).unwrap();
    assert_eq!(sub.amount, 500);
}

#[test]
fn test_release_sub_account() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);

    let token_id = env.register(soroban_sdk::token::StellarAssetContract, (&admin,));
    let token_admin = token::StellarAssetClient::new(&env, &token_id);
    token_admin.mint(&customer, &1000);

    let escrow_id = make_escrow(&client, &customer, &merchant, &token_id, 1000);
    let sub_id = client.create_sub_account(&merchant, &escrow_id, &label(&env), &500, &None);

    client.release_sub_account(&admin, &escrow_id, &sub_id);

    let sub = client.get_sub_account(&escrow_id, &sub_id).unwrap();
    assert!(sub.released);
}

#[test]
fn test_double_release_rejected() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);

    let token_id = env.register(soroban_sdk::token::StellarAssetContract, (&admin,));
    let token_admin = token::StellarAssetClient::new(&env, &token_id);
    token_admin.mint(&customer, &1000);

    let escrow_id = make_escrow(&client, &customer, &merchant, &token_id, 1000);
    let sub_id = client.create_sub_account(&merchant, &escrow_id, &label(&env), &500, &None);

    client.release_sub_account(&admin, &escrow_id, &sub_id);

    let result = client.try_release_sub_account(&admin, &escrow_id, &sub_id);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::SubAccountAlreadyReleased))));
}

#[test]
fn test_funding_exceeds_escrow_rejected() {
    let env = Env::default();
    let (client, _, customer, merchant, token) = setup(&env);

    let escrow_id = make_escrow(&client, &customer, &merchant, &token, 1000);

    // Try to allocate more than the escrow holds
    let result = client.try_create_sub_account(&merchant, &escrow_id, &label(&env), &1001, &None);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::SubAccountFundingExceedsEscrow))));
}

#[test]
fn test_parent_guard_blocks_release_with_unreleased_sub_accounts() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);
    env.ledger().set_timestamp(1000);

    let escrow_id = make_escrow(&client, &customer, &merchant, &token, 1000);
    client.create_sub_account(&merchant, &escrow_id, &label(&env), &500, &None);

    // Parent release should fail because sub-account is not yet released
    let result = client.try_release_escrow(&admin, &escrow_id, &true);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::InvalidStatus))));
}

#[test]
fn test_list_sub_accounts() {
    let env = Env::default();
    let (client, _, customer, merchant, token) = setup(&env);

    let escrow_id = make_escrow(&client, &customer, &merchant, &token, 1000);
    client.create_sub_account(&merchant, &escrow_id, &label(&env), &200, &None);
    client.create_sub_account(
        &merchant,
        &escrow_id,
        &BytesN::from_array(&env, &[2u8; 32]),
        &300,
        &None,
    );

    let subs = client.list_sub_accounts(&escrow_id);
    assert_eq!(subs.len(), 2);
    assert_eq!(subs.get(0).unwrap().amount, 200);
    assert_eq!(subs.get(1).unwrap().amount, 300);
}

#[test]
fn test_sub_account_fee_bps_override() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup(&env);

    client.set_escrow_fee_config(
        &admin,
        &EscrowFeeConfig {
            fee_bps: 500,
            fee_recipient: admin.clone(),
            enabled: true,
        },
    );

    let escrow_id = make_escrow(&client, &customer, &merchant, &token, 1000);

    let fee_free_id = client.create_sub_account(
        &merchant,
        &escrow_id,
        &label(&env),
        &200,
        &Some(0),
    );
    let premium_id = client.create_sub_account(
        &merchant,
        &escrow_id,
        &BytesN::from_array(&env, &[2u8; 32]),
        &300,
        &Some(1000),
    );
    let inherited_id = client.create_sub_account(
        &merchant,
        &escrow_id,
        &BytesN::from_array(&env, &[3u8; 32]),
        &100,
        &None,
    );

    assert_eq!(
        client
            .get_sub_account(&escrow_id, &fee_free_id)
            .unwrap()
            .fee_bps_override,
        Some(0)
    );
    assert_eq!(
        client
            .get_sub_account(&escrow_id, &premium_id)
            .unwrap()
            .fee_bps_override,
        Some(1000)
    );
    assert!(
        client
            .get_sub_account(&escrow_id, &inherited_id)
            .unwrap()
            .fee_bps_override
            .is_none()
    );

    client.set_sub_account_fee_override(&merchant, &escrow_id, &inherited_id, &Some(250));
    assert_eq!(
        client
            .get_sub_account(&escrow_id, &inherited_id)
            .unwrap()
            .fee_bps_override,
        Some(250)
    );
}

// ── Hierarchy budget rebalancing and contingency draw (issue #216) ───────────

use soroban_sdk::testutils::Address as _;

fn setup_with_token(
    env: &Env,
) -> (
    EscrowContractClient<'_>,
    Address, // admin
    Address, // customer (parent payer)
    Address, // merchant
    Address, // token
) {
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let customer = Address::generate(env);
    let merchant = Address::generate(env);
    let token = env.register(soroban_sdk::token::StellarAssetContract, (&admin,));
    let token_mint = token::StellarAssetClient::new(env, &token);
    token_mint.mint(&customer, &1_000_000);
    env.mock_all_auths();
    client.initialize(&admin);
    (client, admin, customer, merchant, token)
}

fn make_hierarchy(
    env: &Env,
    client: &EscrowContractClient,
    admin: &Address,
    customer: &Address,
    merchant: &Address,
    token: &Address,
) -> (u64, u64, u64) {
    // Parent at t=1000, amount=1000
    env.ledger().set_timestamp(1_000);
    let parent_id = client.create_escrow(
        customer, merchant, &1_000_i128, token, &5_000_u64, &0_u64, &0_u64, &false,
    );
    // Two children of parent, each 300
    let child_a = client.create_child_escrow(admin, &parent_id, &300_i128, token, customer, merchant);
    let child_b = client.create_child_escrow(admin, &parent_id, &200_i128, token, customer, merchant);
    (parent_id, child_a, child_b)
}

#[test]
fn test_rebalance_sibling_sub_escrows_success() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup_with_token(&env);
    let (parent_id, child_a, child_b) = make_hierarchy(&env, &client, &admin, &customer, &merchant, &token);

    // Before: child_a=300, child_b=200
    let a_before = client.get_escrow(&child_a).amount;
    let b_before = client.get_escrow(&child_b).amount;
    assert_eq!(a_before, 300);
    assert_eq!(b_before, 200);

    // Rebalance 100 from child_a to child_b
    client.rebalance_sibling_sub_escrows(&parent_id, &child_a, &child_b, &100_i128);

    let a_after = client.get_escrow(&child_a).amount;
    let b_after = client.get_escrow(&child_b).amount;
    assert_eq!(a_after, 200, "source child should lose 100");
    assert_eq!(b_after, 300, "dest child should gain 100");
}

#[test]
fn test_rebalance_disputed_source_child_rejected() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup_with_token(&env);
    let (parent_id, child_a, child_b) = make_hierarchy(&env, &client, &admin, &customer, &merchant, &token);

    // Dispute child_a
    client.dispute_escrow(&customer, &child_a);

    let result = client.try_rebalance_sibling_sub_escrows(&parent_id, &child_a, &child_b, &50_i128);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::ChildDisputed))));
}

#[test]
fn test_rebalance_wrong_parent_rejected() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup_with_token(&env);
    let (_parent_id, child_a, _child_b) = make_hierarchy(&env, &client, &admin, &customer, &merchant, &token);

    // Create a separate parent + child
    let other_parent = client.create_escrow(
        &customer, &merchant, &500_i128, &token, &5_000_u64, &0_u64, &0_u64, &false,
    );
    let other_child = client.create_child_escrow(&admin, &other_parent, &100_i128, &token, &customer, &merchant);

    // Try to rebalance between children of different parents
    let result = client.try_rebalance_sibling_sub_escrows(&other_parent, &child_a, &other_child, &50_i128);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::NotAChildOfParent))));
}

#[test]
fn test_rebalance_zero_amount_rejected() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup_with_token(&env);
    let (parent_id, child_a, child_b) = make_hierarchy(&env, &client, &admin, &customer, &merchant, &token);

    let result = client.try_rebalance_sibling_sub_escrows(&parent_id, &child_a, &child_b, &0_i128);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::InvalidRebalanceAmount))));
}

#[test]
fn test_rebalance_exceeds_source_amount_rejected() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup_with_token(&env);
    let (parent_id, child_a, child_b) = make_hierarchy(&env, &client, &admin, &customer, &merchant, &token);

    // child_a has 300, try to rebalance 400
    let result = client.try_rebalance_sibling_sub_escrows(&parent_id, &child_a, &child_b, &400_i128);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::InvalidRebalanceAmount))));
}

#[test]
fn test_rebalance_terminal_child_rejected() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup_with_token(&env);
    let (parent_id, child_a, child_b) = make_hierarchy(&env, &client, &admin, &customer, &merchant, &token);

    // Release child_a (terminal state)
    env.ledger().set_timestamp(5_001);
    client.release_escrow(&admin, &child_a, &false);

    let result = client.try_rebalance_sibling_sub_escrows(&parent_id, &child_a, &child_b, &50_i128);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::InvalidStatus))));
}

#[test]
fn test_rebalance_preserves_total_locked() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup_with_token(&env);
    let (parent_id, child_a, child_b) = make_hierarchy(&env, &client, &admin, &customer, &merchant, &token);

    let total_before = client.get_escrow(&child_a).amount + client.get_escrow(&child_b).amount;

    client.rebalance_sibling_sub_escrows(&parent_id, &child_a, &child_b, &75_i128);

    let total_after = client.get_escrow(&child_a).amount + client.get_escrow(&child_b).amount;
    assert_eq!(total_before, total_after, "total locked must be preserved");
}

#[test]
fn test_draw_parent_contingency_success() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup_with_token(&env);
    let (parent_id, child_a, _child_b) = make_hierarchy(&env, &client, &admin, &customer, &merchant, &token);

    // Set contingency reserve of 100 and 5000 bps (50%) overdraft cap
    client.set_hierarchy_contingency(&parent_id, &customer, &100_i128, &5_000_u32);

    // child_a amount=300; overdraft cap = 300 * 5000/10000 = 150; draw 80 <= 150 ✓
    client.draw_parent_contingency(&parent_id, &child_a, &80_i128);

    let child_after = client.get_escrow(&child_a);
    assert_eq!(child_after.amount, 380, "child should gain 80 from contingency");

    // Verify reserve depleted
    let hierarchy = client.get_escrow_hierarchy(&parent_id);
    let parent_node = hierarchy.get(0).unwrap();
    assert_eq!(parent_node.contingency_reserve, 20, "reserve should be 100-80=20");
}

#[test]
fn test_draw_parent_contingency_overdraft_cap_exceeded() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup_with_token(&env);
    let (parent_id, child_a, _child_b) = make_hierarchy(&env, &client, &admin, &customer, &merchant, &token);

    // 5000 bps (50%); child_a=300; cap=150
    client.set_hierarchy_contingency(&parent_id, &customer, &200_i128, &5_000_u32);

    // Try to draw 160 > 150 cap
    let result = client.try_draw_parent_contingency(&parent_id, &child_a, &160_i128);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::OverdraftCapExceeded))));
}

#[test]
fn test_draw_parent_contingency_zero_bps_rejected() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup_with_token(&env);
    let (parent_id, child_a, _child_b) = make_hierarchy(&env, &client, &admin, &customer, &merchant, &token);

    // Set reserve but 0 bps (no overdraft permitted)
    client.set_hierarchy_contingency(&parent_id, &customer, &100_i128, &0_u32);

    let result = client.try_draw_parent_contingency(&parent_id, &child_a, &10_i128);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::OverdraftCapExceeded))));
}

#[test]
fn test_draw_parent_contingency_exceeds_reserve() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup_with_token(&env);
    let (parent_id, child_a, _child_b) = make_hierarchy(&env, &client, &admin, &customer, &merchant, &token);

    // Reserve=50, bps=10000 (100%, cap=300), but amount > reserve
    client.set_hierarchy_contingency(&parent_id, &customer, &50_i128, &10_000_u32);

    let result = client.try_draw_parent_contingency(&parent_id, &child_a, &60_i128);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::InvalidRebalanceAmount))));
}

#[test]
fn test_draw_contingency_wrong_child_rejected() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup_with_token(&env);
    let (parent_id, child_a, _child_b) = make_hierarchy(&env, &client, &admin, &customer, &merchant, &token);

    // Create a different parent's child
    let other_parent = client.create_escrow(
        &customer, &merchant, &500_i128, &token, &5_000_u64, &0_u64, &0_u64, &false,
    );
    let other_child = client.create_child_escrow(&admin, &other_parent, &100_i128, &token, &customer, &merchant);

    client.set_hierarchy_contingency(&parent_id, &customer, &100_i128, &5_000_u32);

    // other_child is not a child of parent_id
    let result = client.try_draw_parent_contingency(&parent_id, &other_child, &10_i128);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::NotAChildOfParent))));
}

#[test]
fn test_set_hierarchy_contingency_non_payer_rejected() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup_with_token(&env);
    let (parent_id, _child_a, _child_b) = make_hierarchy(&env, &client, &_admin, &customer, &merchant, &token);

    // Merchant is not the payer; should be rejected
    let result = client.try_set_hierarchy_contingency(&parent_id, &merchant, &100_i128, &5_000_u32);
    assert_eq!(result, Err(Ok(Error::Basic(BasicError::Unauthorized))));
}

#[test]
fn test_set_hierarchy_contingency_exceeds_parent_amount() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup_with_token(&env);
    let (parent_id, _child_a, _child_b) = make_hierarchy(&env, &client, &admin, &customer, &merchant, &token);

    // Parent amount is 1000, try to set reserve to 1001
    let result = client.try_set_hierarchy_contingency(&parent_id, &customer, &1_001_i128, &5_000_u32);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::InvalidRebalanceAmount))));
}

#[test]
fn test_set_hierarchy_contingency_invalid_bps() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup_with_token(&env);
    let (parent_id, _child_a, _child_b) = make_hierarchy(&env, &client, &admin, &customer, &merchant, &token);

    // 10_001 bps > 10_000
    let result = client.try_set_hierarchy_contingency(&parent_id, &customer, &50_i128, &10_001_u32);
    assert_eq!(result, Err(Ok(Error::Basic(BasicError::InvalidBps))));
}

#[test]
fn test_tree_invariant_preserved_after_rebalance() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup_with_token(&env);
    let (parent_id, child_a, child_b) = make_hierarchy(&env, &client, &admin, &customer, &merchant, &token);

    // Total hierarchy locked = parent(1000) + child_a(300) + child_b(200) = 1500
    // But parent amount includes child allocations conceptually; the invariant
    // we verify is that child amounts sum is preserved by rebalancing.
    let children_sum_before = client.get_escrow(&child_a).amount + client.get_escrow(&child_b).amount;

    // Multiple rebalances
    client.rebalance_sibling_sub_escrows(&parent_id, &child_a, &child_b, &50_i128);
    client.rebalance_sibling_sub_escrows(&parent_id, &child_b, &child_a, &25_i128);

    let children_sum_after = client.get_escrow(&child_a).amount + client.get_escrow(&child_b).amount;
    assert_eq!(children_sum_before, children_sum_after, "children sum must be invariant");
}

#[test]
fn test_draw_contingency_preserves_tree_sum() {
    let env = Env::default();
    let (client, admin, customer, merchant, token) = setup_with_token(&env);
    let (parent_id, child_a, child_b) = make_hierarchy(&env, &client, &admin, &customer, &merchant, &token);

    // Set contingency reserve = 100, bps = 10000 (100% cap)
    client.set_hierarchy_contingency(&parent_id, &customer, &100_i128, &10_000_u32);

    let hierarchy_before = client.get_escrow_hierarchy(&parent_id);
    let parent_node_before = hierarchy_before.get(0).unwrap();
    let children_sum_before = client.get_escrow(&child_a).amount + client.get_escrow(&child_b).amount;
    let total_before = parent_node_before.contingency_reserve + children_sum_before;

    // Draw 60 from contingency into child_a
    client.draw_parent_contingency(&parent_id, &child_a, &60_i128);

    let hierarchy_after = client.get_escrow_hierarchy(&parent_id);
    let parent_node_after = hierarchy_after.get(0).unwrap();
    let children_sum_after = client.get_escrow(&child_a).amount + client.get_escrow(&child_b).amount;
    let total_after = parent_node_after.contingency_reserve + children_sum_after;

    // Tree invariant: contingency_reserve + sum(children) is preserved.
    assert_eq!(total_before, total_after, "tree invariant must hold");
}
