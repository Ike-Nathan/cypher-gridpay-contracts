//! Context-aware reentrancy guard (transient, in temporary storage).
//!
//! The Soroban host already refuses to let a contract be re-entered within
//! one call stack, so a malicious token or hook that calls back into this
//! contract during a transfer traps in the host. This guard is a second,
//! contract-level line of defence that does not depend on that host rule:
//!
//! * [`ExternalEntry`] — taken by every fund-moving entry point. It sets
//!   `TemporaryKey::ReentrancyLock` and clears it when dropped (on every
//!   return path; a trap reverts the write anyway). A second acquisition
//!   while the lock is held is rejected as a reentrant call.
//! * Trusted sibling callbacks — while a call into a registered sibling
//!   contract is in flight, [`expect_callback`] records which callback is
//!   legitimate ([`CallContextState`]) and from whom. [`accept_callback`]
//!   lets a callback through only if the caller is that sibling and the
//!   state matches; anything else is rejected. [`end_callback`] resets it.
//!
//! The contract maps the `Err(())`s here to its own `ReentrantCallDetected`.
use soroban_sdk::{contracttype, Address, Env};

/// Keys in temporary storage (transient: they never need to outlive a call).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TemporaryKey {
    ReentrancyLock,
    CallContext,
}

/// Which synchronous callback from a sibling contract is expected right now.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CallContextState {
    Idle,
    AwaitingEscrowCallback(Address),
    AwaitingRefundCallback(Address),
    AwaitingPaymentCallback(Address),
}

/// Ledgers the transient entries are kept; they are removed on exit anyway.
const TRANSIENT_TTL: u32 = 100;

/// Held for the duration of a fund-moving external entry point.
/// Holds its own (cheap) clone of the `Env` so the entry point can still
/// move its `env` while the guard is alive.
pub struct ExternalEntry {
    env: Env,
}

impl ExternalEntry {
    /// Takes the lock, or `Err(())` if it is already held (a reentrant call).
    pub fn acquire(env: &Env) -> Result<Self, ()> {
        if is_locked(env) {
            return Err(());
        }
        let storage = env.storage().temporary();
        storage.set(&TemporaryKey::ReentrancyLock, &true);
        storage.extend_ttl(&TemporaryKey::ReentrancyLock, TRANSIENT_TTL, TRANSIENT_TTL);
        Ok(ExternalEntry { env: env.clone() })
    }
}

impl Drop for ExternalEntry {
    fn drop(&mut self) {
        self.env.storage().temporary().remove(&TemporaryKey::ReentrancyLock);
        self.env.storage().temporary().remove(&TemporaryKey::CallContext);
    }
}

/// Whether an external entry point is currently executing.
pub fn is_locked(env: &Env) -> bool {
    env.storage()
        .temporary()
        .get::<_, bool>(&TemporaryKey::ReentrancyLock)
        .unwrap_or(false)
}

/// Records the callback a sibling may make while a call to it is in flight.
pub fn expect_callback(env: &Env, state: CallContextState) {
    let storage = env.storage().temporary();
    storage.set(&TemporaryKey::CallContext, &state);
    storage.extend_ttl(&TemporaryKey::CallContext, TRANSIENT_TTL, TRANSIENT_TTL);
}

/// The callback currently expected, if any.
pub fn current_context(env: &Env) -> CallContextState {
    env.storage()
        .temporary()
        .get(&TemporaryKey::CallContext)
        .unwrap_or(CallContextState::Idle)
}

/// Lets a sibling callback through only if `caller` is the sibling recorded
/// for exactly `expected`'s kind of callback; `Err(())` otherwise.
pub fn accept_callback(env: &Env, expected: &CallContextState, caller: &Address) -> Result<(), ()> {
    let current = current_context(env);
    let sibling = match (&current, expected) {
        (CallContextState::AwaitingEscrowCallback(a), CallContextState::AwaitingEscrowCallback(_))
        | (CallContextState::AwaitingRefundCallback(a), CallContextState::AwaitingRefundCallback(_))
        | (CallContextState::AwaitingPaymentCallback(a), CallContextState::AwaitingPaymentCallback(_)) => a,
        _ => return Err(()),
    };
    if sibling != caller {
        return Err(());
    }
    Ok(())
}

/// Clears the expected callback once the sibling call has returned.
pub fn end_callback(env: &Env) {
    env.storage().temporary().remove(&TemporaryKey::CallContext);
}
