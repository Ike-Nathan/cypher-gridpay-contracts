//! Storage helpers that keep ledger entries alive.
//!
//! On Soroban, persistent entries and the contract instance are archived once
//! their TTL runs out, locking the funds and records they hold until someone
//! restores them. Every active read or write goes through these helpers so the
//! touched entry's TTL is extended automatically.

use soroban_sdk::{contracttype, Address, Env, IntoVal, TryFromVal, Val};

use crate::{EpochReasonCodeStats, RefundReasonCode};

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

// ── Per-merchant, per-epoch reason-code analytics ───────────────────────────

/// Length of an analytics epoch (30 days, roughly a month).
pub const REASON_EPOCH_SECS: u64 = 30 * 24 * 60 * 60;

/// Every reason code, in a fixed order. Queries walk this list, so they always
/// touch exactly `ALL_REASON_CODES.len()` (6) keys per epoch.
pub const ALL_REASON_CODES: [RefundReasonCode; 6] = [
    RefundReasonCode::ProductDefect,
    RefundReasonCode::NonDelivery,
    RefundReasonCode::DuplicateCharge,
    RefundReasonCode::Unauthorized,
    RefundReasonCode::CustomerRequest,
    RefundReasonCode::Other,
];

#[contracttype]
#[derive(Clone)]
pub enum AnalyticsKey {
    /// Stats for (merchant, epoch, reason); `None` is the platform-wide total.
    EpochReasonStats(Option<Address>, u32, RefundReasonCode),
}

/// The epoch a timestamp falls in.
pub fn reason_epoch(timestamp: u64) -> u32 {
    (timestamp / REASON_EPOCH_SECS) as u32
}

/// Stats for one (scope, epoch, reason); zeroed when nothing was recorded.
pub fn read_epoch_reason_stats(
    env: &Env,
    merchant: Option<Address>,
    epoch_id: u32,
    reason_code: RefundReasonCode,
) -> EpochReasonCodeStats {
    get_persistent(
        env,
        &AnalyticsKey::EpochReasonStats(merchant, epoch_id, reason_code.clone()),
    )
    .unwrap_or(EpochReasonCodeStats {
        epoch_id,
        reason_code,
        count: 0,
        total_amount: 0,
        approved_count: 0,
    })
}

/// Records a refund event against both the merchant's and the platform-wide
/// stats for its epoch: a request (`count`), or a payout (`approved_count`
/// and `total_amount`). Two reads and two writes.
pub fn record_epoch_reason_stats(
    env: &Env,
    merchant: &Address,
    epoch_id: u32,
    reason_code: &RefundReasonCode,
    paid_amount: Option<i128>,
) {
    for scope in [Some(merchant.clone()), None] {
        let mut stats = read_epoch_reason_stats(env, scope.clone(), epoch_id, reason_code.clone());
        match paid_amount {
            None => stats.count = stats.count.saturating_add(1),
            Some(amount) => {
                stats.approved_count = stats.approved_count.saturating_add(1);
                stats.total_amount = stats.total_amount.saturating_add(amount);
            }
        }
        set_persistent(
            env,
            &AnalyticsKey::EpochReasonStats(scope, epoch_id, reason_code.clone()),
            &stats,
        );
    }
}
