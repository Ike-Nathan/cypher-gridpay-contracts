//! Storage helpers that keep ledger entries alive.
//!
//! On Soroban, persistent entries and the contract instance are archived once
//! their TTL runs out, locking the funds and records they hold until someone
//! restores them. Every active read or write goes through these helpers so the
//! touched entry's TTL is extended automatically.

use soroban_sdk::{Env, IntoVal, TryFromVal, Val, Vec};

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

/// Contract-instance storage is loaded (and paid for) on every invocation, and
/// a ledger entry can't exceed 64 KiB. Configuration kept there must stay
/// below this many serialized bytes even when fully loaded; collections that
/// grow live in sharded persistent entries instead (see [`write_sharded`]).
pub const MAX_INSTANCE_STORAGE_BYTES: u32 = 4_096;

/// Elements per persistent shard of a sharded collection.
pub const SHARD_CAPACITY: u32 = 32;

/// Number of shards needed for `count` elements.
fn shard_count(count: u32) -> u32 {
    count.div_ceil(SHARD_CAPACITY)
}

/// Reads every element of a sharded collection, in order.
///
/// The element count is a scalar header in instance storage under
/// `count_key`; shard `i` is a persistent entry at `shard_key(i)` holding
/// elements `i * SHARD_CAPACITY ..`. Returns `None` if the collection has
/// never been written in sharded form (callers fall back to a legacy key).
pub fn read_sharded<T, CK, SK, F>(env: &Env, count_key: &CK, shard_key: F) -> Option<Vec<T>>
where
    T: IntoVal<Env, Val> + TryFromVal<Env, Val>,
    CK: IntoVal<Env, Val>,
    SK: IntoVal<Env, Val>,
    F: Fn(u32) -> SK,
{
    let count: u32 = env.storage().instance().get(count_key)?;
    let mut items = Vec::new(env);
    for shard in 0..shard_count(count) {
        if let Some(part) = get_persistent::<SK, Vec<T>>(env, &shard_key(shard)) {
            items.append(&part);
        }
    }
    Some(items)
}

/// Replaces a sharded collection with `items`: writes the shards it needs,
/// removes shards it no longer needs, and updates the count header.
pub fn write_sharded<T, CK, SK, F>(env: &Env, count_key: &CK, shard_key: F, items: &Vec<T>)
where
    T: IntoVal<Env, Val> + TryFromVal<Env, Val>,
    CK: IntoVal<Env, Val>,
    SK: IntoVal<Env, Val>,
    F: Fn(u32) -> SK,
{
    let old_count: u32 = env.storage().instance().get(count_key).unwrap_or(0);
    let count = items.len();
    for shard in 0..shard_count(count) {
        let start = shard * SHARD_CAPACITY;
        let end = (start + SHARD_CAPACITY).min(count);
        set_persistent(env, &shard_key(shard), &items.slice(start..end));
    }
    for shard in shard_count(count)..shard_count(old_count) {
        env.storage().persistent().remove(&shard_key(shard));
    }
    env.storage().instance().set(count_key, &count);
}

/// Serialized XDR size of everything in `contract`'s instance storage. Test
/// helper for asserting the [`MAX_INSTANCE_STORAGE_BYTES`] budget.
#[cfg(test)]
pub fn instance_storage_xdr_len(env: &Env, contract: &soroban_sdk::Address) -> u32 {
    use soroban_sdk::{testutils::storage::Instance as _, xdr::ToXdr};
    env.as_contract(contract, || env.storage().instance().all().to_xdr(env).len())
}

/// Panics unless `contract`'s instance storage is below
/// [`MAX_INSTANCE_STORAGE_BYTES`] serialized.
#[cfg(test)]
pub fn assert_instance_storage_within_budget(env: &Env, contract: &soroban_sdk::Address) {
    let len = instance_storage_xdr_len(env, contract);
    assert!(
        len < MAX_INSTANCE_STORAGE_BYTES,
        "instance storage is {len} bytes, budget is {MAX_INSTANCE_STORAGE_BYTES}"
    );
}
