// Event structures for the payment contract
// All events use #[contractevent] for Soroban compatibility

use soroban_sdk::{contractevent, Address};

use crate::types::{ActionType, FeeTier};

// ============================================================================
// PAYMENT EVENTS
// ============================================================================

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentCreated {
    pub payment_id: u64,
    pub customer: Address,
    pub merchant: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentCompleted {
    pub payment_id: u64,
    pub merchant: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentRefunded {
    pub payment_id: u64,
    pub customer: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentCancelled {
    pub payment_id: u64,
    pub cancelled_by: Address,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentExpired {
    pub payment_id: u64,
    pub customer: Address,
    pub refunded_amount: i128,
    pub expired_at: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallmentPaid {
    pub payment_id: u64,
    pub installment_number: u32,
    pub amount: i128,
    pub remaining: i128,
    pub payer: Address,
    pub paid_at: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentFullyPaid {
    pub payment_id: u64,
    pub total_installments: u32,
    pub completed_at: u64,
}

// ============================================================================
// ESCROW EVENTS
// ============================================================================

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowedPaymentCreated {
    pub payment_id: u64,
    pub escrow_id: u64,
    pub escrow_contract: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowedPaymentCompleted {
    pub payment_id: u64,
    pub escrow_id: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowedPaymentCancelled {
    pub payment_id: u64,
    pub escrow_id: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowedPaymentDisputed {
    pub payment_id: u64,
    pub raised_by: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentDisputeResolved {
    pub payment_id: u64,
    pub favor_customer: bool,
}

// ============================================================================
// SUBSCRIPTION EVENTS
// ============================================================================

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubscriptionCreated {
    pub subscription_id: u64,
    pub customer: Address,
    pub merchant: Address,
    pub amount: i128,
    pub interval: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecurringPaymentExecuted {
    pub subscription_id: u64,
    pub payment_count: u64,
    pub amount: i128,
    pub next_payment_at: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecurringPaymentFailed {
    pub subscription_id: u64,
    pub retry_count: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubscriptionCancelled {
    pub subscription_id: u64,
    pub cancelled_by: Address,
}

// ============================================================================
// RISK AND FEES EVENTS
// ============================================================================

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RiskFeeApplied {
    pub payment_id: u64,
    pub base_fee_bps: u32,
    pub risk_surcharge_bps: u32,
    pub total_fee_bps: u32,
}

// ============================================================================
// PAYMENT CHANNEL EVENTS
// ============================================================================

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelOpened {
    pub channel_id: u64,
    pub customer: Address,
    pub merchant: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelToppedUp {
    pub channel_id: u64,
    pub amount: i128,
    pub new_deposited: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelSettled {
    pub channel_id: u64,
    pub merchant_amount: i128,
    pub customer_refund: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelExpiredClosed {
    pub channel_id: u64,
    pub refunded_to: Address,
}

// ============================================================================
// METERED BILLING EVENTS
// ============================================================================

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsageReported {
    pub subscription_id: u64,
    pub units: u64,
    pub accumulated: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MeteredBillingExecuted {
    pub subscription_id: u64,
    pub amount: i128,
    pub units_billed: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BillingCapReached {
    pub subscription_id: u64,
    pub cap: i128,
}

// ============================================================================
// SUBSCRIPTION PAUSE/RESUME EVENTS
// ============================================================================

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubscriptionPaused {
    pub subscription_id: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubscriptionResumed {
    pub subscription_id: u64,
    pub next_payment_at: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubscriptionResumedProrated {
    pub subscription_id: u64,
    pub pause_duration: u64,
    pub new_next_billing_date: u64,
    pub prorated_amount: i128,
}

// ============================================================================
// TRIAL EVENTS
// ============================================================================

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrialStarted {
    pub subscription_id: u64,
    pub trial_ends_at: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrialConverted {
    pub subscription_id: u64,
    pub converted_at: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrialCancelled {
    pub subscription_id: u64,
    pub cancelled_at: u64,
}

// ============================================================================
// ADDRESS FLAGGING EVENTS
// ============================================================================

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AddressFlagged {
    pub address: Address,
    pub reason: soroban_sdk::String,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AddressUnflagged {
    pub address: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RateLimitBreached {
    pub address: Address,
    pub payment_count: u32,
}

// ============================================================================
// DUNNING EVENTS
// ============================================================================

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubscriptionEnteredDunning {
    pub subscription_id: u64,
    pub attempt: u32,
    pub next_retry_at: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DunningRetryScheduled {
    pub subscription_id: u64,
    pub retry_at: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubscriptionSuspended {
    pub subscription_id: u64,
    pub reason: soroban_sdk::String,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DunningResolved {
    pub subscription_id: u64,
    pub resolved_at: u64,
}

// ============================================================================
// PAUSE STATE EVENTS
// ============================================================================

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractPausedEvent {
    pub paused_at: u64,
    pub reason: soroban_sdk::String,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractUnpausedEvent {
    pub unpaused_at: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionPausedEvent {
    pub function_name: soroban_sdk::String,
    pub paused_at: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionUnpausedEvent {
    pub function_name: soroban_sdk::String,
    pub unpaused_at: u64,
}

// ============================================================================
// ESCROW AND AUTOMATION EVENTS
// ============================================================================

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutoEscrowTriggered {
    pub payment_id: u64,
    pub merchant: Address,
    pub escrow_amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LargePaymentProposed {
    pub payment_id: u64,
    pub proposer: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LargePaymentApproved {
    pub payment_id: u64,
    pub approver: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LargePaymentExecuted {
    pub payment_id: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LargePaymentThresholdUpdated {
    pub old_threshold: i128,
    pub new_threshold: i128,
}

// ============================================================================
// METADATA AND MEMO EVENTS
// ============================================================================

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentMetadataSet {
    pub payment_id: u64,
    pub key: soroban_sdk::String,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentMetadataUpdated {
    pub payment_id: u64,
    pub key: soroban_sdk::String,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentMemoSet {
    pub payment_id: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentMemoUpdated {
    pub payment_id: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentMemoVerified {
    pub payment_id: u64,
}

// ============================================================================
// PAYMENT FORWARD EVENTS
// ============================================================================

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentForwardConfigSet {
    pub merchant: Address,
    pub forward_to: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentForwardConfigRemoved {
    pub merchant: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentForwarded {
    pub payment_id: u64,
    pub forwarded_amount: i128,
    pub forwarded_to: Address,
}

// ============================================================================
// MULTISIG AND ADMIN EVENTS
// ============================================================================

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionProposed {
    pub proposal_id: soroban_sdk::String,
    pub proposer: Address,
    pub action_type: ActionType,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionApproved {
    pub proposal_id: soroban_sdk::String,
    pub approver: Address,
    pub approval_count: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionExecuted {
    pub proposal_id: soroban_sdk::String,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionRejected {
    pub proposal_id: soroban_sdk::String,
    pub rejected_by: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminAdded {
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminRemoved {
    pub admin: Address,
}

// ============================================================================
// FEE EVENTS
// ============================================================================

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeCollected {
    pub payment_id: u64,
    pub fee_amount: i128,
    pub merchant: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeesWithdrawn {
    pub amount: i128,
    pub treasury: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MerchantTierUpgraded {
    pub merchant: Address,
    pub old_tier: FeeTier,
    pub new_tier: FeeTier,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeWaiverGranted {
    pub merchant: Address,
    pub waiver_bps: u32,
    pub valid_until: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeWaiverRevoked {
    pub merchant: Address,
    pub revoked_by: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeWaiverExpired {
    pub merchant: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeConfigUpdated {
    pub new_base_fee_bps: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MerchantRebateAccrual {
    pub merchant: Address,
    pub accrued_amount: i128,
}

// ============================================================================
// CONDITIONAL PAYMENT EVENTS
// ============================================================================

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConditionEvaluated {
    pub payment_id: u64,
    pub met: bool,
    pub evaluated_at: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConditionalPaymentCreated {
    pub payment_id: u64,
    pub condition_type: soroban_sdk::String,
}
