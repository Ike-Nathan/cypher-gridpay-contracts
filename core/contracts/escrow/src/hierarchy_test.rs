#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, testutils::Ledger, token, Address, BytesN, Env};

fn setup_token(env: &Env, admin: &Address, customer: &Address) -> Address {
    let token_addr = env.register_stellar_asset_contract(admin.clone());
    let token_admin = token::StellarAssetClient::new(env, &token_addr);
    token_admin.mint(customer, &1_000_000);
    token_addr
}

#[test]
fn test_two_level_hierarchy_success() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = setup_token(&env, &admin, &customer);

    client.initialize(&admin);

    // Create root parent escrow (level 0)
    let parent_id = client.create_escrow(
        &customer, &merchant, &1000_i128, &token, &2000_u64, &0_u64, &0_u64, &false,
    );

    // Create child escrow (level 1)
    let child_id =
        client.create_child_escrow(&admin, &parent_id, &500_i128, &token, &customer, &merchant);

    // Verify parent release is blocked since child is unresolved (Locked)
    let release_res = client.try_release_escrow(&admin, &parent_id, &false);
    assert_eq!(
        release_res,
        Err(Ok(Error::Escrow(EscrowError::ChildrenNotResolved)))
    );

    // Advance ledger timestamp so child can be released without early release check
    env.ledger().set_timestamp(2000);

    // Resolve child (release it)
    client.release_escrow(&admin, &child_id, &false);

    // Now parent should be able to release successfully (once the ledger timestamp passes the release timestamp)
    env.ledger().set_timestamp(2001);
    let release_res = client.try_release_escrow(&admin, &parent_id, &false);
    assert!(release_res.is_ok());
}

#[test]
fn test_depth_limit_enforced() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = setup_token(&env, &admin, &customer);

    client.initialize(&admin);

    // Root (level 0)
    let root_id = client.create_escrow(
        &customer, &merchant, &1000_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );

    // Child (level 1)
    let lvl1_id =
        client.create_child_escrow(&admin, &root_id, &500_i128, &token, &customer, &merchant);

    // Grandchild (level 2)
    let lvl2_id =
        client.create_child_escrow(&admin, &lvl1_id, &250_i128, &token, &customer, &merchant);

    // Great-grandchild (level 3)
    let lvl3_id =
        client.create_child_escrow(&admin, &lvl2_id, &100_i128, &token, &customer, &merchant);

    // Creating under level 3 (would be level 4) should fail with MaxHierarchyDepth
    let lvl4_res =
        client.try_create_child_escrow(&admin, &lvl3_id, &50_i128, &token, &customer, &merchant);
    assert_eq!(
        lvl4_res,
        Err(Ok(Error::Escrow(EscrowError::MaxHierarchyDepth)))
    );
}

#[test]
fn test_get_escrow_hierarchy() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = setup_token(&env, &admin, &customer);

    client.initialize(&admin);

    let root_id = client.create_escrow(
        &customer, &merchant, &1000_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );

    let child_1 =
        client.create_child_escrow(&admin, &root_id, &500_i128, &token, &customer, &merchant);

    let child_2 =
        client.create_child_escrow(&admin, &root_id, &300_i128, &token, &customer, &merchant);

    let grandchild_1 =
        client.create_child_escrow(&admin, &child_1, &100_i128, &token, &customer, &merchant);

    let hierarchy = client.get_escrow_hierarchy(&root_id);
    assert_eq!(hierarchy.len(), 4);

    // Root node checks
    let root_node = hierarchy.get(0).unwrap();
    assert_eq!(root_node.escrow_id, root_id);
    assert_eq!(root_node.parent_id, None);
    assert_eq!(root_node.depth, 0);
    assert_eq!(root_node.children.len(), 2);
    assert_eq!(root_node.children.get(0).unwrap(), child_1);
    assert_eq!(root_node.children.get(1).unwrap(), child_2);

    // Child 1 node checks
    let child1_node = hierarchy.get(1).unwrap();
    assert_eq!(child1_node.escrow_id, child_1);
    assert_eq!(child1_node.parent_id, Some(root_id));
    assert_eq!(child1_node.depth, 1);
    assert_eq!(child1_node.children.len(), 1);
    assert_eq!(child1_node.children.get(0).unwrap(), grandchild_1);
}

#[test]
fn test_create_child_escrow_validation() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let unauthorized_caller = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = setup_token(&env, &admin, &customer);

    client.initialize(&admin);

    // Non-admin call fails with NotAnAdmin
    let res = client.try_create_child_escrow(
        &unauthorized_caller,
        &1_u64,
        &100_i128,
        &token,
        &customer,
        &merchant,
    );
    assert_eq!(res, Err(Ok(Error::Basic(BasicError::NotAnAdmin))));

    // Non-existent parent fails with ParentEscrowNotFound
    let res2 =
        client.try_create_child_escrow(&admin, &999_u64, &100_i128, &token, &customer, &merchant);
    assert_eq!(
        res2,
        Err(Ok(Error::Escrow(EscrowError::ParentEscrowNotFound)))
    );
}

/// A parent-level liquidation must never touch the isolated ledgers of its
/// child sub-account milestones: a completed milestone keeps its funds and can
/// never be liquidated twice, while a pending milestone stays reserved and
/// releasable after the parent has been settled.
#[test]
fn test_parent_liquidation_isolates_child_sub_accounts() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = setup_token(&env, &admin, &customer);

    client.initialize(&admin);
    env.ledger().set_timestamp(1000);

    // Parent escrow of 1000, funded into the contract by the customer.
    let parent_id = client.create_escrow(
        &customer, &merchant, &1000_i128, &token, &2000_u64, &0_u64, &0_u64, &false,
    );

    // Child milestone A: completed (released) for 400.
    let completed_id = client.create_sub_account(
        &merchant,
        &parent_id,
        &BytesN::from_array(&env, &[7u8; 32]),
        &400,
        &None,
    );
    client.release_sub_account(&admin, &parent_id, &completed_id);

    // Child milestone B: still pending for 300.
    let pending_id = client.create_sub_account(
        &merchant,
        &parent_id,
        &BytesN::from_array(&env, &[8u8; 32]),
        &300,
        &None,
    );

    let token_client = token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&merchant), 400);
    assert_eq!(token_client.balance(&contract_id), 600);

    // Partial parent dispute: only the parent's unallocated remainder
    // (1000 - 400 - 300 = 300) may be liquidated, in favour of the customer.
    client.dispute_escrow(&customer, &parent_id);
    client.resolve_dispute(&admin, &parent_id, &false);

    assert_eq!(token_client.balance(&customer), 1_000_000 - 1000 + 300);
    assert_eq!(token_client.balance(&merchant), 400);
    assert_eq!(token_client.balance(&contract_id), 300);

    // The completed child ledger is intact after the parent liquidation.
    let completed = client.get_sub_account(&parent_id, &completed_id).unwrap();
    assert!(completed.released);
    assert_eq!(completed.amount, 400);

    // A completed child sub-account can never be liquidated again.
    assert_eq!(
        client.try_release_sub_account(&admin, &parent_id, &completed_id),
        Err(Ok(Error::Escrow(EscrowError::SubAccountAlreadyReleased)))
    );

    // The pending child milestone stays isolated and is still releasable.
    client.release_sub_account(&admin, &parent_id, &pending_id);
    assert_eq!(token_client.balance(&merchant), 700);
    assert_eq!(token_client.balance(&contract_id), 0);
}
