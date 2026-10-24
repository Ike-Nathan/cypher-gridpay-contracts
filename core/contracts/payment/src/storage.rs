//! Storage helpers that keep ledger entries alive.
//!
//! On Soroban, persistent entries and the contract instance are archived once
//! their TTL runs out, locking the funds and records they hold until someone
//! restores them. Every active read or write goes through these helpers so the
//! touched entry's TTL is extended automatically.

use soroban_sdk::{
    contracttype, symbol_short, xdr::ToXdr, Bytes, BytesN, Env, IntoVal, Symbol, TryFromVal, Val,
    Vec,
};

/// Approximate number of ledgers closed per day (~5s per ledger).
pub const DAY_IN_LEDGERS: u32 = 17_280;

/// Persistent entries are extended to this many ledgers when touched.
pub const PERSISTENT_BUMP_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
/// Persistent entries are only extended once their TTL drops below this.
pub const PERSISTENT_LIFETIME_THRESHOLD: u32 = PERSISTENT_BUMP_AMOUNT - DAY_IN_LEDGERS;

/// The contract instance is extended to this many ledgers when touched.
pub const INSTANCE_BUMP_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
/// The contract instance is only extended once its TTL drops below this.
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = INSTANCE_BUMP_AMOUNT - DAY_IN_LEDGERS;

/// Extends the TTL of the contract instance (and all instance storage).
pub fn extend_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

/// Extends the TTL of a persistent entry. The entry must exist.
pub fn extend_persistent<K>(env: &Env, key: &K)
where
    K: IntoVal<Env, Val>,
{
    env.storage().persistent().extend_ttl(
        key,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );
}

/// Reads a persistent entry, extending its TTL if it exists.
pub fn get_persistent<K, V>(env: &Env, key: &K) -> Option<V>
where
    K: IntoVal<Env, Val>,
    V: TryFromVal<Env, Val>,
{
    let value = env.storage().persistent().get::<K, V>(key);
    if value.is_some() {
        extend_persistent(env, key);
    }
    value
}

/// Checks whether a persistent entry exists, extending its TTL if it does.
pub fn has_persistent<K>(env: &Env, key: &K) -> bool
where
    K: IntoVal<Env, Val>,
{
    let exists = env.storage().persistent().has(key);
    if exists {
        extend_persistent(env, key);
    }
    exists
}

/// Writes a persistent entry and extends its TTL.
pub fn set_persistent<K, V>(env: &Env, key: &K, value: &V)
where
    K: IntoVal<Env, Val>,
    V: IntoVal<Env, Val>,
{
    env.storage().persistent().set(key, value);
    extend_persistent(env, key);
}

/// Depth of the archived-record Merkle accumulator (Issue #243): up to 2^20
/// leaves, with a frontier of 20 * 32 = 640 bytes.
pub const MERKLE_TREE_DEPTH: u32 = 20;

const MERKLE_FRONTIER_KEY: Symbol = symbol_short!("arch_imt");

/// Incremental Merkle tree over the SHA-256 digests of finalized/archived
/// records (Issue #243). Only the rightmost filled node of each level is kept,
/// so off-chain copies of archived records can be proven against `root`.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct IncrementalMerkleFrontier {
    pub frontier: Vec<BytesN<32>>,
    pub leaf_count: u64,
    pub root: BytesN<32>,
}

fn hash_pair(env: &Env, left: &BytesN<32>, right: &BytesN<32>) -> BytesN<32> {
    let mut data = Bytes::from_array(env, &left.to_array());
    data.extend_from_array(&right.to_array());
    env.crypto().sha256(&data).to_bytes()
}

/// Returns the stored frontier, or the empty tree (all-zero leaves).
pub fn get_merkle_frontier(env: &Env) -> IncrementalMerkleFrontier {
    if let Some(imt) = get_persistent(env, &MERKLE_FRONTIER_KEY) {
        return imt;
    }
    let mut frontier = Vec::new(env);
    let mut zero = BytesN::from_array(env, &[0u8; 32]);
    for _ in 0..MERKLE_TREE_DEPTH {
        frontier.push_back(zero.clone());
        zero = hash_pair(env, &zero, &zero);
    }
    IncrementalMerkleFrontier {
        frontier,
        leaf_count: 0,
        root: zero,
    }
}

/// SHA-256 digest of a record's XDR encoding, used as its Merkle leaf.
pub fn archived_record_leaf<T>(env: &Env, record: &T) -> BytesN<32>
where
    T: IntoVal<Env, Val> + Clone,
{
    env.crypto().sha256(&record.clone().to_xdr(env)).to_bytes()
}

/// Appends a leaf to the accumulator and returns its index.
///
/// # Panics
/// Panics once all 2^`MERKLE_TREE_DEPTH` leaves are used.
pub fn append_archived_leaf(env: &Env, leaf: BytesN<32>) -> u64 {
    let mut imt = get_merkle_frontier(env);
    let index = imt.leaf_count;
    if index >= 1u64 << MERKLE_TREE_DEPTH {
        panic!("archived record accumulator is full");
    }

    let mut node = leaf;
    let mut zero = BytesN::from_array(env, &[0u8; 32]);
    let mut position = index;
    for level in 0..MERKLE_TREE_DEPTH {
        if position & 1 == 0 {
            imt.frontier.set(level, node.clone());
            node = hash_pair(env, &node, &zero);
        } else {
            node = hash_pair(env, &imt.frontier.get_unchecked(level), &node);
        }
        zero = hash_pair(env, &zero, &zero);
        position >>= 1;
    }

    imt.root = node;
    imt.leaf_count += 1;
    set_persistent(env, &MERKLE_FRONTIER_KEY, &imt);
    index
}

/// Checks that `leaf_hash` sits at `index` under the current root, given its
/// `MERKLE_TREE_DEPTH` sibling hashes ordered from the leaf level upwards.
/// Indices at or beyond `leaf_count` are rejected.
pub fn verify_archived_record_inclusion(
    env: &Env,
    leaf_hash: BytesN<32>,
    index: u64,
    proof: Vec<BytesN<32>>,
) -> bool {
    let imt = get_merkle_frontier(env);
    if index >= imt.leaf_count || proof.len() != MERKLE_TREE_DEPTH {
        return false;
    }

    let mut node = leaf_hash;
    let mut position = index;
    for sibling in proof.iter() {
        node = if position & 1 == 0 {
            hash_pair(env, &node, &sibling)
        } else {
            hash_pair(env, &sibling, &node)
        };
        position >>= 1;
    }
    node == imt.root
}
