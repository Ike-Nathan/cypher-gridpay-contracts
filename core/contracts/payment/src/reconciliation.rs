//! Payment <-> Escrow state reconciliation (#198).
//!
//! An escrow can reach a terminal state directly on the escrow contract
//! (arbitration, clawback, expiry refund) without the payment contract being
//! told. In this contract a payment backed by an escrow bridge record stays
//! `Pending` until `complete_escrowed_payment` / `cancel_escrowed_payment` /
//! `resolve_escrowed_payment_dispute` runs, so such a payment can be stuck.
//! `sync_escrowed_payment_state` is a permissionless entry point that reads the
//! escrow's public view functions and applies the matching transition once.
//!
//! Notes on the existing data model:
//! - `Escrow` carries no `payment_id`; identity is established through the
//!   stored `EscrowedPayment` bridge mapping plus a customer/merchant/amount
//!   equality check against the escrow.
//! - `PaymentStatus` has no `Escrowed`/`Disputed` variants. `Pending` plus a
//!   bridge record is the "escrowed" state; an escrow-side dispute is mirrored
//!   as an unresolved `EscrowedPaymentDispute`, which already blocks
//!   `complete_escrowed_payment`.
//! - The idempotency flag lives in its own storage key instead of a new
//!   `EscrowedPayment` field so existing stored bridge records still decode.
//! - Fees: the escrow contract charges its own fee on release; this module
//!   never touches funds or the payment fee ledger, so no fee can be taken twice.

use escrow::{EscrowContractClient, EscrowStatus};
use soroban_sdk::{contractevent, symbol_short, Address, Env, String, Symbol};

use crate::{
    CustomerAnalytics, CustomerDataKey, DataKey, Error, EscrowedPayment, EscrowedPaymentDispute,
    FeatureError, MerchantAnalytics, MerchantDataKey, Payment, PaymentContract, PaymentError,
    PaymentKey, PaymentStatus, StateDataKey,
};

const RECONCILED_KEY: Symbol = symbol_short!("esc_rec");

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowStateReconciled {
    pub payment_id: u64,
    pub escrow_id: u64,
    pub new_status: PaymentStatus,
}

pub fn is_reconciled(env: &Env, payment_id: u64) -> bool {
    env.storage()
        .instance()
        .get(&(RECONCILED_KEY, payment_id))
        .unwrap_or(false)
}

pub fn sync_escrowed_payment_state(env: &Env, payment_id: u64) -> Result<PaymentStatus, Error> {
    PaymentContract::require_not_paused(env, "sync_escrowed_payment_state")?;
    let pkey = DataKey::Payment(PaymentKey::Data(payment_id));
    let mut payment: Payment = env
        .storage()
        .instance()
        .get(&pkey)
        .ok_or(Error::Payment(PaymentError::NotFound))?;
    let bridge: EscrowedPayment = env
        .storage()
        .instance()
        .get(&DataKey::State(StateDataKey::EscrowedPayment(payment_id)))
        .ok_or(Error::Feature(FeatureError::EscrowMappingNotFound))?;

    // Already terminal or already reconciled: nothing to apply (idempotent).
    if payment.status != PaymentStatus::Pending || is_reconciled(env, payment_id) {
        return Ok(payment.status);
    }

    let client = EscrowContractClient::new(env, &bridge.escrow_contract);
    let bridge_err = || Error::Feature(FeatureError::EscrowBridgeFailed);
    let status = match client.try_get_escrow_status(&bridge.escrow_id) {
        Ok(Ok(s)) => s,
        _ => return Err(bridge_err()),
    };
    let (customer, merchant): (Address, Address) =
        match client.try_get_escrow_parties(&bridge.escrow_id) {
            Ok(Ok(p)) => p,
            _ => return Err(bridge_err()),
        };
    let escrow_amount = match client.try_get_escrow_amount(&bridge.escrow_id) {
        Ok(Ok(a)) => a,
        _ => return Err(bridge_err()),
    };
    if customer != payment.customer || merchant != payment.merchant || escrow_amount != payment.amount
    {
        return Err(Error::Feature(FeatureError::EscrowBridgeFailed));
    }

    let new_status = match status {
        EscrowStatus::Released => PaymentStatus::Completed,
        EscrowStatus::ExpiredRefunded => PaymentStatus::Refunded,
        EscrowStatus::Cancelled => PaymentStatus::Cancelled,
        EscrowStatus::Disputed => {
            // Mirror the escrow-side dispute so completion stays blocked.
            let dkey = DataKey::State(StateDataKey::EscrowedPaymentDispute(payment_id));
            if !env.storage().instance().has(&dkey) {
                env.storage().instance().set(
                    &dkey,
                    &EscrowedPaymentDispute {
                        payment_id,
                        raised_by: payment.customer.clone(),
                        reason: String::from_str(env, "reconciled from escrow dispute"),
                        raised_at: env.ledger().timestamp(),
                        resolved: false,
                        resolved_at: None,
                        favor_customer: None,
                    },
                );
            }
            return Ok(payment.status);
        }
        // Locked: still live. Resolved: the outcome (who was paid) is not
        // exposed by the escrow views, so it is left to the explicit
        // dispute-resolution path rather than guessed here.
        _ => return Ok(payment.status),
    };

    payment.status = new_status.clone();
    env.storage().instance().set(&pkey, &payment);
    env.storage().instance().set(&(RECONCILED_KEY, payment_id), &true);
    apply_analytics(env, &payment, &new_status);

    EscrowStateReconciled {
        payment_id,
        escrow_id: bridge.escrow_id,
        new_status: new_status.clone(),
    }
    .publish(env);
    Ok(new_status)
}

fn apply_analytics(env: &Env, payment: &Payment, status: &PaymentStatus) {
    let mkey = DataKey::Merchant(MerchantDataKey::Analytics(payment.merchant.clone()));
    let mut m: MerchantAnalytics =
        env.storage()
            .instance()
            .get(&mkey)
            .unwrap_or(MerchantAnalytics {
                total_payments: 0,
                total_volume: 0,
                total_completed: 0,
                total_cancelled: 0,
                total_refunded: 0,
                total_refunded_volume: 0,
            });
    match status {
        PaymentStatus::Completed => m.total_completed += 1,
        PaymentStatus::Cancelled => m.total_cancelled += 1,
        PaymentStatus::Refunded => {
            m.total_refunded += 1;
            m.total_refunded_volume += payment.amount;
            let ckey = DataKey::Customer(CustomerDataKey::Analytics(payment.customer.clone()));
            let mut c: CustomerAnalytics = env
                .storage()
                .instance()
                .get(&ckey)
                .unwrap_or(PaymentContract::default_customer_analytics());
            c.total_refunds += payment.amount;
            env.storage().instance().set(&ckey, &c);
        }
        _ => {}
    }
    env.storage().instance().set(&mkey, &m);
}
