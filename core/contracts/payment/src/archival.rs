//! Storage archival and state compression for historical payments (#197).
//!
//! Terminal payments (`Completed`, `Refunded`, `Cancelled`) that have aged past
//! the retention and refund windows are compressed: the full `Payment` is
//! hashed (SHA-256 over its XDR), a `CompactArchivedPayment` commitment is
//! stored, and the verbose payload (metadata, notes, detached metadata/memo/tag
//! entries) is dropped. A stub `Payment` keeps `id`, `customer`, `merchant`,
//! `amount`, `token`, `currency` and the terminal `status` so replay and
//! double-completion guards keep working forever.
//!
//! The existing contract does not record when a payment became terminal, so
//! compaction is two-phase: the first call to `compact_settled_payments` that
//! sees a terminal payment records a first-seen timestamp (a conservative upper
//! bound on the real terminal time); a later call compacts it once the
//! timestamp is at least `max(archival_retention_secs, max_refund_window_secs)`
//! old.

use soroban_sdk::{
    contracttype, symbol_short, xdr::ToXdr, Address, BytesN, Env, String, Symbol, Vec,
};

use crate::{
    BasicError, DataKey, Error, MultiSigConfig, ConfigKey, Payment, PaymentKey, PaymentStatus,
    StateDataKey,
};

const CFG_KEY: Symbol = symbol_short!("arch_cfg");
const SEEN_KEY: Symbol = symbol_short!("arch_seen");
const ARCH_KEY: Symbol = symbol_short!("arch_rec");
pub const MAX_COMPACT_BATCH: u32 = 50;

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct ArchivalConfig {
    /// Minimum time a payment must have been observed terminal before compaction.
    pub archival_retention_secs: u64,
    /// Refund eligibility window; compaction never happens inside it.
    pub max_refund_window_secs: u64,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct CompactArchivedPayment {
    pub payment_id: u64,
    pub payer: Address,
    pub merchant: Address,
    pub token: Address,
    pub amount: i128,
    pub status: PaymentStatus,
    pub completed_at: u64,
    pub state_hash: BytesN<32>,
}

pub fn configure(env: &Env, admin: &Address, cfg: &ArchivalConfig) -> Result<(), Error> {
    admin.require_auth();
    let ms: MultiSigConfig = env
        .storage()
        .instance()
        .get(&DataKey::Config(ConfigKey::MultiSigConfig))
        .ok_or(Error::Basic(BasicError::MultiSigNotInitialized))?;
    if !ms.admins.contains(admin) {
        return Err(Error::Basic(BasicError::Unauthorized));
    }
    env.storage().instance().set(&CFG_KEY, cfg);
    Ok(())
}

pub fn get_config(env: &Env) -> Option<ArchivalConfig> {
    env.storage().instance().get(&CFG_KEY)
}

pub fn get_archived(env: &Env, payment_id: u64) -> Option<CompactArchivedPayment> {
    env.storage().instance().get(&(ARCH_KEY, payment_id))
}

fn state_hash(env: &Env, payment: &Payment) -> BytesN<32> {
    env.crypto().sha256(&payment.clone().to_xdr(env)).to_bytes()
}

fn is_terminal(status: &PaymentStatus) -> bool {
    matches!(
        status,
        PaymentStatus::Completed | PaymentStatus::Refunded | PaymentStatus::Cancelled
    )
}

/// Compacts eligible payments and returns how many were compacted. Payments
/// that are missing, non-terminal, still in the retention/refund window, still
/// holding an unfinalized settlement, or already archived are skipped.
pub fn compact_settled_payments(env: &Env, payment_ids: Vec<u64>) -> Result<u32, Error> {
    if payment_ids.len() == 0 || payment_ids.len() > MAX_COMPACT_BATCH {
        return Err(Error::Basic(BasicError::InvalidBatchSize));
    }
    let cfg = get_config(env).unwrap_or(ArchivalConfig {
        archival_retention_secs: 0,
        max_refund_window_secs: 0,
    });
    let required = core::cmp::max(cfg.archival_retention_secs, cfg.max_refund_window_secs);
    let now = env.ledger().timestamp();
    let mut compacted: u32 = 0;

    for payment_id in payment_ids.iter() {
        let pkey = DataKey::Payment(PaymentKey::Data(payment_id));
        let payment: Payment = match env.storage().instance().get(&pkey) {
            Some(p) => p,
            None => continue,
        };
        if !is_terminal(&payment.status) || get_archived(env, payment_id).is_some() {
            continue;
        }
        // Funds still held for the merchant: keep the full record.
        let held = env
            .storage()
            .instance()
            .has(&DataKey::Payment(PaymentKey::PendingSettlement(payment_id)))
            && !env
                .storage()
                .instance()
                .has(&DataKey::State(StateDataKey::SettlementFinalized(payment_id)));
        if held {
            continue;
        }

        let seen: Option<u64> = env.storage().instance().get(&(SEEN_KEY, payment_id));
        let seen_at = match seen {
            Some(t) => t,
            None => {
                env.storage().instance().set(&(SEEN_KEY, payment_id), &now);
                if required > 0 {
                    continue;
                }
                now
            }
        };
        if now.saturating_sub(seen_at) < required {
            continue;
        }

        let record = CompactArchivedPayment {
            payment_id,
            payer: payment.customer.clone(),
            merchant: payment.merchant.clone(),
            token: payment.token.clone(),
            amount: payment.amount,
            status: payment.status.clone(),
            completed_at: seen_at,
            state_hash: state_hash(env, &payment),
        };
        env.storage().instance().set(&(ARCH_KEY, payment_id), &record);
        // Issue #243: commit the archived payment to the Merkle accumulator.
        crate::storage::append_archived_leaf(env, record.state_hash.clone());

        // Replace the verbose record with a stub that keeps the guard fields.
        let mut stub = payment;
        stub.metadata = String::from_str(env, "");
        stub.notes = String::from_str(env, "");
        env.storage().instance().set(&pkey, &stub);

        let inst = env.storage().instance();
        inst.remove(&DataKey::Payment(PaymentKey::Metadata(payment_id)));
        inst.remove(&DataKey::Payment(PaymentKey::Memo(payment_id)));
        inst.remove(&(SEEN_KEY, payment_id));
        env.storage()
            .persistent()
            .remove(&DataKey::Payment(PaymentKey::Tag(payment_id)));
        compacted += 1;
    }
    Ok(compacted)
}

/// Returns true when `full_payment` hashes to the on-chain commitment for
/// `payment_id`. Lets off-chain indexers prove historical fields.
pub fn verify_archived_payment_blob(env: &Env, payment_id: u64, full_payment: Payment) -> bool {
    match get_archived(env, payment_id) {
        Some(rec) => full_payment.id == payment_id && state_hash(env, &full_payment) == rec.state_hash,
        None => false,
    }
}
