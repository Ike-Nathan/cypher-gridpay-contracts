// Core types and data structures for the payment contract

use soroban_sdk::{contracttype, Address, Bytes, BytesN, String, Vec};

use crate::storage::MerchantDataKey;

// ============================================================================
// ENUMS
// ============================================================================

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum Currency {
    XLM,
    USDC,
    USDT,
    BTC,
    ETH,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub enum PayoutFrequency {
    Immediate,
    Daily,
    Weekly,
    Monthly,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum PaymentStatus {
    Pending,
    Completed,
    Refunded,
    PartialRefunded,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum SubscriptionStatus {
    Active,
    Paused,
    Cancelled,
    Expired,
    InDunning,
    Suspended,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum ConditionType {
    TimestampAfter(u64),
    TimestampBefore(u64),
    OraclePrice(Address, String, i128, PriceComparison),
    CrossContractState(Address, BytesN<32>),
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum PriceComparison {
    GreaterThan,
    LessThan,
    EqualTo,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub enum MerchantVerificationLevel {
    Unverified,
    Basic,
    Standard,
    Premium,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum ActionType {
    ReleaseEscrow,
    ResolveDispute,
    CompletePayment,
    RefundPayment,
    AddAdmin,
    RemoveAdmin,
    UpdateRequiredSignatures,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub enum FeeTier {
    Standard,
    Premium,    // >= configured premium volume
    Enterprise, // >= configured enterprise volume
}

// ============================================================================
// TRIAL AND PAUSE DATA
// ============================================================================

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct SubscriptionTrialData {
    pub period_seconds: u64,
    pub ends_at: u64,
    pub converted: bool,
    pub locked_conversion_amount: i128,
    pub notice_emitted_at: Option<u64>,
    pub min_notice_lead_secs: u64,
    pub post_trial_cancel_grace_secs: u64,
    pub conversion_charge_amount: Option<i128>,
    pub conversion_charged_at: Option<u64>,
    pub post_conversion_usage_units: u64,
    pub conversion_charge_reversed: bool,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct SubscriptionPauseData {
    pub last_paused_at: u64,
    pub total_pause_duration: u64,
    pub proration_enabled: bool,
}

// ============================================================================
// SUBSCRIPTION TYPES
// ============================================================================

#[derive(Clone)]
#[contracttype]
pub struct Subscription {
    pub id: u64,
    pub customer: Address,
    pub merchant: Address,
    pub amount: i128,
    pub token: Address,
    pub currency: Currency,
    pub interval: u64, // seconds between payments
    pub duration: u64, // total seconds the subscription lives (0 = indefinite)
    pub status: SubscriptionStatus,
    pub created_at: u64,
    pub next_payment_at: u64,
    pub ends_at: u64,       // 0 = no hard end
    pub payment_count: u64, // successful executions so far
    pub retry_count: u64,   // consecutive failed attempts on current cycle
    pub max_retries: u64,   // max retries before marking failed cycle skipped
    pub metadata: String,
    pub trial_data: SubscriptionTrialData,
    pub pause_data: SubscriptionPauseData,
}

#[derive(Clone)]
#[contracttype]
pub struct DunningConfig {
    pub initial_backoff_seconds: u64,
    pub max_retries: u32,
}

#[derive(Clone)]
#[contracttype]
pub struct DunningState {
    pub subscription_id: u64,
    pub retry_count: u32,
    pub next_retry_at: u64,
    pub backoff_seconds: u64,
    pub max_retries: u32,
    pub last_failed_at: u64,
}

// ============================================================================
// PAYMENT TYPES
// ============================================================================

#[derive(Clone)]
#[contracttype]
pub struct Payment {
    pub id: u64,
    pub customer: Address,
    pub merchant: Address,
    pub amount: i128,
    pub token: Address,
    pub currency: Currency,
    pub status: PaymentStatus,
    pub created_at: u64,
    pub expires_at: u64,
    pub metadata: String,
    pub notes: String,
    pub refunded_amount: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct PartialPaymentRecord {
    pub payment_id: u64,
    pub installment_number: u32,
    pub amount_paid: i128,
    pub total_amount: i128,
    pub remaining: i128,
    pub paid_at: u64,
    pub payer: Address,
}

#[derive(Clone)]
#[contracttype]
pub struct PayoutSchedule {
    pub merchant: Address,
    pub token: Address,
    pub frequency: PayoutFrequency,
    pub next_payout_at: u64,
    pub accumulated: i128,
}

// ============================================================================
// RATE LIMITING AND VERIFICATION
// ============================================================================

#[derive(Clone)]
#[contracttype]
pub struct RateLimitConfig {
    pub max_payments_per_window: u32,
    pub window_duration: u64,
    pub max_payment_amount: i128,
    pub max_daily_volume: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct AddressRateLimit {
    pub address: Address,
    pub payment_count: u32,
    pub window_start: u64,
    pub daily_volume: i128,
    pub last_payment_at: u64,
    pub flagged: bool,
}

#[derive(Clone)]
#[contracttype]
pub struct MerchantRateLimit {
    pub merchant: Address,
    pub max_transactions_per_hour: u32,
    pub max_amount_per_hour: i128,
    pub current_transactions: u32,
    pub current_amount: i128,
    pub window_start: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct VerificationTierLimits {
    pub level: MerchantVerificationLevel,
    pub tx_per_period: u32,
    pub volume_limit: i128,
}

// ============================================================================
// ESCROW AND CONDITIONAL PAYMENTS
// ============================================================================

#[derive(Clone)]
#[contracttype]
pub struct EscrowedPayment {
    pub payment_id: u64,
    pub escrow_id: u64,
    pub escrow_contract: Address,
    pub auto_release_on_complete: bool,
}

#[derive(Clone)]
#[contracttype]
pub struct EscrowedPaymentDispute {
    pub payment_id: u64,
    pub raised_by: Address,
    pub reason: String,
    pub raised_at: u64,
    pub resolved: bool,
    pub resolved_at: Option<u64>,
    pub favor_customer: Option<bool>,
}

#[derive(Clone)]
#[contracttype]
pub struct ConditionalPayment {
    pub payment_id: u64,
    pub condition: ConditionType,
    pub condition_met: bool,
    pub evaluated_at: Option<u64>,
}

#[derive(Clone)]
#[contracttype]
pub struct AutoEscrowRule {
    pub merchant: Address,
    pub escrow_bps: u32,
    pub min_amount: i128,
    pub token: Address,
    pub active: bool,
    pub escrow_contract: Address,
}

#[derive(Clone)]
#[contracttype]
pub struct ScheduledPayment {
    pub payment_id: u64,
    pub customer: Address,
    pub merchant: Address,
    pub token: Address,
    pub amount: i128,
    pub scheduled_at: u64,
    pub executed: bool,
    pub cancelled: bool,
}

// ============================================================================
// ORACLE AND CONFIGURATION
// ============================================================================

#[derive(Clone)]
#[contracttype]
pub struct OracleRateConfig {
    pub oracle_address: Address,
    pub currency: Currency,
    pub price_feed_id: BytesN<32>,
    pub max_staleness_seconds: u64,
    pub enabled: bool,
}

#[derive(Clone)]
#[contracttype]
pub struct RiskFeeConfig {
    pub base_fee_bps: u32,
    pub large_amount_threshold: i128,
    pub large_amount_surcharge_bps: u32,
    pub new_customer_surcharge_bps: u32,
    pub high_risk_currency_surcharge: u32,
}

#[derive(Clone)]
#[contracttype]
pub struct FinalityConfig {
    pub settlement_delay_seconds: u64,
    pub active: bool,
}

// ============================================================================
// ANALYTICS
// ============================================================================

#[derive(Clone)]
#[contracttype]
pub struct AnalyticsBucket {
    pub bucket_start: u64,
    pub bucket_end: u64,
    pub total_payments: u64,
    pub total_volume: i128,
    pub total_refunds: i128,
    pub failed_count: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct PaymentAnalytics {
    pub total_payments: u64,
    pub total_volume: i128,
    pub average_payment: i128,
    pub failed_payments: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct MerchantAnalytics {
    pub merchant: Address,
    pub total_revenue: i128,
    pub payment_count: u64,
    pub avg_payment_amount: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct CustomerAnalytics {
    pub customer: Address,
    pub total_spent: i128,
    pub payment_count: u64,
    pub avg_payment_amount: i128,
}

// ============================================================================
// PAUSE STATE
// ============================================================================

#[derive(Clone)]
#[contracttype]
pub struct PauseState {
    pub contract_paused: bool,
    pub paused_at: u64,
    pub paused_functions: Vec<String>,
}

#[derive(Clone)]
#[contracttype]
pub struct PauseHistory {
    pub paused_at: u64,
    pub paused_until: u64,
    pub paused_functions: Vec<String>,
    pub reason: String,
}

// ============================================================================
// LOYALTY AND DISCOUNTS
// ============================================================================

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct LoyaltyConfig {
    pub points_per_unit: u32,
    pub redemption_rate: u32,
    pub expiry_seconds: u64,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct CustomerLoyaltyBalance {
    pub customer: Address,
    pub points: u64,
    pub last_updated: u64,
    pub expires_at: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct CustomerSpendLimit {
    pub customer: Address,
    pub daily_limit: i128,
    pub monthly_limit: i128,
    pub daily_spent: i128,
    pub monthly_spent: i128,
    pub daily_reset_at: u64,
    pub monthly_reset_at: u64,
}

// ============================================================================
// MULTISIG AND PROPOSALS
// ============================================================================

#[derive(Clone)]
#[contracttype]
pub struct MultiSigConfig {
    pub admins: Vec<Address>,
    pub required_signatures: u32,
    pub total_admins: u32,
    pub proposal_ttl: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct AdminProposal {
    pub id: String,
    pub proposer: Address,
    pub action_type: ActionType,
    pub target: Address,
    pub data: Bytes,
    pub approvals: Vec<Address>,
    pub approval_count: u32,
    pub executed: bool,
    pub rejected: bool,
    pub created_at: u64,
    pub expires_at: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct LargePaymentProposal {
    pub payment_id: u64,
    pub approvals: Vec<Address>,
    pub required: u32,
    pub proposed_at: u64,
    pub expires_at: u64,
    pub executed: bool,
}

// ============================================================================
// BATCH OPERATIONS
// ============================================================================

#[derive(Clone)]
#[contracttype]
pub struct BatchPaymentEntry {
    pub customer: Address,
    pub merchant: Address,
    pub amount: i128,
    pub token: Address,
    pub currency: Currency,
    pub expiration_duration: u64,
    pub metadata: String,
}

#[derive(Clone)]
#[contracttype]
pub struct BatchResult {
    pub payment_id: u64,
    pub success: bool,
    pub error_code: Option<u32>,
}

// ============================================================================
// FEES
// ============================================================================

#[derive(Clone)]
#[contracttype]
pub struct FeeConfig {
    pub fee_bps: u32,
    pub min_fee: i128,
    pub max_fee: i128,
    pub treasury: Address,
    pub fee_token: Address,
    pub active: bool,
}

#[derive(Clone)]
#[contracttype]
pub struct MerchantFeeRecord {
    pub merchant: Address,
    pub total_fees_paid: i128,
    pub total_volume: i128,
    pub fee_tier: FeeTier,
    /// Volume baseline set on manual tier downgrade; automatic upgrades only
    /// consider volume earned after this point.
    pub tier_volume_baseline: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct FeeWaiver {
    pub merchant: Address,
    pub waiver_bps: u32, // reduction in basis points
    /// Identifier of the promotional campaign this waiver belongs to.
    pub campaign_id: u64,
    /// Aggregate ceiling on fees the campaign may waive.
    pub max_total_waived_fee: i128,
    /// Fees waived so far under this campaign.
    pub accumulated_waived_fee: i128,
    /// Maximum number of waived transactions per customer address.
    pub max_uses_per_address: u32,
    /// Timestamp from which the waiver becomes active.
    pub valid_from: u64,
    /// Timestamp at which the waiver expires.
    pub valid_until: u64,
    pub reason: String,
    pub granted_by: Address,
}

#[derive(Clone)]
#[contracttype]
pub struct FeeRebateConfig {
    pub threshold_volume: i128,
    pub rebate_bps: u32,
    pub rebate_period_seconds: u64,
    pub active: bool,
}

#[derive(Clone)]
#[contracttype]
pub struct MerchantRebateAccrual {
    pub merchant: Address,
    pub accrued_rebate: i128,
    pub period_start: u64,
    pub period_volume: i128,
}

// ============================================================================
// PAYMENT CHANNEL AND SPLITS
// ============================================================================

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PaymentChannel {
    pub channel_id: u64,
    pub customer: Address,
    pub merchant: Address,
    pub token: Address,
    pub deposited: i128,
    pub settled: i128,
    pub settled_nonce: u64,
    pub open: bool,
    pub expires_at: u64,
    pub customer_pk: BytesN<32>,
}

#[derive(Clone)]
#[contracttype]
pub struct SplitRecipient {
    pub address: Address,
    pub share_bps: u32,
}

#[derive(Clone)]
#[contracttype]
pub struct PaymentSplitConfig {
    pub payment_id: u64,
    pub recipients: Vec<SplitRecipient>,
    pub executed: bool,
}

#[derive(Clone)]
#[contracttype]
pub struct FeeSweepRecord {
    pub sweep_id: u64,
    pub token: Address,
    pub amount: i128,
    pub recipient: Address,
    pub swept_at: u64,
}

// ============================================================================
// METERED SUBSCRIPTIONS
// ============================================================================

#[derive(Clone)]
#[contracttype]
pub struct MeteredSubscription {
    pub subscription_id: u64,
    pub merchant: Address,
    pub customer: Address,
    pub token: Address,
    pub price_per_unit: i128,
    pub unit_name: String,
    pub accumulated_units: u64,
    pub billing_cap: Option<i128>,
    pub last_reset_at: u64,
    pub max_units_per_period: Option<u64>,
}

// ============================================================================
// SUBSCRIPTION GROUPS
// ============================================================================

#[derive(Clone)]
#[contracttype]
pub struct SubscriptionGroup {
    pub group_id: u64,
    pub merchant: Address,
    pub created_at: u64,
    pub member_count: u64,
}

// ============================================================================
// PAYMENT FORWARDING AND ROUTING
// ============================================================================

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct PaymentForwardConfig {
    pub merchant: Address,
    pub forward_to: Address,
    pub forward_bps: u32,
    pub active: bool,
}

#[derive(Clone)]
#[contracttype]
pub struct RouteOption {
    pub from: Address,
    pub to: Address,
    pub priority: u32,
    pub active: bool,
    pub fee_bps: u32,
}

// ============================================================================
// INVOICING
// ============================================================================

#[derive(Clone)]
#[contracttype]
pub struct LineItem {
    pub description: String,
    pub quantity: u64,
    pub unit_price: i128,
    pub amount: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct PaymentInvoice {
    pub invoice_id: u64,
    pub payment_id: u64,
    pub items: Vec<LineItem>,
    pub subtotal: i128,
    pub tax: i128,
    pub total: i128,
    pub issued_at: u64,
}

// ============================================================================
// PAYMENT METADATA AND MEMOS
// ============================================================================

#[derive(Clone)]
#[contracttype]
pub struct PaymentMetadata {
    pub payment_id: u64,
    pub key: String,
    pub value: String,
}

#[derive(Clone)]
#[contracttype]
pub struct PaymentMemo {
    pub payment_id: u64,
    pub version: u32,
    pub memo: String,
    pub updated_at: u64,
}

// ============================================================================
// PLACEHOLDER STRUCT FOR CONTRACT MARKER
// ============================================================================

#[derive(Clone)]
pub struct PaymentContract;
