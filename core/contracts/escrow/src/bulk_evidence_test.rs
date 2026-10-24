#![cfg(test)]

use crate::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Bytes, Env, Vec,
};

fn setup(env: &Env) -> (EscrowContractClient, Address, Address, Address, Address) {
    env.mock_all_auths();
    let id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(env, &id);
    let admin = Address::generate(env);
    client.initialize(&admin);

    let token_addr = env.register_stellar_asset_contract(admin.clone());
    let token_admin = token::StellarAssetClient::new(env, &token_addr);
    let customer = Address::generate(env);
    token_admin.mint(&customer, &10_000i128);
    token_admin.mint(&id, &10_000i128);

    (client, admin, customer, Address::generate(env), token_addr)
}

fn make_disputed_escrow(
    env: &Env,
    client: &EscrowContractClient,
    customer: &Address,
    merchant: &Address,
    token: &Address,
) -> u64 {
    let escrow_id = client.create_escrow(
        customer, merchant, &500i128, token, &9999u64, &0u64, &0u64, &false,
    );
    client.dispute_escrow(customer, &escrow_id);
    escrow_id
}

#[test]
fn test_batch_submission_stores_all_items() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let mut items: Vec<Bytes> = Vec::new(&env);
    items.push_back(Bytes::from_array(&env, &[1u8; 32]));
    items.push_back(Bytes::from_array(&env, &[2u8; 32]));
    items.push_back(Bytes::from_array(&env, &[3u8; 32]));

    let page_count = client.submit_evidence_batch(&customer, &escrow_id, &items);
    assert_eq!(page_count, 1u32);

    let page = client.get_evidence_page(&escrow_id, &0u32);
    assert_eq!(page.len(), 3u32);
}

#[test]
fn test_oversized_batch_rejected() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let mut items: Vec<Bytes> = Vec::new(&env);
    for _ in 0..11u32 {
        items.push_back(Bytes::from_array(&env, &[0u8; 32]));
    }

    let result = client.try_submit_evidence_batch(&customer, &escrow_id, &items);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::BatchTooLarge))));
}

#[test]
fn test_pagination_two_pages() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let mut batch1: Vec<Bytes> = Vec::new(&env);
    for _ in 0..10u32 {
        batch1.push_back(Bytes::from_array(&env, &[1u8; 32]));
    }
    client.submit_evidence_batch(&customer, &escrow_id, &batch1);

    let mut batch2: Vec<Bytes> = Vec::new(&env);
    batch2.push_back(Bytes::from_array(&env, &[2u8; 32]));
    let page_count = client.submit_evidence_batch(&merchant, &escrow_id, &batch2);
    assert_eq!(page_count, 2u32);

    assert_eq!(client.get_evidence_page(&escrow_id, &0u32).len(), 10u32);
    assert_eq!(client.get_evidence_page(&escrow_id, &1u32).len(), 1u32);
}

/// Exactly 10 items sits on the boundary — must be accepted, not rejected.
#[test]
fn test_exact_max_batch_accepted() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let mut items: Vec<Bytes> = Vec::new(&env);
    for i in 0..10u32 {
        items.push_back(Bytes::from_array(&env, &[i as u8; 32]));
    }

    let result = client.try_submit_evidence_batch(&customer, &escrow_id, &items);
    assert!(result.is_ok(), "10-item batch should be accepted");

    let page = client.get_evidence_page(&escrow_id, &0u32);
    assert_eq!(page.len(), 10u32);
}

/// An empty batch (0 items) should succeed and create an empty page rather than
/// panic or error out.
#[test]
fn test_empty_batch_creates_empty_page() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let items: Vec<Bytes> = Vec::new(&env);
    let page_count = client.submit_evidence_batch(&customer, &escrow_id, &items);

    assert_eq!(page_count, 1u32);
    assert_eq!(client.get_evidence_page(&escrow_id, &0u32).len(), 0u32);
}

/// Calling submit_evidence_batch on a non-disputed escrow must return
/// `Error::NotDisputed`, regardless of batch size.
#[test]
fn test_batch_on_non_disputed_escrow_rejected() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);

    // Create escrow but do NOT dispute it — status stays Locked.
    let escrow_id = client.create_escrow(
        &customer, &merchant, &500i128, &token, &9999u64, &0u64, &0u64, &false,
    );

    let mut items: Vec<Bytes> = Vec::new(&env);
    items.push_back(Bytes::from_array(&env, &[1u8; 32]));

    let result = client.try_submit_evidence_batch(&customer, &escrow_id, &items);
    assert_eq!(result, Err(Ok(Error::Action(ActionError::NotDisputed))));
}

/// A third party that is neither the customer nor the merchant must be rejected
/// with `Error::Unauthorized`.
#[test]
fn test_unauthorized_caller_rejected() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let stranger = Address::generate(&env);

    let mut items: Vec<Bytes> = Vec::new(&env);
    items.push_back(Bytes::from_array(&env, &[9u8; 32]));

    let result = client.try_submit_evidence_batch(&stranger, &escrow_id, &items);
    assert_eq!(result, Err(Ok(Error::Basic(BasicError::Unauthorized))));
}

/// A rejected oversized batch must leave no partial state: the page count for
/// the escrow must remain 0 after the failed call.
#[test]
fn test_oversized_batch_leaves_no_partial_state() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let mut items: Vec<Bytes> = Vec::new(&env);
    for _ in 0..15u32 {
        items.push_back(Bytes::from_array(&env, &[0u8; 32]));
    }

    // Must fail.
    let result = client.try_submit_evidence_batch(&customer, &escrow_id, &items);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::BatchTooLarge))));

    // No page should have been written — the page is empty.
    let page = client.get_evidence_page(&escrow_id, &0u32);
    assert_eq!(
        page.len(),
        0u32,
        "no partial state should be stored on failure"
    );
}

#[test]
fn test_backward_compat_get_evidence_still_works() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    client.submit_evidence(
        &customer,
        &escrow_id,
        &soroban_sdk::String::from_str(&env, "QmOldHash"),
    );

    let items = client.get_evidence(&escrow_id, &10u64, &0u64);
    assert_eq!(items.len(), 1u32);
    assert_eq!(
        items.get(0).unwrap().ipfs_hash,
        soroban_sdk::String::from_str(&env, "QmOldHash")
    );
}

/// The merchant (not just the customer) must be allowed to submit a batch.
#[test]
fn test_merchant_can_submit_batch() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let mut items: Vec<Bytes> = Vec::new(&env);
    items.push_back(Bytes::from_array(&env, &[0xaau8; 32]));
    items.push_back(Bytes::from_array(&env, &[0xbbu8; 32]));

    let result = client.try_submit_evidence_batch(&merchant, &escrow_id, &items);
    assert!(
        result.is_ok(),
        "merchant should be allowed to submit evidence"
    );

    let page = client.get_evidence_page(&escrow_id, &0u32);
    assert_eq!(page.len(), 2u32);
}

/// Each successful batch call increments the page counter by exactly 1.
/// After three calls the returned value must be 3.
#[test]
fn test_page_count_increments_with_each_batch() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let mut batch: Vec<Bytes> = Vec::new(&env);
    batch.push_back(Bytes::from_array(&env, &[1u8; 32]));

    let c1 = client.submit_evidence_batch(&customer, &escrow_id, &batch);
    let c2 = client.submit_evidence_batch(&merchant, &escrow_id, &batch);
    let c3 = client.submit_evidence_batch(&customer, &escrow_id, &batch);

    assert_eq!(c1, 1u32);
    assert_eq!(c2, 2u32);
    assert_eq!(c3, 3u32);
}

/// Evidence pages are scoped per escrow. A batch submitted to escrow A must not
/// appear in escrow B's pages and vice-versa.
#[test]
fn test_evidence_pages_are_isolated_per_escrow() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);

    let escrow_a = make_disputed_escrow(&env, &client, &customer, &merchant, &token);
    let escrow_b = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let mut items_a: Vec<Bytes> = Vec::new(&env);
    items_a.push_back(Bytes::from_array(&env, &[0xaau8; 32]));
    items_a.push_back(Bytes::from_array(&env, &[0xbbu8; 32]));

    let mut items_b: Vec<Bytes> = Vec::new(&env);
    items_b.push_back(Bytes::from_array(&env, &[0xccu8; 32]));

    client.submit_evidence_batch(&customer, &escrow_a, &items_a);
    client.submit_evidence_batch(&customer, &escrow_b, &items_b);

    // Escrow A has 2 items, escrow B has 1 — no bleed-over.
    assert_eq!(client.get_evidence_page(&escrow_a, &0u32).len(), 2u32);
    assert_eq!(client.get_evidence_page(&escrow_b, &0u32).len(), 1u32);
}

/// Every Evidence entry stored in a batch must record the caller as submitter.
#[test]
fn test_submitter_address_recorded_on_each_evidence_entry() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let mut items: Vec<Bytes> = Vec::new(&env);
    items.push_back(Bytes::from_array(&env, &[1u8; 32]));
    items.push_back(Bytes::from_array(&env, &[2u8; 32]));
    items.push_back(Bytes::from_array(&env, &[3u8; 32]));

    client.submit_evidence_batch(&merchant, &escrow_id, &items);

    let page = client.get_evidence_page(&escrow_id, &0u32);
    for i in 0..page.len() {
        assert_eq!(
            page.get(i).unwrap().submitter,
            merchant,
            "entry {} should record merchant as submitter",
            i
        );
    }
}

/// After the 7-day evidence deadline has passed, submit_evidence_batch must
/// refuse with EvidenceDeadlinePassed (same guard as the single-item path).
#[test]
fn test_batch_rejected_after_evidence_deadline() {
    let env = Env::default();
    // Set a known base timestamp so we can calculate the deadline precisely.
    env.ledger().set_timestamp(1_000);

    let (client, _admin, customer, merchant, token) = setup(&env);

    // create_escrow and dispute_escrow are called at timestamp 1_000.
    // dispute_escrow sets evidence_deadline = dispute_started_at + 7 days.
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    const SEVEN_DAYS_SECS: u64 = 7 * 24 * 60 * 60;
    // Advance past the deadline.
    env.ledger().set_timestamp(1_000 + SEVEN_DAYS_SECS + 1);

    let mut items: Vec<Bytes> = Vec::new(&env);
    items.push_back(Bytes::from_array(&env, &[0xffu8; 32]));

    let result = client.try_submit_evidence_batch(&customer, &escrow_id, &items);
    assert_eq!(
        result,
        Err(Ok(Error::Action(ActionError::EvidenceDeadlinePassed)))
    );
}

/// Evidence hashes longer than 64 bytes (SHA-256 hex length) must be rejected
/// with EvidenceLimitExceeded to prevent storage bloat.
#[test]
fn test_batch_rejected_when_hash_exceeds_max_length() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let mut items: Vec<Bytes> = Vec::new(&env);
    items.push_back(Bytes::from_array(&env, &[0xabu8; 65]));

    let result = client.try_submit_evidence_batch(&customer, &escrow_id, &items);
    assert_eq!(
        result,
        Err(Ok(Error::Escrow(EscrowError::EvidenceLimitExceeded)))
    );
}

// ── Merkle bundle commitment (issue #214) ────────────────────────────────────

use soroban_sdk::BytesN;

// ---- helpers ----------------------------------------------------------------

/// Build a 32-byte `bafy…`-prefixed CID that exactly meets the 32-byte minimum.
/// The real CIDv1 alphabet constraint is not enforced here — just prefix + length.
fn make_cid(env: &Env, _tag: u8) -> soroban_sdk::String {
    // 59-char real-world CIDv1 — valid prefix "bafy", valid length.
    soroban_sdk::String::from_str(env, "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi")
}

fn make_ar_cid(env: &Env) -> soroban_sdk::String {
    // "ar://" + 41-char Arweave TX ID = 46 chars total (>= 32).
    soroban_sdk::String::from_str(env, "ar://abc123456789012345678901234567890abcde12")
}

fn make_qm_cid(env: &Env) -> soroban_sdk::String {
    // IPFS CIDv0 ("Qm…") — 46 chars.
    soroban_sdk::String::from_str(env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG")
}

fn zero_root(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0u8; 32])
}

fn root_from(env: &Env, b: u8) -> BytesN<32> {
    BytesN::from_array(env, &[b; 32])
}

/// Build a single-leaf Merkle tree: root == keccak256(leaf || leaf) or just leaf
/// depending on implementation.  For a 1-leaf tree with empty proof the root IS
/// the leaf hash, so we can set root = keccak256(data).
fn leaf_hash(env: &Env, data: &[u8; 32]) -> BytesN<32> {
    let buf = soroban_sdk::Bytes::from_array(env, data);
    env.crypto().keccak256(&buf).into()
}

// ---- commit_evidence_bundle ------------------------------------------------

#[test]
fn test_commit_bundle_success_bafy_cid() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let root = root_from(&env, 0xAA);
    let cid = make_cid(&env, b'z');

    let bundle_id = client.commit_evidence_bundle(&customer, &escrow_id, &root, &cid, &100u32);
    assert_eq!(bundle_id, 1u32);

    let bundle = client.get_evidence_bundle(&escrow_id, &1u32).unwrap();
    assert_eq!(bundle.bundle_id, 1u32);
    assert_eq!(bundle.escrow_id, escrow_id);
    assert_eq!(bundle.submitter, customer);
    assert_eq!(bundle.merkle_root, root);
    assert_eq!(bundle.leaf_count, 100u32);
}

#[test]
fn test_commit_bundle_success_ar_cid() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let root = root_from(&env, 0xBB);
    let cid = make_ar_cid(&env);

    let id = client.commit_evidence_bundle(&customer, &escrow_id, &root, &cid, &1u32);
    assert_eq!(id, 1u32);
}

#[test]
fn test_commit_bundle_success_qm_cid() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let root = root_from(&env, 0xCC);
    let cid = make_qm_cid(&env);

    let id = client.commit_evidence_bundle(&customer, &escrow_id, &root, &cid, &50u32);
    assert_eq!(id, 1u32);
}

#[test]
fn test_commit_bundle_merchant_allowed() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let root = root_from(&env, 0x01);
    let cid = make_cid(&env, b'y');

    let result = client.try_commit_evidence_bundle(&merchant, &escrow_id, &root, &cid, &10u32);
    assert!(result.is_ok(), "merchant must be allowed to commit a bundle");
}

#[test]
fn test_commit_bundle_increments_count() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    assert_eq!(client.get_evidence_bundle_count(&escrow_id), 0u32);

    client.commit_evidence_bundle(&customer, &escrow_id, &root_from(&env, 1), &make_cid(&env, b'a'), &1u32);
    assert_eq!(client.get_evidence_bundle_count(&escrow_id), 1u32);

    client.commit_evidence_bundle(&merchant, &escrow_id, &root_from(&env, 2), &make_cid(&env, b'b'), &1u32);
    assert_eq!(client.get_evidence_bundle_count(&escrow_id), 2u32);

    client.commit_evidence_bundle(&customer, &escrow_id, &root_from(&env, 3), &make_cid(&env, b'c'), &1u32);
    assert_eq!(client.get_evidence_bundle_count(&escrow_id), 3u32);
}

// ---- CID validation ---------------------------------------------------------

#[test]
fn test_commit_bundle_cid_too_short_rejected() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    // Only 10 bytes — below the 32-byte minimum.
    let short_cid = soroban_sdk::String::from_str(&env, "bafy123456");
    let result = client.try_commit_evidence_bundle(
        &customer, &escrow_id, &zero_root(&env), &short_cid, &1u32,
    );
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::InvalidCidUri))));
}

#[test]
fn test_commit_bundle_cid_too_long_rejected() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    // 129 bytes — above the 128-byte maximum.
    // We construct a 129-char string: "bafy" + 125 'z' chars.
    let long_cid = soroban_sdk::String::from_str(&env,
        "bafyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz");
    let result = client.try_commit_evidence_bundle(
        &customer, &escrow_id, &zero_root(&env), &long_cid, &1u32,
    );
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::InvalidCidUri))));
}

#[test]
fn test_commit_bundle_invalid_prefix_rejected() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    // Valid length but wrong prefix ("ipfs://…").
    let bad_cid = soroban_sdk::String::from_str(&env,
        "ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtzy55fbzdi");
    let result = client.try_commit_evidence_bundle(
        &customer, &escrow_id, &zero_root(&env), &bad_cid, &1u32,
    );
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::InvalidCidUri))));
}

#[test]
fn test_commit_bundle_control_char_in_cid_rejected() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    // Valid prefix, valid length, but contains a tab (0x09) at position 10.
    // Construct a [u8; 59] array: "bafy" prefix + printable fill + one tab.
    let cid_bytes: [u8; 59] = [
        0x62, 0x61, 0x66, 0x79, // "bafy"
        0x78, 0x78, 0x78, 0x78, 0x78, 0x78, // xxxxxx
        0x09, // TAB — control character
        0x78, 0x78, 0x78, 0x78, 0x78, 0x78, 0x78, 0x78, 0x78, 0x78, // xxxxxxxxxx
        0x78, 0x78, 0x78, 0x78, 0x78, 0x78, 0x78, 0x78, 0x78, 0x78, // xxxxxxxxxx
        0x78, 0x78, 0x78, 0x78, 0x78, 0x78, 0x78, 0x78, 0x78, 0x78, // xxxxxxxxxx
        0x78, 0x78, 0x78, 0x78, 0x78, 0x78, 0x78, 0x78, 0x78, 0x78, // xxxxxxxxxx
        0x78, 0x78, 0x78, 0x78, 0x78, 0x78, 0x78, 0x78, // xxxxxxxx
    ];
    let bad_cid = soroban_sdk::String::from_bytes(
        &env,
        &cid_bytes,
    );
    let result = client.try_commit_evidence_bundle(
        &customer, &escrow_id, &zero_root(&env), &bad_cid, &1u32,
    );
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::InvalidCidUri))));
}

// ---- Duplicate root rejection -----------------------------------------------

#[test]
fn test_commit_bundle_duplicate_root_same_submitter_rejected() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let root = root_from(&env, 0xDE);
    let cid1 = make_cid(&env, b'p');
    let cid2 = make_cid(&env, b'q');

    client.commit_evidence_bundle(&customer, &escrow_id, &root, &cid1, &1u32);

    // Same submitter, same root — must be rejected.
    let result = client.try_commit_evidence_bundle(&customer, &escrow_id, &root, &cid2, &1u32);
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::DuplicateMerkleRoot))));
}

#[test]
fn test_commit_bundle_same_root_different_submitter_allowed() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let root = root_from(&env, 0xDE);
    let cid = make_cid(&env, b'p');

    // Customer commits root first.
    client.commit_evidence_bundle(&customer, &escrow_id, &root, &cid, &1u32);

    // Merchant commits the *same* root — different submitter, so allowed.
    let result = client.try_commit_evidence_bundle(&merchant, &escrow_id, &root, &make_cid(&env, b'q'), &1u32);
    assert!(result.is_ok(), "different submitter may commit the same root");
}

#[test]
fn test_commit_bundle_different_root_same_submitter_allowed() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let root1 = root_from(&env, 0xAA);
    let root2 = root_from(&env, 0xBB);

    client.commit_evidence_bundle(&customer, &escrow_id, &root1, &make_cid(&env, b'a'), &1u32);

    // Different root — should be accepted and assigned bundle_id 2.
    let id = client.commit_evidence_bundle(&customer, &escrow_id, &root2, &make_cid(&env, b'b'), &1u32);
    assert_eq!(id, 2u32);
}

// ---- leaf_count validation --------------------------------------------------

#[test]
fn test_commit_bundle_zero_leaf_count_rejected() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let result = client.try_commit_evidence_bundle(
        &customer, &escrow_id, &zero_root(&env), &make_cid(&env, b'z'), &0u32,
    );
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::InvalidBundleLeafCount))));
}

#[test]
fn test_commit_bundle_over_max_leaf_count_rejected() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    // 10_001 > 10_000 maximum.
    let result = client.try_commit_evidence_bundle(
        &customer, &escrow_id, &zero_root(&env), &make_cid(&env, b'z'), &10_001u32,
    );
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::InvalidBundleLeafCount))));
}

#[test]
fn test_commit_bundle_max_leaf_count_accepted() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let result = client.try_commit_evidence_bundle(
        &customer, &escrow_id, &root_from(&env, 1), &make_cid(&env, b'z'), &10_000u32,
    );
    assert!(result.is_ok(), "10_000 leaves must be accepted");
}

// ---- access control ---------------------------------------------------------

#[test]
fn test_commit_bundle_non_participant_rejected() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let stranger = Address::generate(&env);
    let result = client.try_commit_evidence_bundle(
        &stranger, &escrow_id, &zero_root(&env), &make_cid(&env, b'z'), &1u32,
    );
    assert_eq!(result, Err(Ok(Error::Basic(BasicError::Unauthorized))));
}

#[test]
fn test_commit_bundle_non_disputed_escrow_rejected() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);

    // Locked, not disputed.
    let escrow_id = client.create_escrow(
        &customer, &merchant, &500i128, &token, &9999u64, &0u64, &0u64, &false,
    );

    let result = client.try_commit_evidence_bundle(
        &customer, &escrow_id, &zero_root(&env), &make_cid(&env, b'z'), &1u32,
    );
    assert_eq!(result, Err(Ok(Error::Action(ActionError::NotDisputed))));
}

#[test]
fn test_commit_bundle_not_found_rejected() {
    let env = Env::default();
    let (client, _admin, _customer, _merchant, _token) = setup(&env);

    let stranger = Address::generate(&env);
    let result = client.try_commit_evidence_bundle(
        &stranger, &999u64, &zero_root(&env), &make_cid(&env, b'z'), &1u32,
    );
    assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::NotFound))));
}

// ---- evidence deadline enforcement ------------------------------------------

#[test]
fn test_commit_bundle_after_evidence_deadline_rejected() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000);
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    // Advance past 7-day evidence deadline.
    const SEVEN_DAYS: u64 = 7 * 24 * 60 * 60;
    env.ledger().set_timestamp(1_000 + SEVEN_DAYS + 1);

    let result = client.try_commit_evidence_bundle(
        &customer, &escrow_id, &zero_root(&env), &make_cid(&env, b'z'), &1u32,
    );
    assert_eq!(result, Err(Ok(Error::Action(ActionError::EvidenceDeadlinePassed))));
}

// ---- verify_evidence_bundle_item -------------------------------------------

#[test]
fn test_verify_bundle_item_single_leaf_valid_proof() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    // For a single-leaf tree the root IS the leaf hash and the proof is empty.
    let data = [0xABu8; 32];
    let item_h = leaf_hash(&env, &data);
    let root = item_h.clone(); // single-leaf: root == leaf hash

    client.commit_evidence_bundle(&customer, &escrow_id, &root, &make_cid(&env, b'v'), &1u32);

    let proof: Vec<BytesN<32>> = Vec::new(&env);
    let valid = client.verify_evidence_bundle_item(&escrow_id, &1u32, &item_h, &proof, &0u32);
    assert!(valid, "single-leaf proof with matching root must pass");
}

#[test]
fn test_verify_bundle_item_wrong_hash_fails() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let root = root_from(&env, 0xFF);
    client.commit_evidence_bundle(&customer, &escrow_id, &root, &make_cid(&env, b'w'), &1u32);

    // Provide a hash that does not match root with an empty proof.
    let bad_hash = root_from(&env, 0x00);
    let proof: Vec<BytesN<32>> = Vec::new(&env);
    let valid = client.verify_evidence_bundle_item(&escrow_id, &1u32, &bad_hash, &proof, &0u32);
    assert!(!valid, "wrong hash must not verify");
}

#[test]
fn test_verify_bundle_item_nonexistent_bundle_returns_false() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let proof: Vec<BytesN<32>> = Vec::new(&env);
    // Bundle 99 was never committed.
    let valid = client.verify_evidence_bundle_item(
        &escrow_id, &99u32, &zero_root(&env), &proof, &0u32,
    );
    assert!(!valid, "non-existent bundle must return false");
}

#[test]
fn test_verify_bundle_item_two_leaf_tree() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    // Two-leaf tree:
    //        root
    //       /    \
    //    h(L0)  h(L1)
    //
    // root = keccak256(h(L0) || h(L1))
    let l0_data = [0x11u8; 32];
    let l1_data = [0x22u8; 32];
    let h0: BytesN<32> = leaf_hash(&env, &l0_data);
    let h1: BytesN<32> = leaf_hash(&env, &l1_data);

    // Build root: sorted pair (OpenZeppelin-style is max(a,b)||min(a,b),
    // but the contract uses index parity, so left=h0, right=h1 for leaf_index=0).
    let mut raw = [0u8; 64];
    raw[..32].copy_from_slice(&h0.to_array());
    raw[32..].copy_from_slice(&h1.to_array());
    let root: BytesN<32> = env.crypto().keccak256(&soroban_sdk::Bytes::from_array(&env, &raw)).into();

    client.commit_evidence_bundle(&customer, &escrow_id, &root, &make_cid(&env, b'x'), &2u32);

    // Proof for leaf_index=0 is just [h1].
    let mut proof: Vec<BytesN<32>> = Vec::new(&env);
    proof.push_back(h1);
    let valid = client.verify_evidence_bundle_item(&escrow_id, &1u32, &h0, &proof, &0u32);
    assert!(valid, "two-leaf tree proof for index 0 must verify");
}

// ---- bundle isolation per escrow --------------------------------------------

#[test]
fn test_bundles_isolated_per_escrow() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);

    let escrow_a = make_disputed_escrow(&env, &client, &customer, &merchant, &token);
    let escrow_b = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    client.commit_evidence_bundle(&customer, &escrow_a, &root_from(&env, 1), &make_cid(&env, b'a'), &1u32);
    client.commit_evidence_bundle(&customer, &escrow_a, &root_from(&env, 2), &make_cid(&env, b'b'), &1u32);
    client.commit_evidence_bundle(&customer, &escrow_b, &root_from(&env, 3), &make_cid(&env, b'c'), &1u32);

    assert_eq!(client.get_evidence_bundle_count(&escrow_a), 2u32);
    assert_eq!(client.get_evidence_bundle_count(&escrow_b), 1u32);

    // Bundle 1 of escrow_a must not be visible in escrow_b.
    assert!(client.get_evidence_bundle(&escrow_b, &2u32).is_none());
}

// ---- storage footprint note -------------------------------------------------

/// Demonstrates the storage-footprint improvement: committing a single bundle
/// with 1000 leaves uses exactly 3 instance-storage entries
/// (EvidenceBundle, EvidenceBundleCount, EvidenceBundleDupRoot) rather than
/// 1000 separate Evidence entries, each of which would be its own entry.
#[test]
fn test_bundle_commit_uses_fixed_storage_regardless_of_leaf_count() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let before = env.to_ledger_snapshot();

    client.commit_evidence_bundle(
        &customer, &escrow_id, &root_from(&env, 0xCC), &make_cid(&env, b'z'), &1_000u32,
    );

    let after = env.to_ledger_snapshot();

    // Exactly 3 new entries added (bundle record, count, dedup guard).
    let delta = after.ledger_entries.len() as i64 - before.ledger_entries.len() as i64;
    assert_eq!(delta, 3, "bundle commitment must add exactly 3 ledger entries");
}
