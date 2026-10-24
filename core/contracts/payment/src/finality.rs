//! Finality delay risk-tier scaling and challenge-based optimistic settlement
//! cancellation (#196).
//!
//! The legacy `FinalityConfig` (static `delay_seconds`) is left untouched so
//! existing deployments and callers keep working. This module adds an optional
//! `FinalityRiskConfig` that scales the delay by payment amount and merchant
//! verification tier, plus an in-window fraud-flag hook that freezes a pending
//! settlement until it is cleared or resolved by an admin.

use soroban_sdk::{contractevent, contracttype, symbol_short, token, Address, Env, Symbol};

use crate::{
    DataKey, Error, MerchantVerificationLevel, MultiSigConfig, Payment,
    PaymentError, PaymentKey, PendingSettlement, StateDataKey, BasicError, ConfigKey,
};

const BPS_DENOM: u128 = 10_000;
const RISK_CFG_KEY: Symbol = symbol_short!("fin_risk");
const FLAG_KEY: Symbol = symbol_short!("fin_flag");

/// Risk-tier parameters layered on top of the static `FinalityConfig`.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct FinalityRiskConfig {
    /// Base delay applied to every held settlement. `0` falls back to the
    /// legacy `FinalityConfig.delay_seconds`.
    pub base_delay_secs: u64,
    /// Payments at or above this amount get the high-value multiplier.
    pub high_value_threshold: i128,
    /// Multiplier in bps applied to the delay for high-value payments
    /// (10_000 = 1x, 20_000 = 2x). Values below 10_000 are treated as 1x.
    pub high_value_multiplier_bps: u32,
    /// Maximum delay discount in bps for a `Premium` merchant. `Standard`
    /// receives two thirds and `Basic` one third of this discount;
    /// `Unverified` receives none. Capped at 10_000.
    pub verified_merchant_discount_bps: u32,
    /// The computed delay is never lower than this floor.
    pub min_delay_floor_secs: u64,
    /// Address allowed to flag payments during their finality window.
    pub risk_oracle: Option<Address>,
    /// Emergency pauser address allowed to flag payments during the window.
    pub emergency_pauser: Option<Address>,
}

/// A fraud challenge recorded against a pending settlement.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct FinalityFlag {
    pub payment_id: u64,
    pub challenger: Address,
    pub reason: Symbol,
    pub flagged_at: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentFlaggedDuringFinality {
    pub payment_id: u64,
    pub challenger: Address,
    pub reason: Symbol,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinalityFlagResolved {
    pub payment_id: u64,
    pub refunded: bool,
}

pub fn get_risk_config(env: &Env) -> Option<FinalityRiskConfig> {
    env.storage().instance().get(&RISK_CFG_KEY)
}

pub fn set_risk_config(env: &Env, cfg: &FinalityRiskConfig) {
    env.storage().instance().set(&RISK_CFG_KEY, cfg);
}

pub fn get_flag(env: &Env, payment_id: u64) -> Option<FinalityFlag> {
    env.storage().instance().get(&(FLAG_KEY, payment_id))
}

pub fn is_flagged(env: &Env, payment_id: u64) -> bool {
    env.storage().instance().has(&(FLAG_KEY, payment_id))
}

/// Computes the finality delay for `payment`. `legacy_delay` is the static
/// `FinalityConfig.delay_seconds`, used when no risk config is stored or when
/// `base_delay_secs` is zero.
pub fn compute_required_finality_delay(env: &Env, payment: &Payment, legacy_delay: u64) -> u64 {
    let cfg = match get_risk_config(env) {
        Some(c) => c,
        None => return legacy_delay,
    };
    let base = if cfg.base_delay_secs == 0 {
        legacy_delay
    } else {
        cfg.base_delay_secs
    };
    let mut delay = base as u128;

    if payment.amount >= cfg.high_value_threshold && cfg.high_value_multiplier_bps > BPS_DENOM as u32
    {
        delay = delay.saturating_mul(cfg.high_value_multiplier_bps as u128) / BPS_DENOM;
    }

    let level: MerchantVerificationLevel = env
        .storage()
        .instance()
        .get(&DataKey::Merchant(crate::MerchantDataKey::VerificationLevel(
            payment.merchant.clone(),
        )))
        .unwrap_or(MerchantVerificationLevel::Unverified);
    let max_discount = core::cmp::min(cfg.verified_merchant_discount_bps as u128, BPS_DENOM);
    let discount_bps = match level {
        MerchantVerificationLevel::Unverified => 0,
        MerchantVerificationLevel::Basic => max_discount / 3,
        MerchantVerificationLevel::Standard => max_discount * 2 / 3,
        MerchantVerificationLevel::Premium => max_discount,
    };
    delay = delay.saturating_mul(BPS_DENOM - discount_bps) / BPS_DENOM;

    let delay = core::cmp::min(delay, u64::MAX as u128) as u64;
    core::cmp::max(delay, cfg.min_delay_floor_secs)
}

fn require_admin(env: &Env, admin: &Address) -> Result<(), Error> {
    admin.require_auth();
    let ms: MultiSigConfig = env
        .storage()
        .instance()
        .get(&DataKey::Config(ConfigKey::MultiSigConfig))
        .ok_or(Error::Basic(BasicError::MultiSigNotInitialized))?;
    if !ms.admins.contains(admin) {
        return Err(Error::Basic(BasicError::Unauthorized));
    }
    Ok(())
}

pub fn configure(env: &Env, admin: &Address, cfg: &FinalityRiskConfig) -> Result<(), Error> {
    require_admin(env, admin)?;
    if cfg.verified_merchant_discount_bps > BPS_DENOM as u32 {
        return Err(Error::Payment(PaymentError::InvalidFinalityRiskConfig));
    }
    set_risk_config(env, cfg);
    Ok(())
}

pub fn flag(env: &Env, payment_id: u64, challenger: &Address, reason: Symbol) -> Result<(), Error> {
    challenger.require_auth();
    let settlement: PendingSettlement = env
        .storage()
        .instance()
        .get(&DataKey::Payment(PaymentKey::PendingSettlement(payment_id)))
        .ok_or(Error::Payment(PaymentError::NotFound))?;
    if env
        .storage()
        .instance()
        .has(&DataKey::State(StateDataKey::SettlementFinalized(payment_id)))
    {
        return Err(Error::Feature(crate::FeatureError::SettlementAlreadyFinalized));
    }
    if env.ledger().timestamp() >= settlement.release_at {
        return Err(Error::Payment(PaymentError::FinalityWindowClosed));
    }
    if is_flagged(env, payment_id) {
        return Err(Error::Payment(PaymentError::PaymentFlagged));
    }

    let payment: Payment = env
        .storage()
        .instance()
        .get(&DataKey::Payment(PaymentKey::Data(payment_id)))
        .ok_or(Error::Payment(PaymentError::NotFound))?;
    let cfg = get_risk_config(env);
    let allowed = *challenger == payment.customer
        || cfg
            .as_ref()
            .map(|c| {
                c.risk_oracle.as_ref() == Some(challenger)
                    || c.emergency_pauser.as_ref() == Some(challenger)
            })
            .unwrap_or(false);
    if !allowed {
        return Err(Error::Payment(PaymentError::NotAuthorizedChallenger));
    }

    let rec = FinalityFlag {
        payment_id,
        challenger: challenger.clone(),
        reason: reason.clone(),
        flagged_at: env.ledger().timestamp(),
    };
    env.storage().instance().set(&(FLAG_KEY, payment_id), &rec);
    PaymentFlaggedDuringFinality {
        payment_id,
        challenger: challenger.clone(),
        reason,
    }
    .publish(env);
    Ok(())
}

/// Admin resolution of a flagged settlement. `refund = true` cancels the
/// optimistic settlement and returns the held amount to the payer;
/// `refund = false` clears the flag so the settlement can be finalized.
pub fn resolve(env: &Env, admin: &Address, payment_id: u64, refund: bool) -> Result<(), Error> {
    require_admin(env, admin)?;
    if !is_flagged(env, payment_id) {
        return Err(Error::Payment(PaymentError::NotFlagged));
    }
    if refund {
        let settlement: PendingSettlement = env
            .storage()
            .instance()
            .get(&DataKey::Payment(PaymentKey::PendingSettlement(payment_id)))
            .ok_or(Error::Payment(PaymentError::NotFound))?;
        let payment: Payment = env
            .storage()
            .instance()
            .get(&DataKey::Payment(PaymentKey::Data(payment_id)))
            .ok_or(Error::Payment(PaymentError::NotFound))?;
        token::Client::new(env, &settlement.token).transfer(
            &env.current_contract_address(),
            &payment.customer,
            &settlement.amount,
        );
        let key = DataKey::Payment(PaymentKey::TotalPendingSettlement(settlement.token.clone()));
        let total: i128 = env.storage().instance().get(&key).unwrap_or(0);
        env.storage()
            .instance()
            .set(&key, &total.saturating_sub(settlement.amount));
        // Mark finalized so the settlement can never be released afterwards.
        env.storage().instance().set(
            &DataKey::State(StateDataKey::SettlementFinalized(payment_id)),
            &true,
        );
    }
    env.storage().instance().remove(&(FLAG_KEY, payment_id));
    FinalityFlagResolved {
        payment_id,
        refunded: refund,
    }
    .publish(env);
    Ok(())
}

