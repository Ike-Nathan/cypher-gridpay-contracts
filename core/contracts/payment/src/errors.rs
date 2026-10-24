// Error types for the payment contract
// Uses a multi-level enum structure to stay within Soroban's 50-variant XDR limit

use soroban_sdk::{contracterror, TryFromVal, Val};

// ============================================================================
// BASIC ERRORS (100-126)
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
#[contracterror]
pub enum BasicError {
    Unauthorized = 100,
    MetadataTooLarge = 101,
    NotesTooLarge = 102,
    InvalidCurrency = 103,
    InvalidBatchSize = 104,
    BatchPartialFailure = 105,
    RateLimitExceeded = 106,
    DailyVolumeExceeded = 107,
    AddressFlagged = 108,
    AddressAlreadyFlagged = 109,
    AmountExceedsLimit = 110,
    MultiSigNotInitialized = 111,
    InsufficientAdmins = 112,
    NotAnAdmin = 113,
    AlreadyApproved = 114,
    OracleCallFailed = 115,
    ContractPaused = 116,
    FunctionPaused = 117,
    InvalidTierThresholds = 118,
    OracleFeedStale = 119,
    OracleNotConfigured = 120,
    InvalidAmount = 121,
    VerificationLevelNotFound = 122,
    TierLimitsNotConfigured = 123,
    InvalidInterval = 124,
    InvalidBps = 125,
    SchemaAlreadyAtTarget = 126,
}

// ============================================================================
// PAYMENT ERRORS (200-224)
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
#[contracterror]
pub enum PaymentError {
    NotFound = 200,
    InvalidStatus = 201,
    AlreadyProcessed = 202,
    Expired = 203,
    NotExpired = 204,
    NoExpiration = 205,
    TransferFailed = 206,
    RefundExceedsPayment = 207,
    NotYetDue = 208,
    ScheduledPaymentCancelled = 209,
    MetadataAlreadySet = 210,
    MetadataNotFound = 211,
    HashMismatch = 212,
    AlreadyFullyPaid = 213,
    InstallmentExceedsRemaining = 214,
    PartialPaymentNotFound = 215,
    MerchantRateLimitExceeded = 216,
    AmountRateLimitExceeded = 217,
    PayoutScheduleNotFound = 218,
    PayoutNotYetDue = 219,
    NothingToSettle = 220,
    BillingOverflow = 221,
    InvalidLineItem = 222,
    InvalidScheduleTime = 223,
    TokenNotAllowed = 224,
}

// ============================================================================
// SUBSCRIPTION ERRORS (300-318)
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
#[contracterror]
pub enum SubscriptionError {
    NotFound = 300,
    NotActive = 301,
    PaymentNotDue = 302,
    MaxRetriesExceeded = 303,
    Ended = 304,
    DunningNotFound = 305,
    NotInDunning = 306,
    RetryNotDue = 307,
    GracePeriodExpired = 308,
    RetryTooEarly = 309,
    MeteredNotFound = 310,
    BillingCapExceeded = 311,
    GroupNotFound = 312,
    AlreadyInGroup = 313,
    GroupSizeLimitExceeded = 314,
    TrialExpired = 315,
    MaxTrialDurationExceeded = 316,
    MerchantPaused = 317,
    UsageCapExceeded = 318,
}

// ============================================================================
// PROPOSAL ERRORS (400-406)
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
#[contracterror]
pub enum ProposalError {
    NotFound = 400,
    Expired = 401,
    AlreadyExecuted = 402,
    ThresholdNotMet = 403,
    RequiresMultiSig = 404,
    InsufficientApprovals = 405,
    ProposalExpired = 406,
}

// ============================================================================
// FEATURE ERRORS (500-541)
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
#[contracterror]
pub enum FeatureError {
    EscrowMappingNotFound = 500,
    EscrowBridgeFailed = 501,
    FeeConfigNotFound = 502,
    InsufficientFees = 503,
    ConditionNotMet = 504,
    ConditionAlreadyEvaluated = 505,
    AutoEscrowRuleNotFound = 506,
    AutoEscrowBelowMinimum = 507,
    AutoEscrowAlreadyTriggered = 508,
    ConditionEvaluationFailed = 509,
    ConditionRuntimeNotMet = 510,
    InvalidFeeConfig = 511,
    ChannelNotFound = 512,
    InvalidSignature = 513,
    InvalidNonce = 514,
    ChannelClosed = 515,
    ChannelExpired = 516,
    ChannelNotExpired = 517,
    InvalidSplitShares = 518,
    TooManyRecipients = 519,
    InvalidCounterparty = 540,
    SplitConfigNotFound = 520,
    SplitAlreadyExecuted = 521,
    LoyaltyNotConfigured = 522,
    InsufficientPoints = 523,
    PointsExpired = 524,
    NothingToSweep = 525,
    SweepRecipientNotSet = 526,
    SpendLimitExceeded = 527,
    SpendLimitNotConfigured = 528,
    SettlementNotReady = 529,
    FinalityConfigNotFound = 530,
    SettlementAlreadyFinalized = 531,
    RebateThresholdNotMet = 532,
    RebateAlreadyClaimed = 533,
    RebateConfigNotFound = 534,
    ForwardConfigNotFound = 535,
    ForwardLoop = 536,
    InvalidForwardBps = 537,
    SenderIsRecipient = 538,
    BelowMinSplitAmount = 539,
    // Issue #385: claimed settlement amounts must sum exactly to the channel deposit.
    BalanceSumMismatch = 541,
}

// ============================================================================
// AGGREGATED ERROR ENUM
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Error {
    Basic(BasicError),
    Payment(PaymentError),
    Subscription(SubscriptionError),
    Proposal(ProposalError),
    Feature(FeatureError),
}

impl Error {
    /// Converts this error variant to its raw `u32` discriminant for Soroban's
    /// `InvokeError` compatibility.
    ///
    /// # Returns
    /// The `u32` discriminant corresponding to this error variant.
    pub fn to_u32(&self) -> u32 {
        match self {
            Error::Basic(e) => *e as u32,
            Error::Payment(e) => *e as u32,
            Error::Subscription(e) => *e as u32,
            Error::Proposal(e) => *e as u32,
            Error::Feature(e) => *e as u32,
        }
    }
}

// ============================================================================
// ERROR CONVERSIONS
// ============================================================================

impl From<Error> for soroban_sdk::Error {
    fn from(e: Error) -> Self {
        soroban_sdk::Error::from_contract_error(e.to_u32())
    }
}

impl From<&Error> for soroban_sdk::Error {
    fn from(e: &Error) -> Self {
        soroban_sdk::Error::from_contract_error(e.to_u32())
    }
}

impl std::convert::TryFrom<soroban_sdk::Error> for Error {
    type Error = soroban_sdk::Error;
    fn try_from(error: soroban_sdk::Error) -> Result<Self, Self::Error> {
        if error.is_type(soroban_sdk::xdr::ScErrorType::Contract) {
            let code = error.get_code();
            if code >= 500 && code <= 541 {
                return Ok(Error::Feature(unsafe { core::mem::transmute(code) }));
            }
            if code >= 400 && code <= 406 {
                return Ok(Error::Proposal(unsafe { core::mem::transmute(code) }));
            }
            if code >= 300 && code <= 318 {
                return Ok(Error::Subscription(unsafe { core::mem::transmute(code) }));
            }
            if code >= 200 && code <= 224 {
                return Ok(Error::Payment(unsafe { core::mem::transmute(code) }));
            }
            if code >= 100 && code <= 126 {
                return Ok(Error::Basic(unsafe { core::mem::transmute(code) }));
            }
        }
        Err(error)
    }
}

impl std::convert::TryFrom<&soroban_sdk::Error> for Error {
    type Error = soroban_sdk::Error;
    #[inline(always)]
    fn try_from(error: &soroban_sdk::Error) -> Result<Self, Self::Error> {
        <_ as std::convert::TryFrom<soroban_sdk::Error>>::try_from(*error)
    }
}

impl From<Error> for soroban_sdk::InvokeError {
    #[inline(always)]
    fn from(val: Error) -> soroban_sdk::InvokeError {
        <_ as From<&Error>>::from(&val)
    }
}

impl From<&Error> for soroban_sdk::InvokeError {
    fn from(e: &Error) -> Self {
        soroban_sdk::InvokeError::Contract(e.to_u32())
    }
}

impl std::convert::TryFrom<soroban_sdk::InvokeError> for Error {
    type Error = soroban_sdk::InvokeError;
    #[inline(always)]
    fn try_from(error: soroban_sdk::InvokeError) -> Result<Self, soroban_sdk::InvokeError> {
        match error {
            soroban_sdk::InvokeError::Abort => Err(error),
            soroban_sdk::InvokeError::Contract(code) => {
                soroban_sdk::Error::from_contract_error(code)
                    .try_into()
                    .map_err(|_| error)
            }
        }
    }
}

impl std::convert::TryFrom<&soroban_sdk::InvokeError> for Error {
    type Error = soroban_sdk::InvokeError;
    #[inline(always)]
    fn try_from(error: &soroban_sdk::InvokeError) -> Result<Self, soroban_sdk::InvokeError> {
        <_ as std::convert::TryFrom<soroban_sdk::InvokeError>>::try_from(*error)
    }
}

impl TryFromVal<soroban_sdk::Env, Error> for Val {
    type Error = soroban_sdk::ConversionError;
    #[inline(always)]
    fn try_from_val(
        _env: &soroban_sdk::Env,
        val: &Error,
    ) -> Result<Self, soroban_sdk::ConversionError> {
        let error: soroban_sdk::Error = (*val).into();
        Ok(error.into())
    }
}

impl TryFromVal<soroban_sdk::Env, &Error> for Val {
    type Error = soroban_sdk::ConversionError;
    #[inline(always)]
    fn try_from_val(
        env: &soroban_sdk::Env,
        val: &&Error,
    ) -> Result<Self, soroban_sdk::ConversionError> {
        <_ as TryFromVal<soroban_sdk::Env, Error>>::try_from_val(env, *val)
    }
}

// ============================================================================
// TEST ERROR ENUM (for testing only)
// ============================================================================

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TestError {
    PaymentNotFound = 1,
    InvalidStatus = 2,
    AlreadyProcessed = 3,
    Unauthorized = 4,
    PaymentExpired = 5,
    NotExpired = 6,
    NoExpiration = 7,
    TransferFailed = 8,
    MetadataTooLarge = 9,
    NotesTooLarge = 10,
    InvalidCurrency = 11,
    RefundExceedsPayment = 12,
    SubscriptionNotFound = 13,
    SubscriptionNotActive = 14,
    PaymentNotDue = 15,
    MaxRetriesExceeded = 16,
    SubscriptionEnded = 17,
    InvalidBatchSize = 18,
    BatchPartialFailure = 19,
    RateLimitExceeded = 20,
    DailyVolumeExceeded = 21,
    AddressFlagged = 60,
    AddressAlreadyFlagged = 61,
    AmountExceedsLimit = 23,
    DunningNotFound = 24,
    SubscriptionNotInDunning = 25,
    RetryNotDue = 26,
    GracePeriodExpired = 27,
    EscrowMappingNotFound = 28,
    EscrowBridgeFailed = 29,
    MultiSigNotInitialized = 30,
    ProposalNotFound = 31,
    ProposalExpired = 32,
    ProposalAlreadyExecuted = 33,
    MultiSigThresholdNotMet = 34,
    InsufficientAdmins = 35,
    NotAnAdmin = 36,
    AlreadyApproved = 37,
    FeeConfigNotFound = 38,
    InsufficientFees = 39,
    ConditionNotMet = 40,
    ConditionAlreadyEvaluated = 41,
    OracleCallFailed = 42,
    ContractPaused = 43,
    FunctionPaused = 44,
    InvalidTierThresholds = 45,
    AutoEscrowRuleNotFound = 46,
    AutoEscrowBelowMinimum = 47,
    AutoEscrowAlreadyTriggered = 48,
    PaymentNotYetDue = 54,
    ScheduledPaymentCancelled = 55,
    OracleFeedStale = 58,
    OracleNotConfigured = 59,
    ConditionEvaluationFailed = 62,
    ConditionRuntimeNotMet = 63,
    RetryTooEarly = 56,
    PaymentRequiresMultiSig = 64,
    InsufficientPaymentApprovals = 65,
    PaymentProposalExpired = 66,
    MetadataAlreadySet = 67,
    MetadataNotFound = 68,
    HashMismatch = 69,
    PaymentAlreadyFullyPaid = 52,
    InstallmentExceedsRemaining = 53,
    PartialPaymentNotFound = 70,
    MerchantRateLimitExceeded = 50,
    AmountRateLimitExceeded = 51,
    InvalidFeeConfig = 71,
    InvalidAmount = 72,
    ChannelNotFound = 73,
    InvalidSignature = 74,
    InvalidNonce = 75,
    ChannelClosed = 76,
    ChannelExpired = 77,
    ChannelNotExpired = 78,
    MeteredSubscriptionNotFound = 79,
    BillingCapExceeded = 80,
    InvalidSplitShares = 85,
    TooManyRecipients = 86,
    SplitConfigNotFound = 87,
    SplitAlreadyExecuted = 88,
    LoyaltyNotConfigured = 100,
    InsufficientPoints = 101,
    PointsExpired = 102,
    NothingToSweep = 114,
    SweepRecipientNotSet = 115,
    SpendLimitExceeded = 116,
    SpendLimitNotConfigured = 117,
    GroupNotFound = 118,
    SubscriptionAlreadyInGroup = 119,
    GroupSizeLimitExceeded = 120,
    SettlementNotReady = 121,
    FinalityConfigNotFound = 122,
    SettlementAlreadyFinalized = 123,
    VerificationLevelNotFound = 95,
    TierLimitsNotConfigured = 96,
    RebateThresholdNotMet = 106,
    RebateAlreadyClaimed = 107,
    RebateConfigNotFound = 108,
    PayoutScheduleNotFound = 89,
    PayoutNotYetDue = 90,
    NothingToSettle = 91,
    ForwardConfigNotFound = 109,
    ForwardLoop = 110,
    InvalidForwardBps = 111,
    BillingOverflow = 124,
    InvalidInterval = 125,
}
