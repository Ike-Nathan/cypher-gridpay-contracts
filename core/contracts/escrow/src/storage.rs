//! Storage helpers that keep ledger entries alive.
//!
//! On Soroban, persistent entries and the contract instance are archived once
//! their TTL runs out, locking the funds and records they hold until someone
//! restores them. Every active read or write goes through these helpers so the
//! touched entry's TTL is extended automatically.

use soroban_sdk::{Env, IntoVal, TryFromVal, Val};

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
