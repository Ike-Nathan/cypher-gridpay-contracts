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

/// Shortest ledger close time assumed when converting a business deadline (in
/// seconds) into ledgers. Ledgers close every ~5 s; assuming 4 s means faster
/// ledgers can never make an entry expire before its deadline.
pub const MIN_SECONDS_PER_LEDGER: u64 = 4;
/// Ledgers added on top of every deadline-derived TTL.
pub const DEADLINE_SAFETY_LEDGERS: u32 = DAY_IN_LEDGERS;

/// How long an entry is kept alive, by what it holds.
///
/// * `CriticalActiveLock` - funds or authority are locked by it (an escrowed
///   or disputed record, an arbitration, a reserve, a multisig config). Long
///   bump, high threshold, and never shorter than its business deadline
///   (see [`extend_persistent_until`] / [`extend_instance_until`]).
/// * `StandardActive` - live but not locking funds.
/// * `TerminalAuditRecord` - completed, refunded or resolved: kept only for
///   the minimum audit retention, so finished small records stop costing rent.
/// * `EphemeralWindow` - short-lived windows (rate limits, caches).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageTier {
    CriticalActiveLock,
    StandardActive,
    TerminalAuditRecord,
    EphemeralWindow,
}

impl StorageTier {
    /// Ledgers an entry of this tier is extended to.
    pub const fn bump_amount(self) -> u32 {
        match self {
            StorageTier::CriticalActiveLock => 120 * DAY_IN_LEDGERS,
            StorageTier::StandardActive => 30 * DAY_IN_LEDGERS,
            StorageTier::TerminalAuditRecord => 14 * DAY_IN_LEDGERS,
            StorageTier::EphemeralWindow => DAY_IN_LEDGERS,
        }
    }

    /// An entry of this tier is extended once its TTL drops below this.
    pub const fn threshold(self) -> u32 {
        match self {
            StorageTier::CriticalActiveLock => 90 * DAY_IN_LEDGERS,
            StorageTier::StandardActive => 29 * DAY_IN_LEDGERS,
            StorageTier::TerminalAuditRecord => 7 * DAY_IN_LEDGERS,
            StorageTier::EphemeralWindow => DAY_IN_LEDGERS / 2,
        }
    }
}

/// Ledgers until `timestamp` (0 if it has passed), at the conservative
/// [`MIN_SECONDS_PER_LEDGER`].
pub fn ledgers_until(env: &Env, timestamp: u64) -> u32 {
    let secs = timestamp.saturating_sub(env.ledger().timestamp());
    secs.div_ceil(MIN_SECONDS_PER_LEDGER).min(u32::MAX as u64) as u32
}

/// TTL (in ledgers) an entry needs to outlive `deadline + grace_secs`, never
/// below `tier`'s bump, capped at the network maximum. A deadline beyond the
/// maximum gets the maximum; a keeper must bump it again later
/// (`batch_bump_critical`).
pub fn required_ttl(env: &Env, tier: StorageTier, deadline: u64, grace_secs: u64) -> u32 {
    let needed = ledgers_until(env, deadline.saturating_add(grace_secs))
        .saturating_add(DEADLINE_SAFETY_LEDGERS);
    needed.max(tier.bump_amount()).min(env.storage().max_ttl())
}

/// Extends a persistent entry by its tier's policy. The entry must exist.
pub fn extend_persistent_tiered<K>(env: &Env, key: &K, tier: StorageTier)
where
    K: IntoVal<Env, Val>,
{
    let max = env.storage().max_ttl();
    env.storage().persistent().extend_ttl(
        key,
        tier.threshold().min(max),
        tier.bump_amount().min(max),
    );
}

/// Extends a persistent entry so it outlives `deadline + grace_secs` (and at
/// least its tier's bump). The entry must exist. Returns the TTL target.
pub fn extend_persistent_until<K>(
    env: &Env,
    key: &K,
    tier: StorageTier,
    deadline: u64,
    grace_secs: u64,
) -> u32
where
    K: IntoVal<Env, Val>,
{
    let target = required_ttl(env, tier, deadline, grace_secs);
    env.storage().persistent().extend_ttl(key, target, target);
    target
}

/// Extends the contract instance (and every record kept in instance storage)
/// so it outlives `deadline + grace_secs` at `tier`. Returns the TTL target.
pub fn extend_instance_until(env: &Env, tier: StorageTier, deadline: u64, grace_secs: u64) -> u32 {
    let target = required_ttl(env, tier, deadline, grace_secs);
    env.storage().instance().extend_ttl(target, target);
    target
}
